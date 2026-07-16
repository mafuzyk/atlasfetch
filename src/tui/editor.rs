#![allow(dead_code)]

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;
use unicode_width::UnicodeWidthStr;

use crate::ascii;
use crate::config::{Config, FieldDef};
use crate::info;
use crate::layout::AppLayout;
use crate::layout_engine::{self, Layout as EngineLayout};
use crate::render::StyledSegment;
use crate::theme::{self, Color};
use crate::widget::{FieldWidget, Registry, Widget};

// ── Tab enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Layout,
    Theme,
    Ascii,
    Panels,
    Save,
}

impl Tab {
    fn all() -> [Tab; 5] { [Tab::Layout, Tab::Theme, Tab::Ascii, Tab::Panels, Tab::Save] }
    fn label(&self) -> &'static str {
        match self {
            Tab::Layout => " Layout ",
            Tab::Theme  => " Theme ",
            Tab::Ascii  => " ASCII ",
            Tab::Panels => " Panels ",
            Tab::Save   => " Save ",
        }
    }
    fn next(&self) -> Self { match self { Tab::Layout => Tab::Theme, Tab::Theme => Tab::Ascii, Tab::Ascii => Tab::Panels, Tab::Panels => Tab::Save, Tab::Save => Tab::Layout } }
    fn prev(&self) -> Self { match self { Tab::Layout => Tab::Save, Tab::Save => Tab::Panels, Tab::Panels => Tab::Ascii, Tab::Ascii => Tab::Theme, Tab::Theme => Tab::Layout } }
}

// ── Input sub-modes ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,
    EditingCustomPalette,
    EditingLabel,
    AddingPanel,
    EditingHexColor(usize),
    PastingAscii,
    BrowsingFile,
}

// ── Editor state ─────────────────────────────────────────────────────────

pub struct Editor {
    cfg: Config,
    info: crate::info::SysInfo,
    tab: Tab,
    app_layout: AppLayout,
    // Theme
    themes: Vec<theme::Theme>,
    theme_selected: usize,
    custom_palette_input: String,
    // ASCII
    logo_keys: Vec<String>,
    ascii_art: String,
    ascii_source: String, // "builtin:key" | "file:path" | "pasted" | "disabled"
    // Panels
    panel_focus: bool, // false = left, true = right
    panel_left_sel: usize,
    panel_right_sel: usize,
    add_panel_available: Vec<(String, String, String)>, // (key, icon, label)
    add_panel_sel: usize,
    editing_label_input: String,
    // File browser
    file_browser_cwd: std::path::PathBuf,
    file_browser_entries: Vec<(String, bool)>,
    file_browser_sel: usize,
    // Layout selection
    layout_selected: usize,
    // ASCII selection
    ascii_selected: usize,
    // General
    input_mode: InputMode,
    paste_buffer: String,
    saved: bool,
    preview_lines: Vec<Line<'static>>,
    term_width: u16,
    term_height: u16,
    dirty: bool,
}

