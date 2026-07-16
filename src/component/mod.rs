pub mod ascii;
pub mod companion;
pub mod monitor;
pub mod system;

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
    SplitMonitor,
}

#[allow(dead_code)]
impl Scene {
    pub fn all() -> &'static [Scene] {
        &[Scene::Classic, Scene::Dashboard, Scene::Cockpit, Scene::SplitMonitor]
    }
    pub fn name(&self) -> &'static str {
        match self {
            Scene::Classic => "Classic",
            Scene::Dashboard => "Terminal Dashboard",
            Scene::Cockpit => "Cockpit",
            Scene::SplitMonitor => "Split Monitor",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Scene::Classic => "Traditional fetch: ASCII left, info right",
            Scene::Dashboard => "Multi-panel TUI dashboard layout",
            Scene::Cockpit => "ASCII centered, panels arranged around it",
            Scene::SplitMonitor => "Fetch and monitor side by side",
        }
    }
    pub fn min_width(&self) -> usize {
        match self {
            Scene::Classic => 80,
            Scene::Dashboard => 80,
            Scene::Cockpit => 60,
            Scene::SplitMonitor => 90,
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
        Scene::SplitMonitor => render_split_monitor(components, ctx),
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
        let side = (term_w.saturating_sub(t.len())) / 2;
        let header = format!("{}{}{}", sep.repeat(side), t, sep.repeat(side));
        out.push(vec![StyledSpan::new(header)]);
    }
    for line in lines {
        let mut padded = line.to_vec();
        let w: usize = line.iter().map(|s| s.text.len()).sum();
        if w < term_w {
            padded.push(StyledSpan::new(" ".repeat(term_w - w)));
        }
        out.push(padded);
    }
    out
}

fn render_classic(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");
    let companion = find_component(components, "companion");

    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let mon_lines = monitor.map(|c| c.render_styled(ctx)).unwrap_or_default();

    let aw = ascii_lines.iter().map(|l| l.iter().map(|s| s.text.len()).sum::<usize>()).max().unwrap_or(0);
    let gap = 2;
    let right_w = ctx.term_width.saturating_sub(aw + gap + 4);

    let n = ascii_lines.len().max(sys_lines.len()).max(mon_lines.len());
    let mut all = Vec::new();

    for i in 0..n {
        let mut line = Vec::new();
        if i < ascii_lines.len() {
            line.extend(ascii_lines[i].clone());
        }
        let cur_w: usize = line.iter().map(|s| s.text.len()).sum();
        if cur_w < aw + gap + 2 {
            line.push(StyledSpan::new(" ".repeat(aw + gap + 2 - cur_w)));
        }
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        if i < mon_lines.len() {
            let cur_w: usize = line.iter().map(|s| s.text.len()).sum();
            if right_w > 0 && cur_w + 2 < ctx.term_width {
                line.push(StyledSpan::new(" ".repeat(2)));
                line.extend(mon_lines[i].clone());
            }
        }
        let w: usize = line.iter().map(|s| s.text.len()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }

    // Companion section below the main block
    if let Some(comp) = companion {
        let comp_lines = comp.render_styled(ctx);
        if !comp_lines.is_empty() {
            let sep = "\u{2500}".repeat(ctx.term_width.min(40));
            all.push(vec![StyledSpan::new(format!(" {} ", sep))]);
            all.extend(comp_lines.iter().cloned());
        }
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
            let w: usize = line.iter().map(|s| s.text.len()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        } else if i < sys_lines.len() {
            let w: usize = line.iter().map(|s| s.text.len()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        }
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.len()).sum();
        if w < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w))); }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2500}".repeat(ctx.term_width))]);

    for i in 0..mon_lines.len().max(comp_lines.len()) {
        let mut line = Vec::new();
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
            let w: usize = line.iter().map(|s| s.text.len()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        } else {
            let w: usize = line.iter().map(|s| s.text.len()).sum();
            if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        }
        if i < comp_lines.len() {
            line.extend(comp_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.len()).sum();
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
        let w: usize = line.iter().map(|s| s.text.len()).sum();
        let pad = ctx.term_width.saturating_sub(w) / 2;
        if pad > 0 { l.push(StyledSpan::new(" ".repeat(pad))); }
        l.extend(line.clone());
        let w2: usize = l.iter().map(|s| s.text.len()).sum();
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
        let w: usize = line.iter().map(|s| s.text.len()).sum();
        if w < half { line.push(StyledSpan::new(" ".repeat(half - w))); }
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        }
        let w2: usize = line.iter().map(|s| s.text.len()).sum();
        if w2 < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w2))); }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);
    SceneOutput { lines: all }
}

fn render_split_monitor(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let ascii_lines = ascii.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let sys_lines = system.map(|c| c.render_styled(ctx)).unwrap_or_default();
    let mon_lines = monitor.map(|c| wrap_block("MONITOR", &c.render_styled(ctx), 35)).unwrap_or_default();

    let half = ctx.term_width / 2;
    let mut all = Vec::new();

    let n = ascii_lines.len().max(sys_lines.len()).max(mon_lines.len());

    for i in 0..n {
        let mut line = Vec::new();

        // Left: ASCII + system
        if i < ascii_lines.len() {
            line.extend(ascii_lines[i].clone());
            let w: usize = line.iter().map(|s| s.text.len()).sum();
            if w < half / 2 { line.push(StyledSpan::new(" ".repeat(half / 2 - w))); }
        }
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }

        let w_left: usize = line.iter().map(|s| s.text.len()).sum();
        if w_left < half {
            line.push(StyledSpan::new(" ".repeat(half - w_left)));
        }

        // Right: monitor
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        }

        let w: usize = line.iter().map(|s| s.text.len()).sum();
        if w < ctx.term_width { line.push(StyledSpan::new(" ".repeat(ctx.term_width - w))); }
        all.push(line);
    }
    SceneOutput { lines: all }
}
