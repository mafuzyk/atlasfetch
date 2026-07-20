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
use crate::component;
use crate::config::{Config, FieldDef};
use crate::info;
use crate::layout::AppLayout;
use crate::theme::{self, Color};

// ── Tab enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Welcome,
    Theme,
    Mode,
    Panels,
    Ascii,
    Save,
}

impl Tab {
    fn all() -> [Tab; 6] { [Tab::Welcome, Tab::Theme, Tab::Mode, Tab::Panels, Tab::Ascii, Tab::Save] }
    fn label(&self) -> &'static str {
        match self {
            Tab::Welcome => " Welcome ",
            Tab::Theme   => " Theme ",
            Tab::Mode    => " Mode ",
            Tab::Panels  => " Panels ",
            Tab::Ascii   => " ASCII ",
            Tab::Save    => " Save ",
        }
    }
    fn next(&self) -> Self {
        match self {
            Tab::Welcome => Tab::Theme,
            Tab::Theme   => Tab::Mode,
            Tab::Mode    => Tab::Panels,
            Tab::Panels  => Tab::Ascii,
            Tab::Ascii   => Tab::Save,
            Tab::Save    => Tab::Welcome,
        }
    }
    fn prev(&self) -> Self {
        match self {
            Tab::Welcome => Tab::Save,
            Tab::Save    => Tab::Ascii,
            Tab::Ascii   => Tab::Panels,
            Tab::Panels  => Tab::Mode,
            Tab::Mode    => Tab::Theme,
            Tab::Theme   => Tab::Welcome,
        }
    }
}

// ── Input sub-modes ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,
    EditingCustomPalette,
    EditingLabel,
    AddingPanel,
    PastingAscii,
    BrowsingFile,
    SearchingAscii,
}

// ── Editor state ─────────────────────────────────────────────────────────

pub struct Editor {
    cfg: Config,
    info: crate::info::SysInfo,
    tab: Tab,
    app_layout: AppLayout,
    monitor_mode: bool,
    scene_focus: bool, // false = scenes, true = layouts
    // Theme
    themes: Vec<theme::Theme>,
    theme_selected: usize,
    custom_palette_input: String,
    // ASCII
    logo_keys: Vec<String>,
    ascii_art: String,
    ascii_component: component::ascii::AsciiComponent,
    ascii_source: String, // "builtin:key" | "file:path" | "pasted" | "disabled"
    ascii_is_small: bool,
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
    // Scene / Layout
    scene_selected: usize,
    layout_selected: usize,
    // ASCII selection
    ascii_selected: usize,
    ascii_search: String,
    // Cached components
    monitor_comp: component::monitor::MonitorComponent,
    system_comp: component::system::SystemComponent,
    companion_comp: component::companion::CompanionComponent,
    // General
    input_mode: InputMode,
    paste_buffer: String,
    saved: bool,
    preview_width: usize,
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
        let ascii_is_small = {
            let small_key = format!("{}_small", cfg.logo.key);
            let tw = crate::layout::terminal_width();
            tw < 65 && ascii::has_variant(&small_key)
        };

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

        let app_layout = AppLayout::Centered;
        let scene_selected = component::Scene::all().iter()
            .position(|s| s.name().to_lowercase() == cfg.scene)
            .unwrap_or(0);

        let themes = theme::all_themes();
        let theme_selected = themes.iter().position(|t| t.colors == cfg.logo.colors).unwrap_or(0);