impl Editor {
    pub fn new(cfg: Config) -> Result<Self> {
        let info = info::collect()?;
        let logo_keys = ascii::available_logos()?;
        let ascii_art = ascii::load(&cfg)?;

        // Determine initial ASCII source
        let ascii_source = if cfg.logo.key.is_empty() {
            if cfg.logo.path.to_lowercase() == "disabled" {
                "disabled".into()
            } else {
                format!("file:{}", cfg.logo.path)
            }
        } else {
            format!("builtin:{}", cfg.logo.key)
        };

        // Detect app layout from config
        let app_layout = AppLayout::Centered; // default

        let themes = theme::all_themes();
        let theme_selected = themes.iter().position(|t| t.colors == cfg.logo.colors).unwrap_or(0);

        let available = if info::is_android() {
            vec![
                ("device","\u{f109}","Device"),("os","\u{f17c}","OS"),("rom","\u{f0c6}","ROM"),
                ("soc","\u{f2db}","SoC"),("arch","\u{f17c}","Arch"),("kernel","\u{e271}","Krn"),
                ("battery_level","\u{f0e7}","Bat"),("battery_temp","\u{f2c7}","Temp"),
                ("battery_health","\u{f004}","Health"),("battery_status","\u{f0e7}","Charge"),
                ("memory","\u{f1c0}","RAM"),("storage","\u{f0a0}","Stor"),
                ("cpu","\u{f2db}","CPU"),("gpu","\u{f26c}","GPU"),
                ("cpu_temp","\u{f2c7}","CPU Temp"),("uptime","\u{f017}","Up"),
                ("packages","\u{f1b3}","Pkg"),("root_status","\u{f023}","Root"),
                ("bootloader","\u{f085}","Bootloader"),("selinux","\u{f023}","SELinux"),
                ("resolution","\u{f108}","Res"),("brightness","\u{f185}","Brightness"),
                ("refresh_rate","\u{f26c}","Refresh"),("signal","\u{f012}","Signal"),
                ("wifi_ssid","\u{f1eb}","WiFi"),("security_patch","\u{f021}","Patch"),
                ("uptime_days","\u{f017}","Uptime"),("shell","\u{f489}","Sh"),
            ]
        } else {
            vec![
                ("os","\u{f17c}","OS"),("host","\u{f109}","Host"),("user","\u{f007}","Usr"),
                ("kernel","\u{e271}","Krn"),("uptime","\u{f017}","Up"),("packages","\u{f1b3}","Pkg"),
                ("shell","\u{f489}","Sh"),("terminal","\u{f120}","Term"),("cpu","\u{f2db}","CPU"),
                ("gpu","\u{f26c}","GPU"),("memory","\u{f1c0}","Mem"),("disk","\u{f0a0}","Dsk"),
                ("wm","\u{f108}","WM"),("load","\u{f0e7}","Load"),("processes","\u{f013}","Proc"),
                ("local_ip","\u{f0c1}","IP"),("resolution","\u{f108}","Res"),("de","\u{f11b}","DE"),
                ("font","\u{f031}","Font"),("vram","\u{f26c}","VRAM"),("flatpak","\u{f2d8}","Flatpak"),
                ("snap","\u{f1b3}","Snap"),
            ]
        };

        let (tw, th) = terminal::size()?;
        let layout_selected = AppLayout::pc_variants().iter().position(|l| *l == app_layout).unwrap_or(0);
        let ascii_selected = match ascii_source.split_once(':') {
            Some(("builtin", k)) => logo_keys.iter().position(|lk| lk == k).unwrap_or(0),
            _ if ascii_source.starts_with("file:") => logo_keys.len(),
            _ if ascii_source == "pasted" => logo_keys.len() + 1,
            _ => logo_keys.len() + 2,
        };
        let mut ed = Self {
            cfg, info, tab: Tab::Layout, app_layout, layout_selected,
            themes, theme_selected, custom_palette_input: String::new(),
            logo_keys, ascii_art, ascii_source, ascii_selected,
            panel_focus: false, panel_left_sel: 0, panel_right_sel: 0,
            add_panel_available: available.into_iter().map(|(k, i, l)| (k.to_string(), i.to_string(), l.to_string())).collect(),
            add_panel_sel: 0,
            editing_label_input: String::new(),
            file_browser_cwd: std::env::current_dir().unwrap_or_else(|_| "/".into()),
            file_browser_entries: Vec::new(),
            file_browser_sel: 0,
            input_mode: InputMode::Normal,
            paste_buffer: String::new(),
            saved: false,
            preview_lines: Vec::new(),
            term_width: tw, term_height: th, dirty: true,
        };
        ed.refresh_file_browser();
        ed.refresh_preview();
        Ok(ed)
    }

    fn apply_layout(&mut self, idx: usize) {
        let layouts = AppLayout::pc_variants();
        if idx < layouts.len() {
            let l = layouts[idx];
            self.app_layout = l;
            self.cfg.panel.gap = l.gap();
            self.cfg.panel.left_pad = l.padding();
            self.cfg.panel.right_pad = l.padding();
            self.cfg.panel.max_val_width = l.max_panel_width();
            self.dirty = true;
        }
    }

