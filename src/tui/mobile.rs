// Mobile TUI setup — simplified configurator for Android/Termux.
//
// Fewer steps than the PC TUI: no ASCII/Theme selection, mobile-only fields.
// Full-width panel editor, one panel at a time. Uses render_mobile for preview.

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Style, Modifier};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, BorderType};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;

use crate::ascii;
use crate::config::{Config, FieldDef};
use crate::info;
use crate::layout::AppLayout;
use crate::render;

type TuiColor = ratatui::style::Color;

#[allow(dead_code)]
const RESET: &str = "\x1b[0m";

// ── Steps ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Welcome,
    Layout,
    Panels,
    Summary,
}

impl Step {
    fn all() -> &'static [Step] {
        &[Step::Welcome, Step::Layout, Step::Panels, Step::Summary]
    }

    fn next(&self) -> Option<Self> {
        match self {
            Step::Welcome => Some(Step::Layout),
            Step::Layout => Some(Step::Panels),
            Step::Panels => Some(Step::Summary),
            Step::Summary => None,
        }
    }

    fn prev(&self) -> Option<Self> {
        match self {
            Step::Welcome => None,
            Step::Layout => Some(Step::Welcome),
            Step::Panels => Some(Step::Layout),
            Step::Summary => Some(Step::Panels),
        }
    }

    fn index(&self) -> usize {
        Self::all().iter().position(|s| s == self).unwrap_or(0)
    }
}

// ── Available fields ─────────────────────────────────────────────────────

const MOBILE_FIELDS: &[(&str, &str, &str)] = &[
    ("device", "\u{f109}", "Device"),
    ("os", "\u{f17c}", "OS"),
    ("rom", "\u{f0c6}", "ROM"),
    ("soc", "\u{f2db}", "SoC"),
    ("arch", "\u{f17c}", "Arch"),
    ("kernel", "\u{e271}", "Krn"),
    ("battery_level", "\u{f0e7}", "Bat"),
    ("battery_temp", "\u{f2c7}", "Temp"),
    ("battery_health", "\u{f004}", "Health"),
    ("battery_status", "\u{f0e7}", "Charge"),
    ("memory", "\u{f1c0}", "RAM"),
    ("storage", "\u{f0a0}", "Stor"),
    ("cpu", "\u{f2db}", "CPU"),
    ("gpu", "\u{f26c}", "GPU"),
    ("cpu_temp", "\u{f2c7}", "CPU Temp"),
    ("uptime", "\u{f017}", "Up"),
    ("packages", "\u{f1b3}", "Pkg"),
    ("root_status", "\u{f023}", "Root"),
    ("bootloader", "\u{f085}", "Bootloader"),
    ("selinux", "\u{f023}", "SELinux"),
    ("resolution", "\u{f108}", "Res"),
    ("brightness", "\u{f185}", "Brightness"),
    ("refresh_rate", "\u{f26c}", "Refresh"),
    ("signal", "\u{f012}", "Signal"),
    ("wifi_ssid", "\u{f1eb}", "WiFi"),
    ("security_patch", "\u{f0c6}", "Sec. Patch"),
    ("user", "\u{f007}", "Usr"),
    ("local_ip", "\u{f0c1}", "IP"),
    ("shell", "\u{f489}", "Sh"),
];

// ── App state ────────────────────────────────────────────────────────────

struct App {
    cfg: Config,
    step: Step,
    quit: bool,
    show_preview: bool,
    current_ascii: String,
    layout_list_state: ListState,
    panel_left_state: ListState,
    panel_right_state: ListState,
    panel_focus: PanelFocus,
    input_mode: InputMode,
    add_panel_available: Vec<(&'static str, &'static str, &'static str)>,
    add_panel_selected: usize,
    add_panel_side: PanelFocus,
    saved: bool,
    term_width: u16,
    term_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PanelFocus { Left, Right }

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,
    AddingPanel,
}

// ── Public entry point ───────────────────────────────────────────────────

pub fn run(cfg: &mut Config) -> Result<()> {
    ascii::ensure_logos()?;

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let current_ascii = ascii::load(cfg).unwrap_or_default();

    let mut layout_list_state = ListState::default();
    layout_list_state.select(Some(0));

    let mut panel_left_state = ListState::default();
    panel_left_state.select(Some(0));
    let mut panel_right_state = ListState::default();
    panel_right_state.select(Some(0));

    let mut app = App {
        cfg: cfg.clone(),
        step: Step::Welcome,
        quit: false,
        show_preview: false,
        current_ascii,
        layout_list_state,
        panel_left_state,
        panel_right_state,
        panel_focus: PanelFocus::Left,
        input_mode: InputMode::Normal,
        add_panel_available: Vec::new(),
        add_panel_selected: 0,
        add_panel_side: PanelFocus::Left,
        saved: false,
        term_width: 80,
        term_height: 24,
    };

    let res = run_app(&mut terminal, &mut app);

    let _ = terminal::disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen);

