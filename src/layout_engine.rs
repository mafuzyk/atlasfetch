#![allow(dead_code)]

use unicode_width::UnicodeWidthStr;

use crate::theme::Color;
use crate::widget::{RenderCtx, Widget, WidgetOutput};

// ── Layout definitions ───────────────────────────────────────────────────

/// Available display layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// ASCII left, info panels right (traditional fetch)
    Classic,
    /// No ASCII, just info
    Minimal,
    /// Single column, no ASCII
    Compact,
    /// ASCII centered at top, panels below
    Stack,
}

impl Layout {
    pub fn all() -> &'static [Layout] {
        &[Self::Classic, Self::Stack, Self::Minimal, Self::Compact]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Stack => "Stack",
            Self::Minimal => "Minimal",
            Self::Compact => "Compact",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Classic => "ASCII on the left, info panels on the right",
            Self::Stack => "ASCII centered at the top, info stacked below",
            Self::Minimal => "No ASCII, only info panels",
            Self::Compact => "Single column compact layout",
        }
    }

    /// Recommended minimum terminal width for this layout.
    pub fn min_width(&self) -> usize {
        match self {
            Self::Classic => 80,
            Self::Stack => 40,
            Self::Minimal => 30,
            Self::Compact => 20,
        }
    }
}

// ── Positioning ──────────────────────────────────────────────────────────

/// A single row in the layout.
pub struct LayoutRow {
    /// Left-side widget output (empty if none).
    pub left_widgets: Vec<WidgetOutput>,
    /// ASCII art line for this row (empty if no art at this row).
    pub logo_line: Option<String>,
    /// Right-side widget output (empty if none).
    pub right_widgets: Vec<WidgetOutput>,
}

/// The result of arranging widgets into a layout.
pub struct LayoutOutput {
    pub title: String,
    pub separator: String,
    pub rows: Vec<LayoutRow>,
}

// ── Layout Engine trait ──────────────────────────────────────────────────

/// Controls how widgets and ASCII art are positioned on screen.
pub trait LayoutEngine {
    fn layout(&self) -> Layout;

    fn arrange(
        &self,
        widgets: &[&dyn Widget],
        ascii_lines: &[String],
        cfg: &crate::config::Config,
        info: &crate::info::SysInfo,
        term_width: usize,
    ) -> LayoutOutput;
}

// ── Classic Layout ───────────────────────────────────────────────────────

/// Traditional fetch: ASCII left, info panels right, vertically aligned.
pub struct ClassicLayout;

impl LayoutEngine for ClassicLayout {
    fn layout(&self) -> Layout { Layout::Classic }

    fn arrange(
        &self,
        widgets: &[&dyn Widget],
        ascii_lines: &[String],
        cfg: &crate::config::Config,
        info: &crate::info::SysInfo,
        term_width: usize,
    ) -> LayoutOutput {
        let left_pad = cfg.panel.left_pad;
        let right_pad = cfg.panel.right_pad;
        let gap = cfg.panel.gap;
        let max_shift = cfg.panel.max_shift;

        let logo_width = ascii_lines.iter()
            .map(|l| l.trim_end().width())
            .max()
            .unwrap_or(0);

        let n = widgets.len();
        let lh = ascii_lines.len();

        // Check if the ASCII fits
        let logo_origin = if lh > 0 && logo_width < term_width {
            let origin_test = (term_width.saturating_sub(logo_width)) / 2;
            if origin_test >= left_pad + gap + 8
                && term_width >= origin_test + logo_width + gap + right_pad + max_shift + 8
            {
                origin_test
            } else {
                0
            }
        } else {
            0
        };

        let n_iter = if lh == 0 { n } else { lh };
        let start_row = if lh > 0 { lh.saturating_sub(n) / 2 } else { 0 };

        let mut rows = Vec::with_capacity(n_iter);

        for i in 0..n_iter {
            let in_range = if lh > 0 { i >= start_row && i < start_row + n } else { true };
            let shift = if in_range {
                let idx = if lh > 0 { i.saturating_sub(start_row) } else { i };
                cascade_offset(idx, n, max_shift)
            } else {
                0
            };

            let logo_line = if lh > 0 && i < lh {
                Some(ascii_lines[i].clone())
            } else {
                None
            };

            let mut left_widgets = Vec::new();
            let mut right_widgets = Vec::new();

            if in_range {
                let idx = if lh > 0 { i.saturating_sub(start_row) } else { i };
                if let Some(w) = widgets.get(idx) {
                    let color = cfg.logo.colors.get(idx).copied().unwrap_or(Color::new(255, 255, 255));
                    let left_avail = logo_origin.saturating_sub(left_pad + shift + gap).max(4);
                    let ctx = RenderCtx { info, panel_cfg: &cfg.panel, max_width: left_avail, fg_color: color };
                    left_widgets.push(w.render(&ctx));

                    let right_color = cfg.logo.colors.get((idx + 3) % cfg.logo.colors.len())
                        .copied().unwrap_or(Color::new(255, 255, 255));
                    let right_avail = term_width
                        .saturating_sub(logo_origin + logo_width + gap + right_pad + max_shift)
                        .max(4);
                    let ctx = RenderCtx { info, panel_cfg: &cfg.panel, max_width: right_avail, fg_color: right_color };
                    right_widgets.push(w.render(&ctx));
                }
            }

            rows.push(LayoutRow { left_widgets, logo_line, right_widgets });
        }

        LayoutOutput {
            title: cfg.title.format.clone(),
            separator: cfg.separator.char.repeat(cfg.separator.length.min(term_width.saturating_sub(4))),
            rows,
        }
    }
}