    fn refresh_file_browser(&mut self) {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&self.file_browser_cwd) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                entries.push((name, is_dir));
            }
        }
        entries.sort_by(|a, b| { if a.1 != b.1 { b.1.cmp(&a.1) } else { a.0.cmp(&b.0) } });
        self.file_browser_entries = entries;
    }

    fn refresh_preview(&mut self) {
        let tw = self.term_width as usize;
        let engine_layout = match self.app_layout {
            AppLayout::Minimal => EngineLayout::Minimal,
            _ => EngineLayout::Classic,
        };
        let engine = layout_engine::engine_for(engine_layout);

        let mut reg = Registry::new();
        for fd in self.cfg.display.left.iter().chain(self.cfg.display.right.iter()) {
            if fd.enabled {
                reg.register(Box::new(FieldWidget::from_def(fd.clone())));
            }
        }

        let left_widgets: Vec<&dyn Widget> = self.cfg.display.left.iter()
            .filter(|f| f.enabled)
            .filter_map(|fd| reg.get(&fd.field))
            .collect();

        let right_widgets: Vec<&dyn Widget> = self.cfg.display.right.iter()
            .filter(|f| f.enabled)
            .filter_map(|fd| reg.get(&fd.field))
            .collect();

        let show_ascii = self.app_layout != AppLayout::Minimal && self.ascii_source != "disabled";
        let ascii_lines: Vec<String> = if show_ascii {
            self.ascii_art.lines().map(|l| l.to_string()).collect()
        } else {
            Vec::new()
        };

        let output = engine.arrange(&left_widgets, &right_widgets, &ascii_lines, &self.cfg, &self.info, tw);

        let mut lines: Vec<Line> = Vec::new();
        let title_color = tui_color(&Color::from_hex_opt(&self.cfg.title.color).unwrap_or(Color::new(255, 154, 152)));
        if !output.title.is_empty() {
            lines.push(Line::from(vec![Span::raw("  "), Span::styled(output.title.clone(), Style::default().fg(title_color).add_modifier(Modifier::BOLD))]));
        }
        if !output.separator.is_empty() {
            let sep_color = tui_color(&Color::from_hex_opt(&self.cfg.separator.color).unwrap_or(Color::new(157, 133, 255)));
            lines.push(Line::from(vec![Span::raw("  "), Span::styled(output.separator.clone(), Style::default().fg(sep_color))]));
        }

        let logo_width = ascii_lines.iter().map(|l| l.trim_end().width()).max().unwrap_or(0);
        let logo_origin = if logo_width > 0 && logo_width < tw {
            (tw.saturating_sub(logo_width)) / 2
        } else {
            0
        };

        for row in &output.rows {
            let mut spans: Vec<Span> = Vec::new();

            let right_vis = row.right_widgets.iter().map(|w| w.width).sum::<usize>();
            let has_logo = row.logo_line.is_some();

            if has_logo {
                // Left widgets before logo
                for w in &row.left_widgets {
                    spans.extend(styled_to_spans(&w.styled));
                }
                // Gap to logo center
                let cur: usize = spans.iter().map(|s| s.content.width()).sum();
                if logo_origin > cur {
                    spans.push(Span::raw(" ".repeat(logo_origin - cur)));
                }
                // Logo
                if let Some(logo) = &row.logo_line {
                    spans.push(Span::raw(logo.clone()));
                }
                // Gap from logo end to right widgets
                let cur: usize = spans.iter().map(|s| s.content.width()).sum();
                let right_target = tw.saturating_sub(right_vis);
                if right_target > cur {
                    spans.push(Span::raw(" ".repeat(right_target - cur)));
                }
                for w in &row.right_widgets {
                    spans.extend(styled_to_spans(&w.styled));
                }
            } else {
                // No logo: left widgets on left, right widgets on right
                for w in &row.left_widgets {
                    spans.extend(styled_to_spans(&w.styled));
                }
                let cur: usize = spans.iter().map(|s| s.content.width()).sum();
                let right_target = tw.saturating_sub(right_vis);
                if right_target > cur {
                    spans.push(Span::raw(" ".repeat(right_target - cur)));
                }
                for w in &row.right_widgets {
                    spans.extend(styled_to_spans(&w.styled));
                }
            }

            // Pad to full width
            let cur: usize = spans.iter().map(|s| s.content.width()).sum();
            if cur < tw {
                spans.push(Span::raw(" ".repeat(tw - cur)));
            }

            if !spans.is_empty() {
                lines.push(Line::from(spans));
            }
            if lines.len() >= self.term_height as usize { break; }
        }
        self.preview_lines = lines;
        self.dirty = false;
    }
}

// ── Color helpers ────────────────────────────────────────────────────────

fn tui_color(c: &Color) -> TuiColor { TuiColor::Rgb(c.r, c.g, c.b) }

fn styled_to_spans(segs: &[StyledSegment]) -> Vec<Span<'static>> {
    segs.iter().map(|s| {
        let mut st = Style::default();
        if let Some(fg) = &s.fg { st = st.fg(tui_color(fg)); }
        if let Some(bg) = &s.bg { st = st.bg(tui_color(bg)); }
        if s.bold { st = st.add_modifier(Modifier::BOLD); }
        Span::styled(s.text.clone(), st)
    }).collect()
}

// ── Main UI ──────────────────────────────────────────────────────────────