        let mut available: Vec<(String, String, String)> = if info::is_android() {
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
            ].into_iter().map(|(k,i,l)| (k.to_string(),i.to_string(),l.to_string())).collect()
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
            ].into_iter().map(|(k,i,l)| (k.to_string(),i.to_string(),l.to_string())).collect()
        };
        let bar_fields = ["cpu", "gpu", "memory", "disk", "vram", "load", "cpu_temp", "battery_level", "brightness", "signal", "storage"];
        for &base in &bar_fields {
            if let Some((_, icon, label)) = available.iter().find(|(k, _, _)| k == base) {
                available.push((format!("{}_bar", base), icon.clone(), format!("{} [bar]", label)));
            }
        }

        let (tw, th) = terminal::size()?;
        let layout_selected = AppLayout::pc_variants().iter().position(|l| *l == app_layout).unwrap_or(0);
        let ascii_component = component::ascii::AsciiComponent::new(ascii_art.clone());
        let ascii_selected = match ascii_source.split_once(':') {
            Some(("builtin", k)) => logo_keys.iter().position(|lk| lk == k).unwrap_or(0),
            _ if ascii_source.starts_with("file:") => logo_keys.len(),
            _ if ascii_source == "pasted" => logo_keys.len() + 1,
            _ => logo_keys.len() + 2,
        };
        let mut ed = Self {
            cfg, info, tab: Tab::Welcome, app_layout, layout_selected,
            scene_selected, scene_focus: false, monitor_mode: false,
            themes, theme_selected, custom_palette_input: String::new(),
            logo_keys, ascii_art, ascii_component, ascii_source, ascii_selected,
            ascii_search: String::new(), ascii_is_small,
            panel_focus: false, panel_left_sel: 0, panel_right_sel: 0,
            add_panel_available: available,
            add_panel_sel: 0,
            editing_label_input: String::new(),
            file_browser_cwd: std::env::current_dir().unwrap_or_else(|_| "/".into()),
            file_browser_entries: Vec::new(),
            file_browser_sel: 0,
            monitor_comp: component::monitor::MonitorComponent::new(),
            system_comp: component::system::SystemComponent,
            companion_comp: component::companion::CompanionComponent,
            input_mode: InputMode::Normal,
            paste_buffer: String::new(),
            saved: false,
            preview_width: tw.saturating_sub(20) as usize,
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

    fn ensure_ascii_fits(&mut self) {
        let term_w = crate::layout::terminal_width();
        let art_width = self.ascii_art.lines()
            .map(|l| l.trim_end().width())
            .max()
            .unwrap_or(0);
        let small_key = format!("{}_small", self.cfg.logo.key);
        let has_small = crate::ascii::has_variant(&small_key);

        // If the art overflows the terminal, try switching to _small
        if art_width > 0 && term_w.saturating_sub(art_width) < 10 && has_small && !self.ascii_is_small {
            if let Ok(art) = crate::ascii::load_variant(&small_key) {
                self.ascii_art = art;
                self.ascii_is_small = true;
            }
        }

        // If the terminal has grown enough, restore the full-sized art
        if self.ascii_is_small {
            if let Ok(art) = crate::ascii::load_variant(&self.cfg.logo.key) {
                let full_w = art.lines().map(|l| l.trim_end().width()).max().unwrap_or(0);
                if term_w.saturating_sub(full_w) >= 10 {
                    self.ascii_art = art;
                    self.ascii_is_small = false;
                }
            }
        }
    }

    fn filtered_logo_indices(&self) -> Vec<usize> {
        let q = self.ascii_search.to_lowercase();
        self.logo_keys.iter().enumerate()
            .filter(|(_, k)| q.is_empty() || k.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    fn select_logo_at(&mut self, idx: usize) {
        if idx < self.logo_keys.len() {
            self.ascii_selected = idx;
            let key = &self.logo_keys[idx];
            self.ascii_source = format!("builtin:{}", key);
            self.cfg.logo.key = key.clone();
            if let Ok(art) = ascii::load(&self.cfg) {
                self.ascii_art = art;
            }
            self.dirty = true;
        }
    }

    fn jump_to_first_matching_logo(&mut self) {
        let filtered = self.filtered_logo_indices();
        if let Some(&first) = filtered.first() {
            if self.ascii_selected >= self.logo_keys.len() || !filtered.contains(&self.ascii_selected) {
                self.select_logo_at(first);
            }
        }
    }

    fn refresh_preview(&mut self) {
        self.ensure_ascii_fits();
        let tw = self.preview_width.max(20);

        let scene = match self.cfg.scene.as_str() {
            "dashboard" => component::Scene::Dashboard,
            "cockpit" => component::Scene::Cockpit,
            "classicfetch" | "classic_fetch" => component::Scene::ClassicFetch,
            _ => component::Scene::Classic,
        };

        self.ascii_component = component::ascii::AsciiComponent::new(self.ascii_art.clone());

        use component::Component;
        let components: Vec<&dyn Component> = vec![
            &self.ascii_component,
            &self.system_comp,
            &self.monitor_comp,
            &self.companion_comp,
        ];

        let ctx = component::RenderCtx {
            info: &self.info,
            cfg: &self.cfg,
            term_width: tw,
            palette: &self.cfg.logo.colors,
        };

        let output = if self.monitor_mode {
            component::render_monitor_split(&components, &ctx)
        } else {
            component::render_scene(scene, &components, &ctx)
        };

        let mut lines: Vec<Line> = Vec::new();

        for output_line in &output.lines {
            let spans: Vec<Span> = output_line.iter().map(|s| {
                let mut style = Style::default();
                if let Some(fg) = &s.fg { style = style.fg(TuiColor::Rgb(fg.r, fg.g, fg.b)); }
                if let Some(bg) = &s.bg { style = style.bg(TuiColor::Rgb(bg.r, bg.g, bg.b)); }
                if s.bold { style = style.add_modifier(Modifier::BOLD); }
                Span::styled(s.text.clone(), style)
            }).collect();
            if !spans.is_empty() {
                lines.push(Line::from(spans));
            }
        }

        self.preview_lines = lines;
        self.dirty = false;
    }
}

fn tui_color(c: &Color) -> TuiColor { TuiColor::Rgb(c.r, c.g, c.b) }

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

    // Update preview width for layout computations
    let preview_w = if area.width >= 100 {
        (area.width as f64 * 0.55) as u16
    } else {
        (area.height as f64 * 0.55) as u16
    };
    let preview_w = preview_w.saturating_sub(2).max(20) as usize;
    if preview_w != editor.preview_width {
        editor.preview_width = preview_w;
        editor.dirty = true;
    }

    if editor.input_mode != InputMode::Normal && editor.input_mode != InputMode::SearchingAscii {
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
                let prefix = if *is_dir { "[DIR] " } else { "[FILE] " };
                ListItem::new(format!("{}{}", prefix, name))
            }).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("{:?}", editor.file_browser_cwd)))
                .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80)));
            frame.render_widget(Clear, area);
            frame.render_widget(list, area);
            return;
        }
        InputMode::SearchingAscii => unreachable!(),
        InputMode::Normal => unreachable!(),
    };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded);
    let p = Paragraph::new(text).block(block);
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
    frame.render_widget(Paragraph::new(Line::from(tabs)), area);
}