// ── Stack Layout ─────────────────────────────────────────────────────────

/// ASCII centered at top, widgets stacked below in a single column.
pub struct StackLayout;

impl LayoutEngine for StackLayout {
    fn layout(&self) -> Layout { Layout::Stack }

    fn arrange(
        &self,
        widgets: &[&dyn Widget],
        ascii_lines: &[String],
        cfg: &crate::config::Config,
        info: &crate::info::SysInfo,
        term_width: usize,
    ) -> LayoutOutput {
        let pad = cfg.panel.left_pad;

        let logo_width = ascii_lines.iter()
            .map(|l| l.trim_end().width())
            .max()
            .unwrap_or(0);
        let _block_center = term_width.saturating_sub(logo_width) / 2;

        let _logo_rows: Vec<_> = ascii_lines.iter().map(|l| {
            let trimmed = l.trim_end();
            format!("{:w$}", trimmed, w = logo_width)
        }).collect();

        let mut rows = Vec::new();

        // ASCII rows
        for line in ascii_lines {
            rows.push(LayoutRow {
                left_widgets: vec![],
                logo_line: Some(format!("{:w$}", line.trim_end(), w = logo_width)),
                right_widgets: vec![],
            });
        }

        // Blank separator
        rows.push(LayoutRow {
            left_widgets: vec![],
            logo_line: None,
            right_widgets: vec![],
        });

        // Widget rows
        for (i, w) in widgets.iter().enumerate() {
            let color = cfg.logo.colors.get(i).copied().unwrap_or(Color::new(255, 255, 255));
            let avail = term_width.saturating_sub(pad * 2).max(10);
            let ctx = RenderCtx { info, panel_cfg: &cfg.panel, max_width: avail, fg_color: color };
            rows.push(LayoutRow {
                left_widgets: vec![w.render(&ctx)],
                logo_line: None,
                right_widgets: vec![],
            });
        }

        LayoutOutput {
            title: cfg.title.format.clone(),
            separator: cfg.separator.char.repeat(cfg.separator.length.min(term_width.saturating_sub(4))),
            rows,
        }
    }
}

// ── Minimal Layout ───────────────────────────────────────────────────────

/// No ASCII art, just a compact list of info panels.
pub struct MinimalLayout;

impl LayoutEngine for MinimalLayout {
    fn layout(&self) -> Layout { Layout::Minimal }

    fn arrange(
        &self,
        widgets: &[&dyn Widget],
        _ascii_lines: &[String],
        cfg: &crate::config::Config,
        info: &crate::info::SysInfo,
        term_width: usize,
    ) -> LayoutOutput {
        let pad = cfg.panel.left_pad;
        let mut rows = Vec::new();

        for (i, w) in widgets.iter().enumerate() {
            let color = cfg.logo.colors.get(i).copied().unwrap_or(Color::new(255, 255, 255));
            let avail = term_width.saturating_sub(pad * 2).max(10);
            let ctx = RenderCtx { info, panel_cfg: &cfg.panel, max_width: avail, fg_color: color };
            rows.push(LayoutRow {
                left_widgets: vec![w.render(&ctx)],
                logo_line: None,
                right_widgets: vec![],
            });
        }

        LayoutOutput {
            title: cfg.title.format.clone(),
            separator: cfg.separator.char.repeat(cfg.separator.length.min(term_width.saturating_sub(4))),
            rows,
        }
    }
}