fn render_editor(frame: &mut Frame, editor: &mut Editor) {
    let area = frame.area();
    let (content, preview) = if area.width >= 100 {
        let c = ratatui::layout::Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
        (c[0], c[1])
    } else {
        let c = ratatui::layout::Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
        (c[0], c[1])
    };

    if editor.input_mode != InputMode::Normal {
        render_overlay(frame, area, editor);
    } else {
        render_sidebar(frame, content, editor);
        render_preview_panel(frame, preview, editor);
    }
}

fn render_overlay(frame: &mut Frame, area: Rect, editor: &Editor) {
    // Simple overlay for input modes
    let text = match editor.input_mode {
        InputMode::EditingCustomPalette => {
            let parsed: Vec<Color> = editor.custom_palette_input.split_whitespace()
                .filter_map(|s| Color::from_hex_opt(s)).collect();
            let mut spans = vec![Span::raw("Hex colors: ")];
            for c in &parsed { spans.push(Span::styled("  ", Style::default().bg(tui_color(c)))); }
            spans.push(Span::raw(format!(" {}", editor.custom_palette_input)));
            Text::from(vec![
                Line::from(spans),
                Line::from("Type space-separated hex colors, Enter to apply, Esc to cancel"),
            ])
        }
        InputMode::EditingLabel => Text::from(vec![
            Line::from(Span::raw(format!("Label: {}", editor.editing_label_input))),
            Line::from("Enter to save, Esc to cancel"),
        ]),
        InputMode::AddingPanel => {
            let items: Vec<ListItem> = editor.add_panel_available.iter().enumerate().map(|(i, (k, _, l))| {
                let prefix = if i == editor.add_panel_sel { "▸ " } else { "  " };
                ListItem::new(format!("{}{} ({})", prefix, l, k))
            }).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Add Field"))
                .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));
            frame.render_widget(Clear, area);
            frame.render_widget(list, area);
            return;
        }
        InputMode::PastingAscii => Text::from(vec![
            Line::from(Span::raw(format!("Buffer ({} chars):", editor.paste_buffer.len()))),
            Line::from(Span::raw(editor.paste_buffer.clone())),
            Line::from("Enter to apply, Esc to cancel"),
        ]),
        InputMode::BrowsingFile => {
            let items: Vec<ListItem> = editor.file_browser_entries.iter().map(|(name, is_dir)| {
                let prefix = if *is_dir { "📁 " } else { "📄 " };
                ListItem::new(format!("{}{}", prefix, name))
            }).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("{:?}", editor.file_browser_cwd)))
                .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));
            frame.render_widget(Clear, area);
            frame.render_widget(list, area);
            return;
        }
        InputMode::EditingHexColor(_) => Text::from(vec![
            Line::from("Edit hex color value (e.g. #FF6692)"),
        ]),
        InputMode::Normal => unreachable!(),
    };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded);
    let p = Paragraph::new(text).block(block);
    frame.render_widget(Clear, area);
    frame.render_widget(p, area);
}

// ── Sidebar ──────────────────────────────────────────────────────────────

fn render_sidebar(frame: &mut Frame, area: Rect, editor: &Editor) {
    let v = ratatui::layout::Layout::vertical([Constraint::Length(3), Constraint::Fill(1), Constraint::Length(3)]);
    let c = v.split(area);
    render_tabs(frame, c[0], editor);
    render_tab_content(frame, c[1], editor);
    render_hints(frame, c[2], editor);
}

fn render_tabs(frame: &mut Frame, area: Rect, editor: &Editor) {
    let tabs: Vec<Span> = Tab::all().iter().map(|t| {
        let active = *t == editor.tab;
        Span::styled(t.label(), if active { Style::default().fg(TuiColor::Rgb(157, 133, 255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(120, 120, 120)) })
    }).collect();
    frame.render_widget(Paragraph::new(Line::from(tabs)).style(Style::default().bg(TuiColor::Rgb(20, 20, 20))), area);
}

fn render_hints(frame: &mut Frame, area: Rect, editor: &Editor) {
    let text = match editor.tab {
        Tab::Layout => " ↑↓ layout  Tab next  q quit",
        Tab::Theme  => " ↑↓ theme  Enter apply  c custom palette  Tab next  q quit",
        Tab::Ascii  => " ↑↓ logo  Enter select  c custom file  p paste  d disable  Tab next  q quit",
        Tab::Panels => " ↑↓ nav  Space toggle  [/] panel  a add  d delete  r reorder  e edit label Tab next  q quit",
        Tab::Save   => " Enter save  q discard",
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, Style::default().fg(TuiColor::Rgb(140, 140, 140))))).style(Style::default().bg(TuiColor::Rgb(15, 15, 15))), area);
}