fn render_hints(frame: &mut Frame, area: Rect, editor: &Editor) {
    let text = match editor.tab {
        Tab::Welcome => " Tab/Enter next  q quit",
        Tab::Theme   => " ↑↓ theme  c custom palette  v toggle direction  Tab/Enter next  q quit",
        Tab::Mode    => " ↑↓ scene/layout  ←→ focus  m monitor  Tab/Enter next  q quit",
        Tab::Ascii   => " ↑↓ logo  / search  d disable  c file  p paste  Tab/Enter next  q quit",
        Tab::Panels  => " ↑↓ nav  Space toggle  [/] panel  a add  d delete  r reorder  e edit label  Tab/Enter next  q quit",
        Tab::Save    => " s save & exit  q discard",
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, Style::default().fg(TuiColor::Rgb(140, 140, 140))))), area);
}

fn render_tab_content(frame: &mut Frame, area: Rect, editor: &Editor) {
    match editor.tab {
        Tab::Welcome => render_welcome_tab(frame, area, editor),
        Tab::Theme   => render_theme_tab(frame, area, editor),
        Tab::Mode    => render_mode_tab(frame, area, editor),
        Tab::Ascii   => render_ascii_tab(frame, area, editor),
        Tab::Panels  => render_panels_tab(frame, area, editor),
        Tab::Save    => render_save_tab(frame, area, editor),
    }
}

// ── Welcome tab ──────────────────────────────────────────────────────────

