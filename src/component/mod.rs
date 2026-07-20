pub mod ascii;
pub mod companion;
pub mod monitor;
pub mod system;

use std::any::Any;
use unicode_width::UnicodeWidthStr;

use crate::config::Config;
use crate::info::SysInfo;
use crate::theme::Color;

pub trait Component: Send + Sync {
    fn name(&self) -> &str;
    #[allow(dead_code)]
    fn render_ansi(&self, ctx: &RenderCtx) -> String;
    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>>;
    #[allow(dead_code)]
    fn min_width(&self) -> usize;
    #[allow(dead_code)]
    fn min_height(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
}

pub struct RenderCtx<'a> {
    pub info: &'a SysInfo,
    pub cfg: &'a Config,
    pub term_width: usize,
    pub palette: &'a [Color],
}

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), fg: None, bg: None, bold: false }
    }
    pub fn fg(mut self, c: Color) -> Self { self.fg = Some(c); self }
    pub fn bold(mut self) -> Self { self.bold = true; self }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scene {
    Classic,
    Dashboard,
    Cockpit,
    ClassicFetch,
}

#[allow(dead_code)]
impl Scene {
    pub fn all() -> &'static [Scene] {
        &[Scene::Classic, Scene::Dashboard, Scene::Cockpit, Scene::ClassicFetch]
    }
    pub fn name(&self) -> &'static str {
        match self {
            Scene::Classic => "Classic",
            Scene::Dashboard => "Terminal Dashboard",
            Scene::Cockpit => "Cockpit",
            Scene::ClassicFetch => "ClassicFetch",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Scene::Classic => "ASCII centered, left/right powerline panels",
            Scene::Dashboard => "Multi-panel TUI dashboard layout",
            Scene::Cockpit => "ASCII centered, panels arranged around it",
            Scene::ClassicFetch => "Fastfetch-style: info left, logo right, no panels",
        }
    }
    pub fn min_width(&self) -> usize {
        match self {
            Scene::Classic => 80,
            Scene::Dashboard => 80,
            Scene::Cockpit => 60,
            Scene::ClassicFetch => 70,
        }
    }
}

pub struct SceneOutput {
    pub lines: Vec<Vec<StyledSpan>>,
}

pub fn render_scene_ansi(scene: Scene, components: &[&dyn Component], ctx: &RenderCtx) -> String {
    let output = render_scene(scene, components, ctx);
    spans_to_ansi(&output.lines)
}

fn spans_to_ansi(lines: &[Vec<StyledSpan>]) -> String {
    let mut out = String::new();
    for line in lines {
        for span in line {
            if span.bold { out.push_str("\x1b[1m"); }
            if let Some(fg) = &span.fg { out.push_str(&fg.fg_escape()); }
            if let Some(bg) = &span.bg { out.push_str(&bg.bg_escape()); }
            out.push_str(&span.text);
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

pub fn render_scene(scene: Scene, components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    match scene {
        Scene::Classic => render_classic(components, ctx),
        Scene::Dashboard => render_dashboard(components, ctx),
        Scene::Cockpit => render_cockpit(components, ctx),
        Scene::ClassicFetch => render_classicfetch(components, ctx),
    }
}

fn find_component<'a>(components: &[&'a dyn Component], name: &str) -> Option<&'a dyn Component> {
    components.iter().find(|c| c.name() == name).copied()
}

fn wrap_block(title: &str, lines: &[Vec<StyledSpan>], term_w: usize) -> Vec<Vec<StyledSpan>> {
    let mut out = Vec::new();
    if !title.is_empty() {
        let sep = "\u{2500}";
        let t = format!(" {} ", title);
        let side = (term_w.saturating_sub(t.width())) / 2;
        let header = format!("{}{}{}", sep.repeat(side), t, sep.repeat(side));
        out.push(vec![StyledSpan::new(header)]);
    }
    for line in lines {
        let mut padded = line.to_vec();
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < term_w {
            padded.push(StyledSpan::new(" ".repeat(term_w - w)));
        }
        out.push(padded);
    }
    out
}