fn render_tab_content(frame: &mut Frame, area: Rect, editor: &Editor) {
    match editor.tab {
        Tab::Layout => render_layout_tab(frame, area, editor),
        Tab::Theme  => render_theme_tab(frame, area, editor),
        Tab::Ascii  => render_ascii_tab(frame, area, editor),
        Tab::Panels => render_panels_tab(frame, area, editor),
        Tab::Save   => render_save_tab(frame, area, editor),
    }
}

// ── Layout tab ───────────────────────────────────────────────────────────

fn render_layout_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let layouts = AppLayout::pc_variants();
    let items: Vec<ListItem> = layouts.iter().enumerate().map(|(i, l)| {
        let sel = i == editor.layout_selected;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::styled(l.name(), if sel { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
            Span::raw("  "),
            Span::styled(l.description(), Style::default().fg(TuiColor::Rgb(120,120,120))),
        ]))
    }).collect();
    frame.render_widget(List::new(items).block(Block::default().title("Layout").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(157, 133, 255)))), area);
}

// ── Theme tab ────────────────────────────────────────────────────────────

fn render_theme_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let top = ratatui::layout::Layout::vertical([Constraint::Fill(1), Constraint::Length(4)]).split(area);

    let items: Vec<ListItem> = editor.themes.iter().enumerate().map(|(i, t)| {
        let swatch: Vec<Span> = t.colors.iter().map(|c| Span::styled("  ", Style::default().bg(tui_color(c)))).collect();
        let mut line = vec![
            Span::styled(if i == editor.theme_selected { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(format!("{:15}", t.name)),
        ];
        line.extend(swatch);
        ListItem::new(Line::from(line))
    }).collect();
    frame.render_widget(List::new(items).block(Block::default().title("Theme Presets").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152)))), top[0]);

    let swatch: Vec<Span> = editor.cfg.logo.colors.iter().map(|c| Span::styled("  ", Style::default().bg(tui_color(c)))).collect();
    frame.render_widget(Paragraph::new(Line::from(swatch)).block(Block::default().title("Current Palette").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152)))), top[1]);
}

// ── ASCII tab ────────────────────────────────────────────────────────────

fn render_ascii_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let items: Vec<ListItem> = editor.logo_keys.iter().enumerate().map(|(i, key)| {
        let sel = i == editor.ascii_selected;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::styled(key.clone(), if sel { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
        ]))
    }).collect();
    let mut items = items;
    let n = editor.logo_keys.len();
    // Custom file entry
    let has_file = editor.ascii_selected == n;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if has_file { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        Span::styled("[ Custom file ]", if has_file { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));
    // Pasted entry
    let is_pasted = editor.ascii_selected == n + 1;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if is_pasted { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        Span::styled("[ Paste ASCII ]", if is_pasted { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));
    // Disabled entry
    let disabled = editor.ascii_selected == n + 2;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if disabled { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        Span::styled("[ Disabled ]", if disabled { Style::default().fg(TuiColor::Rgb(255,102,146)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));
    frame.render_widget(List::new(items).block(Block::default().title("ASCII Art").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(255, 184, 131)))), area);
}

// ── Panels tab ───────────────────────────────────────────────────────────

fn render_panels_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let c = ratatui::layout::Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    // Left panel
    let left_items: Vec<ListItem> = editor.cfg.display.left.iter().enumerate().map(|(i, f)| {
        let check = if f.enabled { "✓" } else { " " };
        let sel = !editor.panel_focus && editor.panel_left_sel == i;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(format!(" [{}] {} {}", check, f.icon, f.label)),
        ]))
    }).collect();
    frame.render_widget(List::new(left_items)
        .block(Block::default()
            .title(if !editor.panel_focus { "Left [focused]" } else { "Left" })
            .borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(if !editor.panel_focus { Style::default().fg(TuiColor::Rgb(133, 188, 255)) } else { Style::default().fg(TuiColor::Rgb(80, 80, 80)) })
        ), c[0]);

    // Right panel
    let right_items: Vec<ListItem> = editor.cfg.display.right.iter().enumerate().map(|(i, f)| {
        let check = if f.enabled { "✓" } else { " " };
        let sel = editor.panel_focus && editor.panel_right_sel == i;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(format!(" [{}] {} {}", check, f.icon, f.label)),
        ]))
    }).collect();
    frame.render_widget(List::new(right_items)
        .block(Block::default()
            .title(if editor.panel_focus { "Right [focused]" } else { "Right" })
            .borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(if editor.panel_focus { Style::default().fg(TuiColor::Rgb(133, 188, 255)) } else { Style::default().fg(TuiColor::Rgb(80, 80, 80)) })
        ), c[1]);
}