fn render_welcome_tab(frame: &mut Frame, area: Rect, _editor: &Editor) {
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled("  Welcome to atlasfetch setup!", Style::default().fg(TuiColor::Rgb(133, 188, 255)).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("  Use Tab to navigate through the configuration tabs:"),
            Line::from(""),
            Line::from("  1. Theme  — pick a color theme for your fetch"),
            Line::from("  2. Mode   — choose scene, layout, or Monitor mode"),
            Line::from("  3. Panels — add, remove, and reorder info fields"),
            Line::from("  4. ASCII  — select or disable ASCII art logos"),
            Line::from("  5. Save   — save your configuration"),
            Line::from(""),
            Line::from(Span::styled("  Press Tab or Enter to start configuring!", Style::default().fg(TuiColor::Rgb(157, 133, 255)))),
        ]))
        .block(Block::default().title("Welcome").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255)))),
        area);
}

// ── Mode tab ─────────────────────────────────────────────────────────────

fn render_mode_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let rows = ratatui::layout::Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).split(area);

    // Monitor toggle
    let monitor_label = if editor.monitor_mode { "▶ Monitor Mode" } else { "  Monitor Mode" };
    let monitor_style = if editor.monitor_mode {
        Style::default().fg(TuiColor::Rgb(255, 102, 146)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TuiColor::Rgb(140, 140, 140))
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(monitor_label, monitor_style),
            Span::styled(" — Split 50/50: fetch + live monitor", Style::default().fg(TuiColor::Rgb(120, 120, 120))),
        ])),
        rows[0],
    );

    if editor.monitor_mode {
        // In monitor mode, just show info
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(""),
                Line::from(Span::styled("  Monitor Mode active", Style::default().fg(TuiColor::Rgb(255, 102, 146)).add_modifier(Modifier::BOLD))),
                Line::from("  Press m to toggle off"),
                Line::from(""),
                Line::from("  The preview shows a 50/50 split between your"),
                Line::from("  selected fetch layout and live system monitoring."),
            ])),
            rows[1],
        );
        return;
    }

    let halves = ratatui::layout::Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);

    let scenes = component::Scene::all();
    let scene_items: Vec<ListItem> = scenes.iter().enumerate().map(|(i, s)| {
        let sel = !editor.scene_focus && i == editor.scene_selected;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::styled(s.name(), if sel { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
            Span::raw("  "),
            Span::styled(s.description(), Style::default().fg(TuiColor::Rgb(120,120,120))),
        ]))
    }).collect();
    let scene_block = Block::default()
        .title(" Scene ")
        .borders(Borders::ALL).border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if editor.scene_focus { TuiColor::Rgb(100,100,100) } else { TuiColor::Rgb(157, 133, 255) }));
    frame.render_widget(List::new(scene_items).block(scene_block), halves[0]);

    let layouts = AppLayout::pc_variants();
    let layout_items: Vec<ListItem> = layouts.iter().enumerate().map(|(i, l)| {
        let sel = editor.scene_focus && i == editor.layout_selected;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { "▶ " } else { "  " }, Style::default().fg(TuiColor::Rgb(255, 184, 131))),
            Span::styled(l.name(), if sel { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
            Span::raw("  "),
            Span::styled(l.description(), Style::default().fg(TuiColor::Rgb(120,120,120))),
        ]))
    }).collect();
    let layout_block = Block::default()
        .title(" Layout ")
        .borders(Borders::ALL).border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if editor.scene_focus { TuiColor::Rgb(157, 133, 255) } else { TuiColor::Rgb(100,100,100) }));
    frame.render_widget(List::new(layout_items).block(layout_block), halves[1]);
}

// ── Theme tab ────────────────────────────────────────────────────────────

fn render_theme_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let top = ratatui::layout::Layout::vertical([Constraint::Fill(1), Constraint::Length(4), Constraint::Length(3)]).split(area);

    let items: Vec<ListItem> = editor.themes.iter().enumerate().map(|(i, t)| {
        let swatch: Vec<Span> = t.colors.iter().map(|c| Span::styled("  ", Style::default().bg(tui_color(c)))).collect();
        let mut line = vec![
            Span::styled(if i == editor.theme_selected { "▸ " } else { "  " }, Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(format!("{:15}", t.name)),
        ];
        line.extend(swatch);
        ListItem::new(Line::from(line))
    }).collect();
    let mut state = ratatui::widgets::ListState::default().with_selected(Some(editor.theme_selected));
    frame.render_stateful_widget(
        List::new(items).block(Block::default().title("Theme Presets").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152)))),
        top[0], &mut state);

    let swatch: Vec<Span> = editor.cfg.logo.colors.iter().map(|c| Span::styled("  ", Style::default().bg(tui_color(c)))).collect();
    frame.render_widget(Paragraph::new(Line::from(swatch)).block(Block::default().title("Current Palette").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152)))), top[1]);

    let dir_label = if editor.cfg.logo.color_dir == "vertical" { "Vertical" } else { "Horizontal" };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(format!(" v toggle color direction — currently: {}", dir_label), Style::default().fg(TuiColor::Rgb(140, 140, 140))))).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)), top[2]);
}

