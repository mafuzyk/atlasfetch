#![allow(dead_code)]

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;

use crate::config::Config;
use crate::info;
use crate::layout_engine::{self, Layout as EngineLayout};
use crate::widget::{Registry, Widget};

// ── Editor state ─────────────────────────────────────────────────────────

pub struct Editor {
    cfg: Config,
    info: crate::info::SysInfo,
    engine_layout: EngineLayout,
    registry: Registry,
    widget_order: Vec<String>,
    preview_lines: Vec<String>,
    ascii_art: String,
    term_width: u16,
    term_height: u16,
    dirty: bool,
    exit: bool,
}

impl Editor {
    pub fn new(cfg: Config) -> Result<Self> {
        let info = info::collect()?;
        let ascii_art = crate::ascii::load(&cfg)?;
        let engine_layout = EngineLayout::Classic;
        let registry = Registry::from_fields(&cfg.display.left, &cfg.display.right);

        let widget_order: Vec<String> = cfg.display.left.iter()
            .chain(cfg.display.right.iter())
            .filter(|f| f.enabled)
            .map(|f| f.field.clone())
            .collect();

        let (tw, th) = terminal::size()?;
        let mut ed = Self {
            cfg,
            info,
            engine_layout,
            registry,
            widget_order,
            preview_lines: Vec::new(),
            ascii_art,
            term_width: tw,
            term_height: th,
            dirty: true,
            exit: false,
        };
        ed.refresh_preview();
        Ok(ed)
    }

    fn refresh_preview(&mut self) {
        let tw = self.term_width as usize;
        let engine = layout_engine::engine_for(self.engine_layout);

        // Build widgets list in order
        let widgets: Vec<&dyn Widget> = self.widget_order.iter()
            .filter_map(|key| self.registry.get(key))
            .collect();

        let ascii_lines: Vec<String> = if self.engine_layout != EngineLayout::Minimal
            && self.engine_layout != EngineLayout::Compact {
            self.ascii_art.lines().map(|l| l.to_string()).collect()
        } else {
            Vec::new()
        };

        let output = engine.arrange(&widgets, &ascii_lines, &self.cfg, &self.info, tw);

        // Convert to styled preview lines
        self.preview_lines = render_layout_preview(&output, &ascii_lines, tw, self.term_height as usize);
        self.dirty = false;
    }
}

// ── Render preview to styled lines ───────────────────────────────────────

fn render_layout_preview(
    output: &layout_engine::LayoutOutput,
    ascii_lines: &[String],
    term_width: usize,
    max_height: usize,
) -> Vec<String> {
    use crate::theme::Color;
    use unicode_width::UnicodeWidthStr;
    let mut lines = Vec::new();
    let reset = "\x1b[0m";
    let bold = "\x1b[1m";

    // Title
    if !output.title.is_empty() {
        let title_color = Color::from_hex_opt("#FF9A98").unwrap_or(Color::new(255, 154, 152));
        lines.push(format!("{}  {}{}{}{}", title_color.fg_escape(), bold, output.title, reset, reset));
    }

    // Separator
    if !output.separator.is_empty() {
        let sep_color = Color::from_hex_opt("#9D85FF").unwrap_or(Color::new(157, 133, 255));
        lines.push(format!("{}  {}{}{}", sep_color.fg_escape(), output.separator, reset, reset));
    }

    // Logo block width for centering
    let logo_width = ascii_lines.iter()
        .map(|l| l.trim_end().width())
        .max()
        .unwrap_or(0);

    // Rows
    for row in &output.rows {
        let mut line = String::new();

        // Logo line
        if let Some(logo) = &row.logo_line {
            let center = term_width.saturating_sub(logo_width) / 2;
            line.push_str(&" ".repeat(center));
            // Use a neutral color for logo in preview
            line.push_str(logo);
        }

        // Left widget
        for w in &row.left_widgets {
            line.push_str(&w.ansi);
        }

        // Right widget
        for w in &row.right_widgets {
            let rv = line.width();
            let remaining = term_width.saturating_sub(rv + w.width);
            if remaining > 0 {
                line.push_str(&" ".repeat(remaining));
            }
            line.push_str(&w.ansi);
        }

        // Try to pad to term_width
        let vis = line.width();
        if vis < term_width {
            line.push_str(&" ".repeat(term_width.saturating_sub(vis)));
        }

        if !line.trim().is_empty() {
            lines.push(line);
        }

        if lines.len() >= max_height {
            break;
        }
    }

    lines
}

// ── TUI rendering ────────────────────────────────────────────────────────

fn render_editor(frame: &mut Frame, editor: &mut Editor) {
    let area = frame.area();
    let (left, right) = if area.width >= 120 {
        let chunks = ratatui::layout::Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        // Stack vertically on narrow terminals
        let chunks = ratatui::layout::Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        (chunks[0], chunks[1])
    };

    render_sidebar(frame, left, editor);
    render_preview_panel(frame, right, editor);
    render_footer(frame, area, editor);
}