// ── Save tab ─────────────────────────────────────────────────────────────

fn render_save_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let theme_name = editor.themes.iter()
        .find(|t| t.colors == editor.cfg.logo.colors)
        .map(|t| t.name)
        .unwrap_or("custom");

    let n_enabled = editor.cfg.display.left.iter().chain(editor.cfg.display.right.iter()).filter(|f| f.enabled).count();
    let ascii_info = match editor.ascii_source.split_once(':') {
        Some(("builtin", k)) => format!("Built-in: {}", k),
        Some(("file", p)) => format!("File: {}", p),
        Some(("pasted", _)) => "Pasted ASCII".into(),
        _ => "Disabled".into(),
    };

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(Span::styled("  Configuration Summary", Style::default().fg(TuiColor::Rgb(133, 188, 255)).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(format!("  Layout:    {}", editor.app_layout.name())),
            Line::from(format!("  Theme:     {}", theme_name)),
            Line::from(format!("  ASCII:     {}", ascii_info)),
            Line::from(format!("  Fields:    {} enabled", n_enabled)),
            Line::from(format!("  Config:    ~/.config/atlasfetch/config.json")),
            Line::from(""),
            Line::from(Span::styled("  Enter — Save & Exit", Style::default().fg(TuiColor::Rgb(133, 188, 255)))),
            Line::from(Span::styled("  q     — Discard & Exit", Style::default().fg(TuiColor::Rgb(255, 102, 146)))),
        ]))
        .block(Block::default().title("Save").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255)))),
        area);
}

// ── Preview ──────────────────────────────────────────────────────────────

fn render_preview_panel(frame: &mut Frame, area: Rect, editor: &Editor) {
    let block = Block::default().title("Preview").borders(Borders::ALL).border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255)));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(Text::from(editor.preview_lines.clone())).style(Style::default().fg(TuiColor::Rgb(200, 200, 200))), inner);
}

// ── Event handling ───────────────────────────────────────────────────────

