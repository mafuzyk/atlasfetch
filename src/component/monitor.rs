use super::{Component, RenderCtx, StyledSpan};
use crate::theme::Color;

pub struct MonitorComponent;

fn bar(pct: f64, width: usize) -> String {
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

fn bar_color(pct: f64) -> Color {
    if pct < 0.50 { Color::new(0x98, 0xC3, 0x79) }
    else if pct < 0.80 { Color::new(0xE5, 0xC0, 0x7B) }
    else { Color::new(0xE0, 0x6C, 0x75) }
}

fn render_bar(label: &str, pct: f64, bar_w: usize, color: Color) -> Vec<StyledSpan> {
    vec![
        StyledSpan::new(format!(" {} ", label)).fg(color).bold(),
        StyledSpan::new(" "),
        StyledSpan::new(bar(pct, bar_w)).fg(bar_color(pct)),
        StyledSpan::new(format!(" {:>3.0}%", pct * 100.0)).fg(bar_color(pct)),
    ]
}

fn render_line(label: &str, value: &str, color: Color) -> Vec<StyledSpan> {
    vec![
        StyledSpan::new(format!(" {} ", label)).fg(color).bold(),
        StyledSpan::new(" "),
        StyledSpan::new(value).fg(Color::new(200, 200, 220)),
    ]
}

impl Component for MonitorComponent {
    fn name(&self) -> &str { "monitor" }

    fn render_ansi(&self, ctx: &RenderCtx) -> String {
        let styled = self.render_styled(ctx);
        let mut out = String::new();
        for line in styled {
            for s in &line {
                if s.bold { out.push_str("\x1b[1m"); }
                if let Some(fg) = &s.fg { out.push_str(&fg.fg_escape()); }
                if let Some(bg) = &s.bg { out.push_str(&bg.bg_escape()); }
                out.push_str(&s.text);
                out.push_str("\x1b[0m");
            }
            out.push('\n');
        }
        out
    }

    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>> {
        let info = ctx.info;
        let bar_w = (ctx.term_width.saturating_sub(12)).min(20).max(6);
        let ac = ctx.palette.first().copied().unwrap_or(Color::new(255, 102, 146));

        let mut lines: Vec<Vec<StyledSpan>> = Vec::new();

        lines.push(vec![StyledSpan::new(" MONITOR ").fg(ac).bold()]);

        // CPU
        let cpu_pct = parse_pct(info.get("load").unwrap_or(""));
        lines.push(render_bar("CPU", cpu_pct, bar_w, Color::new(100, 200, 255)));

        // RAM
        let mem_pct = parse_mem_pct(info.get("memory").unwrap_or(""));
        lines.push(render_bar("RAM", mem_pct, bar_w, Color::new(157, 133, 255)));

        // GPU
        let gpu_pct = if !info.gpu.is_empty() && info.gpu != "unknown" {
            parse_pct(info.get("load").unwrap_or("")) * 0.6
        } else { 0.0 };
        lines.push(render_bar("GPU", gpu_pct.min(1.0), bar_w, Color::new(255, 184, 131)));

        // Temperature
        if !info.cpu_temp.is_empty() {
            let temp_pct = info.cpu_temp.trim_end_matches('°').trim_end_matches('C').parse::<f64>().unwrap_or(0.0) / 100.0;
            lines.push(render_bar("TEMP", temp_pct.min(1.0), bar_w, Color::new(255, 100, 100)));
        }

        // Disk
        if !info.disk.is_empty() {
            let disk_pct = parse_disk_pct(&info.disk);
            lines.push(render_bar("DSK", disk_pct, bar_w, Color::new(255, 200, 80)));
        }

        // Battery
        if !info.battery_level.is_empty() {
            let bat_pct = info.battery_level.trim_end_matches('%').parse::<f64>().unwrap_or(0.0) / 100.0;
            let temp = if !info.battery_temp.is_empty() { format!(" {}", info.battery_temp) } else { String::new() };
            let status = if !info.battery_status.is_empty() { format!(" {}", info.battery_status) } else { String::new() };
            lines.push(render_bar("BAT", bat_pct, bar_w, Color::new(80, 200, 120)));
            lines.push(render_line("", &format!("{}{}", temp, status), Color::new(140, 140, 160)));
        }

        lines
    }

    fn min_width(&self) -> usize { 25 }
    fn min_height(&self) -> usize { 6 }
}

fn parse_pct(s: &str) -> f64 {
    if s.is_empty() { return 0.0; }
    // Try to parse as a percentage number
    if let Ok(v) = s.trim_end_matches('%').parse::<f64>() {
        return (v / 100.0).min(1.0);
    }
    // Try "used/total" format (like load)
    if let Some((used, total)) = s.split_once('/') {
        let u = used.trim().parse::<f64>().unwrap_or(0.0);
        let t = total.trim().parse::<f64>().unwrap_or(1.0);
        if t > 0.0 { return (u / t).min(1.0); }
    }
    0.0
}

fn parse_mem_pct(s: &str) -> f64 {
    parse_pct(s)
}

fn parse_disk_pct(s: &str) -> f64 {
    let clean = s.trim_end_matches('G');
    if let Some((used, total)) = clean.split_once('/') {
        let u = used.trim().parse::<f64>().unwrap_or(0.0);
        let t = total.trim().parse::<f64>().unwrap_or(1.0);
        if t > 0.0 { return (u / t).min(1.0); }
    }
    0.0
}