fn cascade_offset(i: usize, total: usize, max_shift: usize) -> usize {
    if total <= 1 { return 0; }
    let mid = (total - 1) as f64 / 2.0;
    if mid <= 0.0 { return 0; }
    let rel = (i as f64 / mid - 1.0).abs();
    (rel * max_shift as f64).round() as usize
}

fn render_classic(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let (ascii_lines, ascii_w) = ascii
        .and_then(|c| c.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|c| c.render_colored_lines(ctx))
        .unwrap_or_default();

    let sys = system
        .and_then(|c| c.as_any().downcast_ref::<system::SystemComponent>());

    let left_pad = ctx.cfg.panel.left_pad;
    let right_pad = ctx.cfg.panel.right_pad;
    let gap = ctx.cfg.panel.gap.max(1);
    let max_shift = ctx.cfg.panel.max_shift;

    let logo_origin = if ascii_w > 0 && ascii_w < ctx.term_width {
        (ctx.term_width.saturating_sub(ascii_w)) / 2
    } else {
        0
    };

    // Available width for left/right panels (accounting for padding, gap, shift, and ASCII block)
    let left_avail = logo_origin.saturating_sub(left_pad + max_shift + gap + 1).max(4);
    let right_avail = ctx.term_width
        .saturating_sub(logo_origin + ascii_w + gap + right_pad + max_shift + 1)
        .max(4);

    let left_lines = sys.map(|s| s.render_left_styled(ctx, left_avail)).unwrap_or_default();
    let right_lines = sys.map(|s| s.render_right_styled(ctx, right_avail)).unwrap_or_default();
    let mon_lines = monitor.map(|c| c.render_styled(ctx)).unwrap_or_default();

    let left_w = left_lines.iter()
        .map(|row| row.iter().map(|s| s.text.width()).sum::<usize>())
        .max().unwrap_or(0);
    let right_w = right_lines.iter()
        .map(|row| row.iter().map(|s| s.text.width()).sum::<usize>())
        .max().unwrap_or(0);

    let lh = ascii_lines.len();
    let n = left_lines.len().max(right_lines.len()).max(mon_lines.len());
    let start_row = if lh > 0 { lh.saturating_sub(n) / 2 } else { 0 };
    let total_rows = if lh == 0 { n } else { lh.max(start_row + n) };

    let mut all = Vec::new();

    // ── Title ──
    let title_color = Color::from_hex_opt(&ctx.cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let title_text = ctx.cfg.title.format
        .replace("{user}", &ctx.info.user)
        .replace("{host}", &ctx.info.host);
    if !title_text.is_empty() {
        let mut title_line = vec![
            StyledSpan::new("  "),
            StyledSpan::new(title_text).fg(title_color).bold(),
        ];
        let tw: usize = title_line.iter().map(|s| s.text.width()).sum();
        if tw < ctx.term_width {
            title_line.push(StyledSpan::new(" ".repeat(ctx.term_width - tw)));
        }
        all.push(title_line);
    }

    // ── Separator ──
    let sep_color = Color::from_hex_opt(&ctx.cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let sep_len = ctx.cfg.separator.length.min(ctx.term_width.saturating_sub(4));
    if sep_len > 0 {
        let sep_str: String = ctx.cfg.separator.char.repeat(sep_len);
        let mut sep_line = vec![
            StyledSpan::new("  "),
            StyledSpan::new(sep_str).fg(sep_color),
        ];
        let sw: usize = sep_line.iter().map(|s| s.text.width()).sum();
        if sw < ctx.term_width {
            sep_line.push(StyledSpan::new(" ".repeat(ctx.term_width - sw)));
        }
        all.push(sep_line);
    }

    for i in 0..total_rows {
        let in_range = lh == 0 || (i >= start_row && i < start_row + n);
        let row_idx = if lh > 0 { i.saturating_sub(start_row) } else { i };
        let s = cascade_offset(row_idx, n, max_shift);

        let mut line = Vec::new();

        if in_range {
            // ── Left padding with cascade shift ──
            line.push(StyledSpan::new(" ".repeat(left_pad + s)));

            // ── Left panel (right-aligned within its block) ──
            if row_idx < left_lines.len() {
                let cw: usize = left_lines[row_idx].iter().map(|s| s.text.width()).sum();
                if cw < left_w {
                    line.push(StyledSpan::new(" ".repeat(left_w - cw)));
                }
                line.extend(left_lines[row_idx].clone());
            } else {
                line.push(StyledSpan::new(" ".repeat(left_w.max(1))));
            }

            // ── Gap before ASCII ──
            let cur: usize = line.iter().map(|s| s.text.width()).sum();
            let target = logo_origin.saturating_sub(gap);
            if target > cur {
                line.push(StyledSpan::new(" ".repeat(target - cur)));
            }
            if ascii_w > 0 {
                line.push(StyledSpan::new(" ".repeat(gap)));
            }

            // ── ASCII ──
            if i < ascii_lines.len() {
                line.extend(ascii_lines[i].clone());
            } else if ascii_w > 0 {
                line.push(StyledSpan::new(" ".repeat(ascii_w)));
            }

            // ── Right panel (left-aligned) ──
            if row_idx < right_lines.len() {
                let cur: usize = line.iter().map(|s| s.text.width()).sum();
                let r_target = ctx.term_width.saturating_sub(right_pad + s + right_w);
                if r_target > cur {
                    line.push(StyledSpan::new(" ".repeat(r_target - cur)));
                }
                line.extend(right_lines[row_idx].clone());
            }

            // ── Monitor panel ──
            if row_idx < mon_lines.len() {
                let cur: usize = line.iter().map(|s| s.text.width()).sum();
                let mon_w: usize = mon_lines[row_idx].iter().map(|s| s.text.width()).sum();
                if cur + 2 + mon_w <= ctx.term_width {
                    line.push(StyledSpan::new(" ".repeat(2)));
                    line.extend(mon_lines[row_idx].clone());
                }
            }
        } else {
            // ── ASCII only (no panels) — center the logo ──
            if logo_origin > 0 {
                line.push(StyledSpan::new(" ".repeat(logo_origin)));
            }
            if i < ascii_lines.len() {
                line.extend(ascii_lines[i].clone());
            }
        }

        // ── Fill remaining width ──
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }

        all.push(line);
    }

    SceneOutput { lines: all }
}

fn render_dashboard(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");
    let companion = find_component(components, "companion");

    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| wrap_block("SYSTEM", &c.render_styled(ctx), 30)).unwrap_or_default();
    let mon_lines = monitor.map(|c| wrap_block("MONITOR", &c.render_styled(ctx), 30)).unwrap_or_default();
    let comp_lines = companion.map(|c| wrap_block("STATUS", &c.render_styled(ctx), 30)).unwrap_or_default();

    let half = ctx.term_width / 2;
    let mut all = Vec::new();

    for i in 0..ascii_lines.len().max(sys_lines.len()) {
        let mut line = Vec::new();
        if i < ascii_lines.len() {
            line.extend(ascii_lines[i].clone());
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        } else if i < sys_lines.len() {
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        }
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w))); }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2500}".repeat(ctx.term_width))]);

    for i in 0..mon_lines.len().max(comp_lines.len()) {
        let mut line = Vec::new();
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        } else {
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        }
        if i < comp_lines.len() {
            line.extend(comp_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w))); }
        all.push(line);
    }
    SceneOutput { lines: all }
}