// ── Compact Layout ───────────────────────────────────────────────────────

/// Single column, no ASCII, tight spacing.
pub struct CompactLayout;

impl LayoutEngine for CompactLayout {
    fn layout(&self) -> Layout { Layout::Compact }

    fn arrange(
        &self,
        widgets: &[&dyn Widget],
        _ascii_lines: &[String],
        cfg: &crate::config::Config,
        info: &crate::info::SysInfo,
        term_width: usize,
    ) -> LayoutOutput {
        let pad = 1; // tighter padding
        let mut rows = Vec::new();

        for (i, w) in widgets.iter().enumerate() {
            let color = cfg.logo.colors.get(i).copied().unwrap_or(Color::new(255, 255, 255));
            let avail = term_width.saturating_sub(pad * 2).max(10);
            let ctx = RenderCtx { info, panel_cfg: &cfg.panel, max_width: avail, fg_color: color };
            rows.push(LayoutRow {
                left_widgets: vec![w.render(&ctx)],
                logo_line: None,
                right_widgets: vec![],
            });
        }

        LayoutOutput {
            title: String::new(),  // no title in compact
            separator: String::new(), // no separator in compact
            rows,
        }
    }
}

// ── Layout engine factory ────────────────────────────────────────────────

/// Get the layout engine for a given layout.
pub fn engine_for(layout: Layout) -> Box<dyn LayoutEngine> {
    match layout {
        Layout::Classic => Box::new(ClassicLayout),
        Layout::Stack => Box::new(StackLayout),
        Layout::Minimal => Box::new(MinimalLayout),
        Layout::Compact => Box::new(CompactLayout),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Cascade offset for panel staggering (same as render.rs).
fn cascade_offset(i: usize, total: usize, max_shift: usize) -> usize {
    if total <= 1 { return 0; }
    let mid = (total - 1) as f64 / 2.0;
    if mid <= 0.0 { return 0; }
    let rel = (i as f64 / mid - 1.0).abs();
    (rel * max_shift as f64).round() as usize
}

/// Convert layout output to ANSI string for terminal display.
pub fn render_layout_output(
    output: &LayoutOutput,
    _ascii_lines: &[String],
    term_width: usize,
) -> String {
    let mut out = String::new();
    let reset = "\x1b[0m";
    let bold = "\x1b[1m";

    // Title
    if !output.title.is_empty() {
        let title_color = Color::from_hex_opt("#FF9A98").unwrap_or(Color::new(255, 154, 152));
        out.push_str(&format!("\n{}  {}{}{}{}\n", title_color.fg_escape(), bold, output.title, reset, reset));
    }

    // Separator
    if !output.separator.is_empty() {
        let sep_color = Color::from_hex_opt("#9D85FF").unwrap_or(Color::new(157, 133, 255));
        out.push_str(&format!("{}  {}{}{}\n", sep_color.fg_escape(), output.separator, reset, reset));
    }

    // Rows
    for row in &output.rows {
        let mut line = String::new();

        // Left widgets
        for w in &row.left_widgets {
            line.push_str(&w.ansi);
        }

        // Logo line
        if let Some(logo) = &row.logo_line {
            // Center the logo line
            let center = term_width.saturating_sub(logo.trim_end().width()) / 2;
            let mut logo_part = " ".repeat(center);
            for ch in logo.trim_end().chars() {
                if ch != ' ' {
                    logo_part.push_str(&format!("\x1b[38;5;231m{}", ch)); // white as fallback
                } else {
                    logo_part.push(' ');
                }
            }
            line = logo_part; // For now, logo replaces the line
        }

        // Right widgets
        for w in &row.right_widgets {
            let remaining = term_width.saturating_sub(line.width());
            if remaining > w.width {
                line.push_str(&" ".repeat(remaining - w.width));
            }
            line.push_str(&w.ansi);
        }

        let vis = line.width();
        if vis < term_width {
            line.push_str(&" ".repeat(term_width.saturating_sub(vis)));
        }
        line.push_str(reset);
        line.push('\n');
        out.push_str(&line);
    }

    out
}