// ── ASCII tab ────────────────────────────────────────────────────────────

fn render_ascii_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let q = editor.ascii_search.to_lowercase();
    let n = editor.logo_keys.len();

    let items: Vec<ListItem> = editor.logo_keys.iter().enumerate()
        .filter(|(_, key)| q.is_empty() || key.to_lowercase().contains(&q))
        .map(|(i, key)| {
            let sel = i == editor.ascii_selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▶ " } else { "  " },
                    Style::default().fg(if sel { TuiColor::Rgb(255, 184, 131) } else { TuiColor::Rgb(60, 60, 70) }),
                ),
                Span::styled(
                    key.clone(),
                    if sel {
                        Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TuiColor::Rgb(200,200,200))
                    },
                ),
            ]))
        }).collect();
    let mut items = items;
    // Special entries always at the bottom
    let has_file = editor.ascii_selected == n;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if has_file { "▶ " } else { "  " }, Style::default().fg(if has_file { TuiColor::Rgb(255, 184, 131) } else { TuiColor::Rgb(60, 60, 70) })),
        Span::styled("[ Custom file ]", if has_file { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));
    let is_pasted = editor.ascii_selected == n + 1;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if is_pasted { "▶ " } else { "  " }, Style::default().fg(if is_pasted { TuiColor::Rgb(255, 184, 131) } else { TuiColor::Rgb(60, 60, 70) })),
        Span::styled("[ Paste ASCII ]", if is_pasted { Style::default().fg(TuiColor::Rgb(255,255,255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));
    let disabled = editor.ascii_selected == n + 2;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(if disabled { "▶ " } else { "  " }, Style::default().fg(if disabled { TuiColor::Rgb(255, 102, 146) } else { TuiColor::Rgb(60, 60, 70) })),
        Span::styled("[ Disabled ]", if disabled { Style::default().fg(TuiColor::Rgb(255,102,146)).add_modifier(Modifier::BOLD) } else { Style::default().fg(TuiColor::Rgb(200,200,200)) }),
    ])));


    let search_hint = if editor.input_mode == InputMode::SearchingAscii {
        if editor.ascii_search.is_empty() {
            " Search: type to filter (Esc to cancel)".to_string()
        } else {
            format!(" Search: {}", editor.ascii_search)
        }
    } else {
        " / to search".to_string()
    };

    let list_block = Block::default()
        .title("ASCII Art")
        .title_bottom(search_hint)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(255, 184, 131)));
    let mut list_state = ratatui::widgets::ListState::default().with_selected(Some(editor.ascii_selected));
    frame.render_stateful_widget(List::new(items).block(list_block), area, &mut list_state);
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
            Line::from(Span::styled("  s — Save & Exit", Style::default().fg(TuiColor::Rgb(133, 188, 255)))),
            Line::from(Span::styled("  q     — Discard & Exit", Style::default().fg(TuiColor::Rgb(255, 102, 146)))),
        ]))
        .block(Block::default().title("Save").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255)))),
        area);
}

// ── Preview ──────────────────────────────────────────────────────────────

fn render_preview_panel(frame: &mut Frame, area: Rect, editor: &Editor) {
    let title = if editor.monitor_mode {
        " Preview — Monitor Mode  LIVE ".to_string()
    } else {
        let scenes = component::Scene::all();
        let scene_name = scenes.get(editor.scene_selected).map(|s| s.name()).unwrap_or("Classic");
        let layout_name = editor.app_layout.name();
        format!(" Preview — {} / {} ", scene_name, layout_name)
    };
    let block = Block::default().title(title).borders(Borders::ALL).border_type(BorderType::Rounded)
        .border_style(if editor.monitor_mode {
            Style::default().fg(TuiColor::Rgb(255, 102, 146))
        } else {
            Style::default().fg(TuiColor::Rgb(133, 188, 255))
        });
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(Text::from(editor.preview_lines.clone())).style(Style::default().fg(TuiColor::Rgb(200, 200, 200))), inner);
}