fn render_cockpit(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let mon_lines = monitor.map(|c| c.render_styled(ctx)).unwrap_or_default();

    let mut all = Vec::new();

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    for line in &ascii_lines {
        let mut l = Vec::new();
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        let pad = ctx.term_width.saturating_sub(w) / 2;
        if pad > 0 { l.push(StyledSpan::new(" ".repeat(pad))); }
        l.extend(line.clone());
        let w2: usize = l.iter().map(|s| s.text.width()).sum();
        if w2 < ctx.term_width { l.push(StyledSpan::new(" ".repeat(ctx.term_width - w2))); }
        all.push(l);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    let half = ctx.term_width / 2;
    for i in 0..sys_lines.len().max(mon_lines.len()) {
        let mut line = Vec::new();
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        }
        let w2: usize = line.iter().map(|s| s.text.width()).sum();
        if w2 < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w2))); }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);
    SceneOutput { lines: all }
}

pub fn render_monitor_split(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let half = ctx.term_width / 2;
    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| c.render_styled(ctx)).unwrap_or_default();

    // Monitor rendered in its own half
    let mon_ctx = RenderCtx {
        info: ctx.info,
        cfg: ctx.cfg,
        term_width: half.saturating_sub(4),
        palette: ctx.palette,
    };
    let mon_lines = monitor.map(|c| wrap_block(" MONITOR ", &c.render_styled(&mon_ctx), half.saturating_sub(4))).unwrap_or_default();

    let n = ascii_lines.len().max(sys_lines.len()).max(mon_lines.len());
    let mut all = Vec::new();

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    for i in 0..n {
        let mut line = Vec::new();

        // Left half: fetch (ASCII + system)
        let mut left = Vec::new();
        if i < ascii_lines.len() {
            left.extend(ascii_lines[i].clone());
        }
        if i < sys_lines.len() {
            let w: usize = left.iter().map(|s| s.text.width()).sum();
            if w < half {
                // try to fit sys on same line if possible
            }
            left.extend(sys_lines[i].clone());
        }
        let w_left: usize = left.iter().map(|s| s.text.width()).sum();
        if w_left > half {
            // overflow — put sys on its own line
            left.clear();
            if i < ascii_lines.len() {
                left.extend(ascii_lines[i].clone());
                let w: usize = left.iter().map(|s| s.text.width()).sum();
                if w < half { left.push(StyledSpan::new(" ".repeat(half - w))); }
            }
        } else {
            if w_left < half {
                left.push(StyledSpan::new(" ".repeat(half - w_left)));
            }
        }
        line.extend(left);

        // Right half: monitor
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        } else {
            line.push(StyledSpan::new(" ".repeat(half.saturating_sub(4))));
        }

        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);
    SceneOutput { lines: all }
}

fn render_classicfetch(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");

    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let ascii_w = ascii_lines.iter()
        .flat_map(|l| l.iter())
        .map(|s| s.text.width())
        .max()
        .unwrap_or(0);

    let gap = 4;
    let left_w = if ascii_w > 0 { ctx.term_width.saturating_sub(ascii_w + gap) } else { ctx.term_width };

    let n = ascii_lines.len().max(sys_lines.len());
    let mut all = Vec::new();

    for i in 0..n {
        let mut line = Vec::new();

        // Left: system info (right-aligned within its block or left-aligned)
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < left_w {
                line.push(StyledSpan::new(" ".repeat(left_w - w)));
            }
        } else if ascii_w > 0 {
            line.push(StyledSpan::new(" ".repeat(left_w)));
        }

        // Gap + ASCII on the right
        if i < ascii_lines.len() && ascii_w > 0 {
            if left_w > 0 {
                let cur: usize = line.iter().map(|s| s.text.width()).sum();
                let target = ctx.term_width.saturating_sub(ascii_w);
                if cur < target {
                    line.push(StyledSpan::new(" ".repeat(target - cur)));
                }
            }
            line.extend(ascii_lines[i].clone());
        }

        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }
    SceneOutput { lines: all }
}