    if res.is_ok() && app.saved {
        *cfg = app.cfg;
    }

    res
}

// ── Main loop ────────────────────────────────────────────────────────────

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| ui(f, app))?;
        handle_event(app)?;
    }
    Ok(())
}

fn handle_event(app: &mut App) -> Result<()> {
    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press { return Ok(()); }

        match app.input_mode {
            InputMode::AddingPanel => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.add_panel_selected > 0 {
                            app.add_panel_selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.add_panel_selected + 1 < app.add_panel_available.len() {
                            app.add_panel_selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if app.add_panel_selected < app.add_panel_available.len() {
                            let (key, icon, label) = app.add_panel_available[app.add_panel_selected];
                            let fd = FieldDef {
                                field: key.to_string(),
                                icon: icon.to_string(),
                                label: label.to_string(),
                                enabled: true,
                            };
                            match app.add_panel_side {
                                PanelFocus::Left => app.cfg.display.left.push(fd),
                                PanelFocus::Right => app.cfg.display.right.push(fd),
                            }
                        }
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {}
                }
                return Ok(());
            }
            InputMode::Normal => {}
        }

        match app.step {
            Step::Welcome => {
                match key.code {
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                        if let Some(next) = app.step.next() { app.step = next; }
                    }
                    KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Esc => { app.quit = true; }
                    _ => {}
                }
            }
            Step::Layout => {
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        let layouts = AppLayout::mobile_variants();
                        let i = app.layout_list_state.selected().unwrap_or(0);
                        if i + 1 < layouts.len() {
                            app.layout_list_state.select(Some(i + 1));
                            apply_layout(app, i + 1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = app.layout_list_state.selected().unwrap_or(0);
                        if i > 0 {
                            app.layout_list_state.select(Some(i - 1));
                            apply_layout(app, i - 1);
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                        if let Some(next) = app.step.next() { app.step = next; }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(prev) = app.step.prev() { app.step = prev; }
                    }
                    KeyCode::Char('p') => { app.show_preview = !app.show_preview; }
                    KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Esc => { app.quit = true; }
                    _ => {}
                }
            }
            Step::Panels => {
                match key.code {
                    KeyCode::Char('[') | KeyCode::Char(']') => {
                        app.panel_focus = match app.panel_focus {
                            PanelFocus::Left => PanelFocus::Right,
                            PanelFocus::Right => PanelFocus::Left,
                        };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        match app.panel_focus {
                            PanelFocus::Left => {
                                let i = app.panel_left_state.selected().unwrap_or(0);
                                if i + 1 < app.cfg.display.left.len() {
                                    app.panel_left_state.select(Some(i + 1));
                                }
                            }
                            PanelFocus::Right => {
                                let i = app.panel_right_state.selected().unwrap_or(0);
                                if i + 1 < app.cfg.display.right.len() {
                                    app.panel_right_state.select(Some(i + 1));
                                }
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        match app.panel_focus {
                            PanelFocus::Left => {
                                let i = app.panel_left_state.selected().unwrap_or(0);
                                if i > 0 { app.panel_left_state.select(Some(i - 1)); }
                            }
                            PanelFocus::Right => {
                                let i = app.panel_right_state.selected().unwrap_or(0);
                                if i > 0 { app.panel_right_state.select(Some(i - 1)); }
                            }
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Char(' ') => {
                        match app.panel_focus {
                            PanelFocus::Left => {
                                if let Some(i) = app.panel_left_state.selected() {
                                    if i < app.cfg.display.left.len() {
                                        app.cfg.display.left[i].enabled = !app.cfg.display.left[i].enabled;
                                    }
                                }
                            }
                            PanelFocus::Right => {
                                if let Some(i) = app.panel_right_state.selected() {
                                    if i < app.cfg.display.right.len() {
                                        app.cfg.display.right[i].enabled = !app.cfg.display.right[i].enabled;
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('a') => {
                        let fields = match app.panel_focus {
                            PanelFocus::Left => &app.cfg.display.left,
                            PanelFocus::Right => &app.cfg.display.right,
                        };
                        let existing: std::collections::HashSet<&str> =
                            fields.iter().map(|f| f.field.as_str()).collect();
                        app.add_panel_available = MOBILE_FIELDS
                            .iter()
                            .filter(|(key, _, _)| !existing.contains(*key))
                            .copied()
                            .collect();
                        if !app.add_panel_available.is_empty() {
                            app.add_panel_selected = 0;
                            app.add_panel_side = app.panel_focus;
                            app.input_mode = InputMode::AddingPanel;
                        }
                    }
                    KeyCode::Char('d') => {
                        let idx = match app.panel_focus {
                            PanelFocus::Left => app.panel_left_state.selected(),
                            PanelFocus::Right => app.panel_right_state.selected(),
                        };
                        if let Some(i) = idx {
                            let fields = match app.panel_focus {
                                PanelFocus::Left => &mut app.cfg.display.left,
                                PanelFocus::Right => &mut app.cfg.display.right,
                            };
                            if i < fields.len() {
                                fields.remove(i);
                                let len = fields.len();
                                match app.panel_focus {
                                    PanelFocus::Left => {
                                        app.panel_left_state.select(Some(i.min(len.saturating_sub(1))));
                                    }
                                    PanelFocus::Right => {
                                        app.panel_right_state.select(Some(i.min(len.saturating_sub(1))));
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        match app.panel_focus {
                            PanelFocus::Left => {
                                if let Some(i) = app.panel_left_state.selected() {
                                    if i > 0 {
                                        app.cfg.display.left.swap(i, i - 1);
                                        app.panel_left_state.select(Some(i - 1));
                                    }
                                }
                            }
                            PanelFocus::Right => {
                                if let Some(i) = app.panel_right_state.selected() {
                                    if i > 0 {
                                        app.cfg.display.right.swap(i, i - 1);
                                        app.panel_right_state.select(Some(i - 1));
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                        if let Some(next) = app.step.next() { app.step = next; }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(prev) = app.step.prev() { app.step = prev; }
                    }
                    KeyCode::Char('p') => { app.show_preview = !app.show_preview; }
                    KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Esc => { app.quit = true; }
                    _ => {}
                }
            }
            Step::Summary => {
                match key.code {
                    KeyCode::Char('s') | KeyCode::Enter => {
                        app.cfg.save()?;
                        app.saved = true;
                        app.quit = true;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(prev) = app.step.prev() { app.step = prev; }
                    }
                    KeyCode::Char('p') => { app.show_preview = !app.show_preview; }
                    KeyCode::Char('q') | KeyCode::Backspace | KeyCode::Esc => { app.quit = true; }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

// ── UI ───────────────────────────────────────────────────────────────────

fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.term_width = area.width;
    app.term_height = area.height;

    // ── Preview overlay ──
    if app.show_preview {
        render_preview_overlay(frame, area, app);
        return;
    }

    // ── Step indicator ──
    let steps = Step::all();
    let step_names = ["Welcome", "Layout", "Panels", "Save"];
    let step_line: String = steps.iter().enumerate().map(|(i, s)| {
        if *s == app.step {
            format!(" >{}< ", step_names[i])
        } else if steps.iter().position(|x| x == s).unwrap() < app.step.index() {
            format!("  {}  ", step_names[i])
        } else {
            format!("  {}  ", step_names[i])
        }
    }).collect::<Vec<_>>().join(" → ");
    let step_title = format!(" AtlasFetch Mobile Setup — {}", step_line);

    // ── Main layout ──
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // step indicator
            Constraint::Min(5),      // content
            Constraint::Length(3),   // help
        ])
        .split(area);

    // Step indicator
    let nav_style = Style::default().fg(TuiColor::Rgb(157, 133, 255));
    frame.render_widget(
        Paragraph::new(Text::from(Line::from(Span::styled(&step_title, nav_style)))),
        chunks[0],
    );

    // Content
    match app.step {
        Step::Welcome => render_welcome(frame, chunks[1], app),
        Step::Layout => render_layout_selection(frame, chunks[1], app),
        Step::Panels => render_panel_editor(frame, chunks[1], app),
        Step::Summary => render_summary(frame, chunks[1], app),
    }

    // Help bar
    let help = match app.step {
        Step::Welcome => " [l/→] Next  [q] Quit",
        Step::Layout => " [j/k] Navigate  [l/→] Next  [h/←] Back  [p] Preview  [q] Quit",
        Step::Panels => " [j/k] Navigate  [/] Focus  [x] Toggle  [a] Add  [d] Delete  [r] Reorder  [l/→] Next  [p] Preview  [q] Quit",
        Step::Summary => " [s/Enter] Save  [p] Preview  [q] Quit",
    };
    let help_style = Style::default().fg(TuiColor::Rgb(100, 100, 120));
    frame.render_widget(
        Paragraph::new(Text::from(Line::from(Span::styled(help, help_style)))),
        chunks[2],
    );
}

// ── Welcome ──────────────────────────────────────────────────────────────

fn render_welcome(frame: &mut Frame, area: Rect, app: &mut App) {
    let ac = if !app.cfg.logo.colors.is_empty() {
        app.cfg.logo.colors[0]
    } else {
        crate::theme::Color::new(255, 102, 146)
    };

    let device = info::collect()
        .map(|i| i.device)
        .unwrap_or_default();
    let device = if device.is_empty() {
        std::env::var("HOSTNAME").unwrap_or_else(|_| "device".into())
    } else {
        device
    };

    let text = Text::from(vec![
        Line::from(Span::styled("", Style::default())),
        Line::from(Span::styled("  ◢ ATLAS ◣", Style::default().fg(TuiColor::Rgb(ac.r, ac.g, ac.b)).add_modifier(Modifier::BOLD))),
        Line::from(Span::raw("")),
        Line::from(Span::raw(format!("  Welcome to AtlasFetch Mobile, explorer!"))),
        Line::from(Span::raw(format!("  Device: {}", device))),
        Line::from(Span::raw("")),
        Line::from(Span::raw("  Configure how your device info is displayed.")),
        Line::from(Span::raw("  Optimized for Android/Termux.")),
        Line::from(Span::raw("")),
        Line::from(Span::styled("  Press l/→ to start", Style::default().fg(TuiColor::Rgb(100, 200, 120)))),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

// ── Layout selection ─────────────────────────────────────────────────────

fn render_layout_selection(frame: &mut Frame, area: Rect, app: &mut App) {
    let layouts = AppLayout::mobile_variants();
    let items: Vec<ListItem> = layouts.iter().map(|l| {
        ListItem::new(vec![
            Line::from(Span::styled(format!("  {} ", l.name()), Style::default().add_modifier(Modifier::BOLD))),
            Line::from(Span::raw(format!("   {}  ", l.description()))),
        ])
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Layout").border_type(BorderType::Rounded))
        .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));

    frame.render_stateful_widget(list, area, &mut app.layout_list_state);
}

fn apply_layout(app: &mut App, index: usize) {
    let layouts = AppLayout::mobile_variants();
    if index < layouts.len() {
        let layout = layouts[index];
        app.cfg.panel.gap = layout.gap();
        app.cfg.panel.left_pad = layout.padding();
        app.cfg.panel.right_pad = layout.padding();
        app.cfg.panel.max_val_width = layout.max_panel_width();
    }
}

// ── Panel editor ─────────────────────────────────────────────────────────

fn render_panel_editor(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.input_mode == InputMode::AddingPanel {
        let items: Vec<ListItem> = app.add_panel_available.iter().map(|(key, icon, label)| {
            ListItem::new(format!(" {} ({}) {}", icon, key, label))
        }).collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Add Field").border_type(BorderType::Rounded))
            .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));
        let mut state = ListState::default().with_selected(Some(app.add_panel_selected));
        frame.render_stateful_widget(list, area, &mut state);
        return;
    }

    // Show only the focused panel, full width
    let (items, state, title) = match app.panel_focus {
        PanelFocus::Left => {
            let items: Vec<ListItem> = app.cfg.display.left.iter().map(|f| {
                let check = if f.enabled { "✓" } else { " " };
                ListItem::new(format!(" [{}] {} ({})  {}", check, f.field, f.icon, f.label))
            }).collect();
            (items, &mut app.panel_left_state, "Left Panel")
        }
        PanelFocus::Right => {
            let items: Vec<ListItem> = app.cfg.display.right.iter().map(|f| {
                let check = if f.enabled { "✓" } else { " " };
                ListItem::new(format!(" [{}] {} ({})  {}", check, f.field, f.icon, f.label))
            }).collect();
            (items, &mut app.panel_right_state, "Right Panel")
        }
    };

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!("{} [focused]", title))
            .border_style(Style::default().fg(TuiColor::Cyan)))
        .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));

    frame.render_stateful_widget(list, area, state);
}

// ── Summary ──────────────────────────────────────────────────────────────

fn render_summary(frame: &mut Frame, area: Rect, app: &App) {
    let enabled_count = app.cfg.display.left.iter().filter(|f| f.enabled).count()
        + app.cfg.display.right.iter().filter(|f| f.enabled).count();

    let text = Text::from(vec![
        Line::from(Span::styled("  Setup Complete", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(Span::raw("")),
        Line::from(Span::raw(format!("  Layout: {}", AppLayout::mobile_variants()[
            app.layout_list_state.selected().unwrap_or(0)
        ].name()))),
        Line::from(Span::raw(format!("  Enabled fields: {}", enabled_count))),
        Line::from(Span::raw("")),
        Line::from(Span::raw("  Press s/Enter to save and exit.")),
        Line::from(Span::raw("  Press q to discard changes.")),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

// ── Preview overlay ──────────────────────────────────────────────────────

fn render_preview_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let info = info::SysInfo {
        os: "Android 16".into(),
        host: "atlasphone".into(),
        user: std::env::var("USER").unwrap_or_else(|_| "user".into()),
        kernel: "6.x".into(),
        uptime: "2h 14m".into(),
        packages: "1200".into(),
        shell: "bash".into(),
        terminal: "Termux".into(),
        cpu: "Cortex-A78".into(),
        gpu: "Adreno".into(),
        memory: "4.2/8.0G".into(),
        disk: "64/128G".into(),
        wm: String::new(),
        load: String::new(),
        processes: String::new(),
        local_ip: String::new(),
        resolution: "1080x2400".into(),
        de: String::new(),
        font: String::new(),
        vram: String::new(),
        flatpak: String::new(),
        snap: String::new(),
        device: "Moto G54".into(),
        rom: "LineageOS 22".into(),
        soc: "Dimensity 7020".into(),
        arch: "aarch64".into(),
        battery_level: "82%".into(),
        battery_temp: "31°C".into(),
        battery_health: "Good".into(),
        battery_status: "Charging".into(),
        root_status: "Magisk active".into(),
        bootloader: "Unlocked".into(),
        selinux: "Enforcing".into(),
        storage: "64/128G".into(),
        cpu_temp: "45°C".into(),
        brightness: "60%".into(),
        refresh_rate: "120Hz".into(),
        signal: "4/4".into(),
        wifi_ssid: "Home WiFi".into(),
        security_patch: "2025-05".into(),
        uptime_days: "2d 14h".into(),
    };

    let is_narrow = app.term_width < 55;
    let preview_lines = render::render_mobile_preview(
        &app.cfg, &info, &app.current_ascii,
        area.width.saturating_sub(2), is_narrow,
    );

    let lines: Vec<Line> = preview_lines.iter().map(|sl| {
        let spans: Vec<Span> = sl.segments.iter().map(|seg| {
            let mut style = Style::default();
            if let Some(fg) = &seg.fg {
                style = style.fg(TuiColor::Rgb(fg.r, fg.g, fg.b));
            }
            if let Some(bg) = &seg.bg {
                style = style.bg(TuiColor::Rgb(bg.r, bg.g, bg.b));
            }
            if seg.bold { style = style.add_modifier(Modifier::BOLD); }
            Span::styled(seg.text.clone(), style)
        }).collect();
        Line::from(spans)
    }).collect();

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Preview (p to close)").border_type(BorderType::Rounded));
    frame.render_widget(paragraph, area);
}