fn render_sidebar(frame: &mut Frame, area: Rect, editor: &Editor) {
    let chunks = ratatui::layout::Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(4 + EngineLayout::all().len() as u16 * 3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(area);

    // ── Title ──
    let title = Paragraph::new("AtlasFetch Editor")
        .style(Style::default().fg(TuiColor::Rgb(255, 154, 152)).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(title, chunks[0]);

    // ── Layout selector ──
    let layout_block = Block::default()
        .title("Layout")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(157, 133, 255)));
    let layout_items: Vec<ListItem> = EngineLayout::all().iter().map(|l| {
        let prefix = if *l == editor.engine_layout { "▸ " } else { "  " };
        ListItem::new(Line::from(vec![
            Span::styled(prefix, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(l.name()),
        ]))
    }).collect();
    let layout_list = List::new(layout_items)
        .block(layout_block)
        .highlight_style(Style::default().fg(TuiColor::Rgb(255, 255, 255)));
    frame.render_widget(layout_list, chunks[1]);

    // ── Widget list ──
    let widget_block = Block::default()
        .title("Widgets")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(251, 255, 168)));
    let widget_items: Vec<ListItem> = editor.widget_order.iter().map(|key| {
        let w = editor.registry.get(key);
        let label = w.map(|w| format!("{} {}", w.icon(), w.label())).unwrap_or_else(|| key.clone());
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::raw(label),
        ]))
    }).collect();
    let widget_list = List::new(widget_items).block(widget_block);
    frame.render_widget(widget_list, chunks[2]);

    // ── Key hints ──
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("↑↓  ", Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        Span::raw("Select  "),
        Span::styled("Enter", Style::default().fg(TuiColor::Rgb(251, 255, 168))),
        Span::raw(" Layout  "),
        Span::styled("Q", Style::default().fg(TuiColor::Rgb(255, 102, 146))),
        Span::raw(" Quit"),
    ]))
    .style(Style::default().fg(TuiColor::Rgb(180, 180, 180)));
    frame.render_widget(hints, chunks[3]);
}

fn render_preview_panel(frame: &mut Frame, area: Rect, editor: &Editor) {
    let block = Block::default()
        .title("Preview")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255)));
    let inner = block.inner(area);

    frame.render_widget(block, area);

    // Render preview lines
    let lines: Vec<Line> = editor.preview_lines.iter().map(|l| {
        Line::from(Span::raw(l.clone()))
    }).collect();
    let preview = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(TuiColor::Rgb(200, 200, 200)));
    frame.render_widget(preview, inner);
}

fn render_footer(frame: &mut Frame, area: Rect, editor: &Editor) {
    // Bottom bar with current layout name
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} ", editor.engine_layout.name()), Style::default().fg(TuiColor::Rgb(157, 133, 255)).bg(TuiColor::Rgb(30, 30, 30))),
        Span::raw("  "),
        Span::styled(editor.engine_layout.description(), Style::default().fg(TuiColor::Rgb(130, 130, 130))),
    ]))
    .style(Style::default().bg(TuiColor::Rgb(20, 20, 20)));
    let footer_area = Rect::new(area.x, area.y + area.height.saturating_sub(1), area.width, 1);
    frame.render_widget(footer, footer_area);
}

// ── Event handling ───────────────────────────────────────────────────────

fn handle_event(editor: &mut Editor) -> Result<bool> {
    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
            return Ok(true);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                return Ok(false);
            }
            KeyCode::Up => {
                let layouts = EngineLayout::all();
                let idx = layouts.iter().position(|l| *l == editor.engine_layout).unwrap_or(0);
                if idx > 0 {
                    editor.engine_layout = layouts[idx - 1];
                    editor.dirty = true;
                }
            }
            KeyCode::Down => {
                let layouts = EngineLayout::all();
                let idx = layouts.iter().position(|l| *l == editor.engine_layout).unwrap_or(0);
                if idx + 1 < layouts.len() {
                    editor.engine_layout = layouts[idx + 1];
                    editor.dirty = true;
                }
            }
            KeyCode::Enter => {
                editor.dirty = true;
            }
            _ => {}
        }
    }
    if editor.dirty {
        editor.refresh_preview();
    }
    Ok(true)
}

// ── Main entry ───────────────────────────────────────────────────────────

pub fn run(cfg: &mut Config) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut editor = Editor::new(cfg.clone())?;

    let res = loop {
        terminal.draw(|frame| render_editor(frame, &mut editor))?;
        if !handle_event(&mut editor)? {
            break Ok(());
        }
    };

    // Copy editor state back to config if changed
    if editor.engine_layout != EngineLayout::Classic {
        // Layout selection is kept in the editor for now
        // Config will be extended to store layout preference
    }
    cfg.display.left = editor.widget_order.iter()
        .filter_map(|key| {
            let orig = editor.cfg.display.left.iter()
                .chain(editor.cfg.display.right.iter())
                .find(|f| f.field == *key)
                .cloned();
            orig
        })
        .collect();

    terminal::disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;

    res
}