fn handle_event(editor: &mut Editor) -> Result<bool> {
    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press { return Ok(true); }

        // Handle input modes first
        match editor.input_mode {
            InputMode::EditingCustomPalette => {
                match key.code {
                    KeyCode::Enter => {
                        let parsed: Vec<Color> = editor.custom_palette_input.split_whitespace()
                            .filter_map(|s| Color::from_hex_opt(s)).collect();
                        if !parsed.is_empty() {
                            editor.cfg.logo.colors = parsed;
                            editor.dirty = true;
                        }
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc => { editor.input_mode = InputMode::Normal; }
                    KeyCode::Char(c) => { editor.custom_palette_input.push(c); }
                    KeyCode::Backspace => { editor.custom_palette_input.pop(); }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::EditingLabel => {
                match key.code {
                    KeyCode::Enter => {
                        let fields = if editor.panel_focus { &mut editor.cfg.display.right } else { &mut editor.cfg.display.left };
                        let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                        if idx < fields.len() {
                            fields[idx].label = editor.editing_label_input.clone();
                            editor.dirty = true;
                        }
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc => { editor.input_mode = InputMode::Normal; }
                    KeyCode::Char(c) => { editor.editing_label_input.push(c); }
                    KeyCode::Backspace => { editor.editing_label_input.pop(); }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::AddingPanel => {
                match key.code {
                    KeyCode::Up => { editor.add_panel_sel = editor.add_panel_sel.saturating_sub(1); }
                    KeyCode::Down => { editor.add_panel_sel = (editor.add_panel_sel + 1).min(editor.add_panel_available.len().saturating_sub(1)); }
                    KeyCode::Enter => {
                        if editor.add_panel_sel < editor.add_panel_available.len() {
                            let (k, i, l) = &editor.add_panel_available[editor.add_panel_sel];
                            let fd = FieldDef { field: k.clone(), icon: i.clone(), label: l.clone(), enabled: true };
                            // Check if already exists in either panel
                            let exists = editor.cfg.display.left.iter().chain(editor.cfg.display.right.iter()).any(|f| f.field == fd.field);
                            if !exists {
                                editor.cfg.display.left.push(fd);
                                editor.dirty = true;
                            }
                        }
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc => { editor.input_mode = InputMode::Normal; }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::PastingAscii => {
                match key.code {
                    KeyCode::Enter => {
                        if !editor.paste_buffer.is_empty() {
                            editor.ascii_art = editor.paste_buffer.clone();
                            editor.ascii_source = "pasted".into();
                            editor.dirty = true;
                        }
                        editor.paste_buffer.clear();
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc => {
                        editor.paste_buffer.clear();
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => { editor.paste_buffer.push(c); }
                    KeyCode::Backspace => { editor.paste_buffer.pop(); }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::BrowsingFile => {
                match key.code {
                    KeyCode::Up => { editor.file_browser_sel = editor.file_browser_sel.saturating_sub(1); }
                    KeyCode::Down => { editor.file_browser_sel = (editor.file_browser_sel + 1).min(editor.file_browser_entries.len().saturating_sub(1)); }
                    KeyCode::Char('~') => {
                        editor.file_browser_cwd = std::env::var("HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| "/".into());
                        editor.refresh_file_browser();
                        editor.file_browser_sel = 0;
                    }
                    KeyCode::Backspace => {
                        if editor.file_browser_cwd.pop() {
                            editor.refresh_file_browser();
                            editor.file_browser_sel = 0;
                        }
                    }
                    KeyCode::Enter => {
                        if editor.file_browser_sel < editor.file_browser_entries.len() {
                            let (name, is_dir) = &editor.file_browser_entries[editor.file_browser_sel];
                            if *is_dir {
                                editor.file_browser_cwd.push(name);
                                editor.refresh_file_browser();
                                editor.file_browser_sel = 0;
                            } else {
                                let path = editor.file_browser_cwd.join(name);
                                let path_str = path.to_string_lossy().to_string();
                                editor.cfg.logo.path = path_str.clone();
                                editor.ascii_source = format!("file:{}", path_str);
                                if let Ok(art) = ascii::load(&editor.cfg) {
                                    editor.ascii_art = art;
                                }
                                editor.dirty = true;
                                editor.input_mode = InputMode::Normal;
                            }
                        }
                    }
                    KeyCode::Esc => { editor.input_mode = InputMode::Normal; }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::EditingHexColor(_) => {
                match key.code {
                    KeyCode::Esc => { editor.input_mode = InputMode::Normal; }
                    _ => { editor.input_mode = InputMode::Normal; }
                }
                return Ok(true);
            }
            InputMode::Normal => {}
        }

        // Normal mode key handling
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if editor.tab == Tab::Save { return Ok(false); }
                return Ok(false);
            }
            KeyCode::Tab => { editor.tab = editor.tab.next(); }
            KeyCode::BackTab => { editor.tab = editor.tab.prev(); }
            KeyCode::Up => {
                match editor.tab {
                    Tab::Layout => {
                        editor.layout_selected = editor.layout_selected.saturating_sub(1);
                    }
                    Tab::Theme => {
                        editor.theme_selected = editor.theme_selected.saturating_sub(1);
                    }
                    Tab::Ascii => {
                        editor.ascii_selected = editor.ascii_selected.saturating_sub(1);
                    }
                    Tab::Panels => {
                        if editor.panel_focus {
                            editor.panel_right_sel = editor.panel_right_sel.saturating_sub(1);
                        } else {
                            editor.panel_left_sel = editor.panel_left_sel.saturating_sub(1);
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down => {
                match editor.tab {
                    Tab::Layout => {
                        let max = AppLayout::pc_variants().len().saturating_sub(1);
                        editor.layout_selected = (editor.layout_selected + 1).min(max);
                    }
                    Tab::Theme => {
                        let max = editor.themes.len().saturating_sub(1);
                        editor.theme_selected = (editor.theme_selected + 1).min(max);
                    }
                    Tab::Ascii => {
                        let max = editor.logo_keys.len() + 2; // builtins + file + paste + disable
                        editor.ascii_selected = (editor.ascii_selected + 1).min(max);
                    }
                    Tab::Panels => {
                        let max = if editor.panel_focus {
                            editor.cfg.display.right.len().saturating_sub(1)
                        } else {
                            editor.cfg.display.left.len().saturating_sub(1)
                        };
                        if editor.panel_focus {
                            editor.panel_right_sel = (editor.panel_right_sel + 1).min(max);
                        } else {
                            editor.panel_left_sel = (editor.panel_left_sel + 1).min(max);
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Enter => {
                match editor.tab {
                    Tab::Layout => {
                        editor.apply_layout(editor.layout_selected);
                    }
                    Tab::Theme => {
                        // Apply selected theme
                        if editor.theme_selected < editor.themes.len() {
                            editor.cfg.logo.colors = editor.themes[editor.theme_selected].colors.clone();
                            editor.dirty = true;
                        }
                    }
                    Tab::Ascii => {
                        let n = editor.logo_keys.len();
                        if editor.ascii_selected < n {
                            let key = &editor.logo_keys[editor.ascii_selected];
                            editor.ascii_source = format!("builtin:{}", key);
                            editor.cfg.logo.key = key.clone();
                            if let Ok(art) = ascii::load(&editor.cfg) {
                                editor.ascii_art = art;
                            }
                            editor.dirty = true;
                        } else if editor.ascii_selected == n {
                            editor.input_mode = InputMode::BrowsingFile;
                            editor.file_browser_sel = 0;
                            editor.refresh_file_browser();
                        } else if editor.ascii_selected == n + 1 {
                            editor.input_mode = InputMode::PastingAscii;
                        } else if editor.ascii_selected == n + 2 {
                            editor.ascii_source = "disabled".into();
                            editor.cfg.logo.key = String::new();
                            editor.cfg.logo.path = "disabled".into();
                            editor.ascii_art = String::new();
                            editor.dirty = true;
                        }
                    }
                    Tab::Panels => {
                        // Toggle enabled on focused panel's selected field
                        let fields = if editor.panel_focus { &mut editor.cfg.display.right } else { &mut editor.cfg.display.left };
                        let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                        if idx < fields.len() {
                            fields[idx].enabled = !fields[idx].enabled;
                            editor.dirty = true;
                        }
                    }
                    Tab::Save => {
                        editor.saved = true;
                        return Ok(false); // save and exit
                    }
                }
            }
            KeyCode::Char(' ') => {
                if editor.tab == Tab::Panels {
                    let fields = if editor.panel_focus { &mut editor.cfg.display.right } else { &mut editor.cfg.display.left };
                    let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                    if idx < fields.len() {
                        fields[idx].enabled = !fields[idx].enabled;
                        editor.dirty = true;
                    }
                }
            }
            KeyCode::Char('/') => {
                if editor.tab == Tab::Panels {
                    editor.panel_focus = !editor.panel_focus;
                }
            }
            KeyCode::Char('a') => {
                if editor.tab == Tab::Panels {
                    editor.input_mode = InputMode::AddingPanel;
                    editor.add_panel_sel = 0;
                }
            }
            KeyCode::Char('d') => {
                if editor.tab == Tab::Panels {
                    let fields = if editor.panel_focus { &mut editor.cfg.display.right } else { &mut editor.cfg.display.left };
                    let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                    if idx < fields.len() {
                        fields.remove(idx);
                        editor.dirty = true;
                    }
                }
            }
            KeyCode::Char('r') => {
                if editor.tab == Tab::Panels {
                    let fields = if editor.panel_focus { &mut editor.cfg.display.right } else { &mut editor.cfg.display.left };
                    let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                    if idx > 0 && idx < fields.len() {
                        fields.swap(idx, idx - 1);
                        if editor.panel_focus { editor.panel_right_sel = idx - 1; } else { editor.panel_left_sel = idx - 1; }
                        editor.dirty = true;
                    }
                }
            }
            KeyCode::Char('e') => {
                if editor.tab == Tab::Panels {
                    let fields = if editor.panel_focus { &editor.cfg.display.right } else { &editor.cfg.display.left };
                    let idx = if editor.panel_focus { editor.panel_right_sel } else { editor.panel_left_sel };
                    if idx < fields.len() {
                        editor.editing_label_input = fields[idx].label.clone();
                        editor.input_mode = InputMode::EditingLabel;
                    }
                }
            }
            KeyCode::Char('c') => {
                if editor.tab == Tab::Theme {
                    editor.custom_palette_input.clear();
                    editor.input_mode = InputMode::EditingCustomPalette;
                }
                if editor.tab == Tab::Ascii {
                    editor.input_mode = InputMode::BrowsingFile;
                    editor.file_browser_sel = 0;
                    editor.refresh_file_browser();
                }
            }
            KeyCode::Char('p') => {
                if editor.tab == Tab::Ascii {
                    editor.input_mode = InputMode::PastingAscii;
                }
            }
            _ => {}
        }
    }
    if editor.dirty { editor.refresh_preview(); }
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

    // Copy changes back if saved
    if editor.saved {
        cfg.logo = editor.cfg.logo.clone();
        cfg.panel = editor.cfg.panel.clone();
        cfg.display = editor.cfg.display.clone();
    }

    terminal::disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    res
}