// ── Event handling ───────────────────────────────────────────────────────

fn handle_event(editor: &mut Editor) -> Result<bool> {
    match event::read()? {
        Event::Resize(w, h) => {
            editor.term_width = w;
            editor.term_height = h;
            editor.preview_width = (w.saturating_sub(20)) as usize;
            editor.dirty = true;
        }
        Event::Key(key) if key.kind == KeyEventKind::Press => {

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
                            editor.ascii_selected = editor.logo_keys.len() + 1; // "Paste" slot
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
                                editor.ascii_selected = editor.logo_keys.len(); // "Custom file" slot
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
            InputMode::SearchingAscii => {
                match key.code {
                    KeyCode::Esc => {
                        editor.ascii_search.clear();
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Enter | KeyCode::Tab => {
                        editor.input_mode = InputMode::Normal;
                    }
                    KeyCode::Backspace => {
                        if !editor.ascii_search.is_empty() {
                            editor.ascii_search.pop();
                            editor.jump_to_first_matching_logo();
                            editor.dirty = true;
                        }
                    }
                    KeyCode::Up => {
                        let filtered = editor.filtered_logo_indices();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected) {
                            if pos > 0 {
                                editor.select_logo_at(filtered[pos - 1]);
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[filtered.len() - 1]);
                        }
                    }
                    KeyCode::Down => {
                        let filtered = editor.filtered_logo_indices();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected) {
                            if pos + 1 < filtered.len() {
                                editor.select_logo_at(filtered[pos + 1]);
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[0]);
                        }
                    }
                    KeyCode::Char(ch) if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' => {
                        editor.ascii_search.push(ch);
                        editor.jump_to_first_matching_logo();
                        editor.dirty = true;
                    }
                    _ => {}
                }
                return Ok(true);
            }
            InputMode::Normal => {}
        }

        // Normal mode key handling
        match key.code {
            KeyCode::Char('q') => {
                if editor.tab == Tab::Save { return Ok(false); }
                return Ok(false);
            }
            KeyCode::Enter | KeyCode::Tab => { editor.tab = editor.tab.next(); }
            KeyCode::BackTab => { editor.tab = editor.tab.prev(); }
            KeyCode::Left => {
                if editor.tab == Tab::Mode { editor.scene_focus = false; }
                if editor.tab == Tab::Panels { editor.panel_focus = false; }
            }
            KeyCode::Right => {
                if editor.tab == Tab::Mode { editor.scene_focus = true; }
                if editor.tab == Tab::Panels { editor.panel_focus = true; }
            }
            KeyCode::Up => {
                match editor.tab {
                    Tab::Mode => 'mode_up: {
                        if editor.monitor_mode { break 'mode_up; }
                        if editor.scene_focus {
                            editor.layout_selected = editor.layout_selected.saturating_sub(1);
                            editor.apply_layout(editor.layout_selected);
                        } else {
                            let scenes = component::Scene::all();
                            if editor.scene_selected > 0 {
                                editor.scene_selected -= 1;
                                editor.cfg.scene = scenes[editor.scene_selected].name().to_lowercase();
                                editor.dirty = true;
                            }
                        }
                    }
                    Tab::Theme => {
                        editor.theme_selected = editor.theme_selected.saturating_sub(1);
                        if editor.theme_selected < editor.themes.len() {
                            editor.cfg.logo.colors = editor.themes[editor.theme_selected].colors.clone();
                            editor.dirty = true;
                        }
                    }
                    Tab::Ascii => {
                        let filtered = editor.filtered_logo_indices();
                        let n = editor.logo_keys.len();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected) {
                            if pos > 0 {
                                editor.select_logo_at(filtered[pos - 1]);
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[filtered.len() - 1]);
                        } else if editor.ascii_selected >= n && editor.ascii_selected != n + 2 {
                            if !filtered.is_empty() {
                                editor.select_logo_at(filtered[filtered.len() - 1]);
                            } else if n > 0 {
                                editor.select_logo_at(0);
                            }
                        }
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
                    Tab::Mode => 'mode_down: {
                        if editor.monitor_mode { break 'mode_down; }
                        if editor.scene_focus {
                            let max = AppLayout::pc_variants().len().saturating_sub(1);
                            editor.layout_selected = (editor.layout_selected + 1).min(max);
                            editor.apply_layout(editor.layout_selected);
                        } else {
                            let scenes = component::Scene::all();
                            if editor.scene_selected + 1 < scenes.len() {
                                editor.scene_selected += 1;
                                editor.cfg.scene = scenes[editor.scene_selected].name().to_lowercase();
                                editor.dirty = true;
                            }
                        }
                    }
                    Tab::Theme => {
                        let max = editor.themes.len().saturating_sub(1);
                        editor.theme_selected = (editor.theme_selected + 1).min(max);
                        if editor.theme_selected < editor.themes.len() {
                            editor.cfg.logo.colors = editor.themes[editor.theme_selected].colors.clone();
                            editor.dirty = true;
                        }
                    }
                    Tab::Ascii => {
                        let filtered = editor.filtered_logo_indices();
                        let n = editor.logo_keys.len();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected) {
                            if pos + 1 < filtered.len() {
                                editor.select_logo_at(filtered[pos + 1]);
                            } else {
                                if editor.ascii_selected < n {
                                    editor.ascii_selected = n;
                                    editor.dirty = true;
                                } else if editor.ascii_selected == n {
                                    editor.ascii_selected = n + 1;
                                    editor.dirty = true;
                                } else if editor.ascii_selected == n + 1 {
                                    editor.ascii_selected = n + 2;
                                    editor.ascii_source = "disabled".into();
                                    editor.cfg.logo.key = String::new();
                                    editor.cfg.logo.path = "disabled".into();
                                    editor.ascii_art = String::new();
                                    editor.dirty = true;
                                }
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[0]);
                        }
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
            KeyCode::Char('m') => {
                if editor.tab == Tab::Mode {
                    editor.monitor_mode = !editor.monitor_mode;
                    editor.dirty = true;
                }
            }
            KeyCode::Char('/') => {
                if editor.tab == Tab::Panels {
                    editor.panel_focus = !editor.panel_focus;
                }
                if editor.tab == Tab::Ascii {
                    editor.input_mode = InputMode::SearchingAscii;
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
                if editor.tab == Tab::Ascii {
                    editor.ascii_source = "disabled".into();
                    editor.cfg.logo.key = String::new();
                    editor.cfg.logo.path = "disabled".into();
                    editor.ascii_art = String::new();
                    let n = editor.logo_keys.len();
                    editor.ascii_selected = n + 2;
                    editor.dirty = true;
                }
            }
            KeyCode::Char('s') => {
                if editor.tab == Tab::Save {
                    editor.saved = true;
                    return Ok(false);
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
            KeyCode::Char('v') => {
                if editor.tab == Tab::Theme {
                    editor.cfg.logo.color_dir = if editor.cfg.logo.color_dir == "vertical" { "horizontal".into() } else { "vertical".into() };
                    editor.dirty = true;
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
            KeyCode::Esc => {
                if editor.tab == Tab::Save { return Ok(false); }
                return Ok(false);
            }
            _ => {}
        }
    }
    _ => {}
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

        // Auto-refresh every 1.5s when in monitor mode for live data
        if editor.monitor_mode {
            if crossterm::event::poll(std::time::Duration::from_millis(1500))? {
                if !handle_event(&mut editor)? {
                    break Ok(());
                }
            } else {
                editor.dirty = true;
                editor.refresh_preview();
            }
        } else if !handle_event(&mut editor)? {
            break Ok(());
        }
    };

    // Copy changes back if saved
    if editor.saved {
        cfg.logo = editor.cfg.logo.clone();
        cfg.panel = editor.cfg.panel.clone();
        cfg.display = editor.cfg.display.clone();
        cfg.scene = editor.cfg.scene.clone();
        cfg.title = editor.cfg.title.clone();
        cfg.separator = editor.cfg.separator.clone();
        cfg.palette = editor.cfg.palette.clone();
    }

    terminal::disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    res
}
