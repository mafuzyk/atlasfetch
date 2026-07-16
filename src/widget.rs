#![allow(dead_code)]

use unicode_width::UnicodeWidthStr;

use crate::config::{FieldDef, PanelConfig};
use crate::info::SysInfo;
use crate::theme::Color;
use crate::render::StyledSegment;

const RESET: &str = "\x1b[0m";
const BAR_FILL: &str = "█";
const BAR_EMPTY: &str = "░";

// ── Widget trait ─────────────────────────────────────────────────────────

/// Context passed to every widget during rendering.
#[derive(Debug, Clone)]
pub struct RenderCtx<'a> {
    pub info: &'a SysInfo,
    pub panel_cfg: &'a PanelConfig,
    pub max_width: usize,
    pub fg_color: Color,
}

/// The rendered output of a widget.
#[derive(Debug, Clone)]
pub struct WidgetOutput {
    /// ANSI-escaped string for direct terminal output.
    pub ansi: String,
    /// Structured segments for TUI rendering.
    pub styled: Vec<StyledSegment>,
    /// Visible (non-ANSI) width of the output.
    pub width: usize,
    /// Progress bar value (0.0–1.0) if this widget renders a bar.
    pub bar_value: Option<f64>,
}

/// A single information widget — the atomic unit of the display.
pub trait Widget: Send + Sync {
    /// Unique key (e.g. "os", "kernel", "cpu").
    fn key(&self) -> &str;

    /// Human-readable label.
    fn label(&self) -> &str;

    /// Icon (Nerd Font glyph or emoji).
    fn icon(&self) -> &str;

    /// Render the widget content.
    fn render(&self, ctx: &RenderCtx) -> WidgetOutput;

    /// Minimum width in columns the widget needs.
    fn min_width(&self) -> usize {
        4
    }
}

// ── FieldWidget ──────────────────────────────────────────────────────────

/// A widget backed by a FieldDef — the standard way to display a sysinfo field.
pub struct FieldWidget {
    pub def: FieldDef,
}

impl FieldWidget {
    pub fn new(field: impl Into<String>, icon: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            def: FieldDef {
                field: field.into(),
                icon: icon.into(),
                label: label.into(),
                enabled: true,
            },
        }
    }

    pub fn from_def(def: FieldDef) -> Self {
        Self { def }
    }
}

impl Widget for FieldWidget {
    fn key(&self) -> &str {
        &self.def.field
    }

    fn label(&self) -> &str {
        &self.def.label
    }

    fn icon(&self) -> &str {
        &self.def.icon
    }

    fn render(&self, ctx: &RenderCtx) -> WidgetOutput {
        let val = ctx.info.get(&self.def.field).unwrap_or("?");
        let max_w = ctx.max_width.min(ctx.panel_cfg.max_val_width);

        let sep = "\u{e0b0}";
        let seg = format!(" {} {} ", self.def.icon, self.def.label);
        let mut val_text = format!(" {} ", val);

        let seg_vis = seg.width();
        let val_vis = val_text.width();

        let sep_color = Color::from_hex_opt(&ctx.panel_cfg.sep_color)
            .unwrap_or(Color::new(157, 133, 255));
        let val_color = Color::from_hex_opt(&ctx.panel_cfg.val_color)
            .unwrap_or(Color::new(245, 220, 227));

        let is_compact = ctx.panel_cfg.max_val_width <= 35;
        let sep_space = if is_compact { "" } else { " " };

        if seg_vis + 1 + val_vis > max_w {
            let need = seg_vis + 1 + 3;
            if need > max_w {
                val_text = String::new();
            } else {
                let r = max_w.saturating_sub(seg_vis + 2);
                if r < 1 {
                    val_text = String::new();
                } else {
                    let visible_r = r.saturating_sub(2);
                    let mut visible: String = val.chars().take(visible_r.max(1)).collect();
                    if visible.len() < val.len() {
                        visible.pop();
                        visible.push('\u{2026}');
                    }
                    val_text = format!(" {} ", visible);
                }
            }
        }

        let ansi = if val_text.trim().is_empty() {
            format!(
                "{}{} {}{}{}",
                ctx.fg_color.fg_escape(),
                seg,
                sep_color.fg_escape(),
                sep,
                RESET,
            )
        } else {
            format!(
                "{}{}{}{}{}{}{}{}",
                ctx.fg_color.fg_escape(),
                seg,
                sep_color.fg_escape(),
                sep,
                sep_space,
                val_color.fg_escape(),
                val_text.trim(),
                RESET,
            )
        };

        let styled = if val_text.trim().is_empty() {
            vec![
                StyledSegment { text: seg, fg: Some(ctx.fg_color), bg: None, bold: false },
                StyledSegment { text: sep.into(), fg: Some(sep_color), bg: None, bold: false },
            ]
        } else {
            vec![
                StyledSegment { text: seg, fg: Some(ctx.fg_color), bg: None, bold: false },
                StyledSegment { text: sep.into(), fg: Some(sep_color), bg: None, bold: false },
                StyledSegment { text: sep_space.into(), fg: None, bg: None, bold: false },
                StyledSegment { text: val_text.trim().into(), fg: Some(val_color), bg: None, bold: false },
            ]
        };

        let width = crate::render::strip_ansi(&ansi).width();
        WidgetOutput { ansi, styled, width, bar_value: None }
    }

    fn min_width(&self) -> usize {
        let seg = format!(" {} {} ", self.def.icon, self.def.label);
        seg.width() + 2
    }
}

// ── MonitorWidget (progress bars) ───────────────────────────────────────

/// A widget that renders a progress bar (e.g. CPU ██████░░ 63%).
/// Falls back to text-only if the field has no numeric bar value.
pub struct MonitorWidget {
    pub def: FieldDef,
}

impl MonitorWidget {
    pub fn new(field: impl Into<String>, icon: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            def: FieldDef {
                field: field.into(),
                icon: icon.into(),
                label: label.into(),
                enabled: true,
            },
        }
    }

    pub fn from_def(def: FieldDef) -> Self {
        Self { def }
    }
}

impl Widget for MonitorWidget {
    fn key(&self) -> &str {
        &self.def.field
    }

    fn label(&self) -> &str {
        &self.def.label
    }

    fn icon(&self) -> &str {
        &self.def.icon
    }

    fn render(&self, ctx: &RenderCtx) -> WidgetOutput {
        let val = ctx.info.get(&self.def.field).unwrap_or("?");
        let bar = ctx.info.get_bar(&self.def.field);
        let max_w = ctx.max_width.min(ctx.panel_cfg.max_val_width);
        let bar_width = (max_w.saturating_sub(4) / 2).max(4); // at least 4

        if let Some(pct) = bar {
            let filled = (pct * bar_width as f64).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar_str: String = (0..filled).map(|_| '█').chain((0..empty).map(|_| '░')).collect();
            let pct_str = format!("{:>3.0}%", pct * 100.0);

            let label_seg = format!(" {} {} ", self.def.icon, self.def.label);
            let bar_color_ansi = bar_color(pct).fg_escape();
            let ansi = format!(
                "{}{}{} {}{}",
                ctx.fg_color.fg_escape(),
                label_seg.trim(),
                bar_color_ansi,
                bar_str,
                pct_str,
            );

            let styled = vec![
                StyledSegment { text: label_seg.trim().into(), fg: Some(ctx.fg_color), bg: None, bold: false },
                StyledSegment { text: " ".into(), fg: None, bg: None, bold: false },
                StyledSegment { text: bar_str, fg: Some(bar_color(pct)), bg: None, bold: false },
                StyledSegment { text: format!(" {}", pct_str), fg: Some(bar_color(pct)), bg: None, bold: false },
            ];

            let w = crate::render::strip_ansi(&ansi).width();
            return WidgetOutput { ansi, styled, width: w, bar_value: Some(pct) };
        }

        // No bar value: render as plain text (fallback)
        let sep = "\u{e0b0}";
        let seg = format!(" {} {} ", self.def.icon, self.def.label);
        let val_text = format!(" {} ", val);
        let sep_color = Color::from_hex_opt(&ctx.panel_cfg.sep_color)
            .unwrap_or(Color::new(157, 133, 255));
        let val_color = Color::from_hex_opt(&ctx.panel_cfg.val_color)
            .unwrap_or(Color::new(245, 220, 227));

        let ansi = format!(
            "{}{}{}{} {}{}",
            ctx.fg_color.fg_escape(),
            seg,
            sep_color.fg_escape(),
            sep,
            val_color.fg_escape(),
            val_text.trim(),
        );
        let styled = vec![
            StyledSegment { text: seg, fg: Some(ctx.fg_color), bg: None, bold: false },
            StyledSegment { text: sep.into(), fg: Some(sep_color), bg: None, bold: false },
            StyledSegment { text: " ".into(), fg: None, bg: None, bold: false },
            StyledSegment { text: val_text.trim().into(), fg: Some(val_color), bg: None, bold: false },
        ];
        let w = crate::render::strip_ansi(&ansi).width();
        WidgetOutput { ansi, styled, width: w, bar_value: None }
    }

    fn min_width(&self) -> usize {
        let seg = format!(" {} {} ", self.def.icon, self.def.label);
        seg.width() + 6
    }
}

fn bar_color(pct: f64) -> Color {
    if pct < 0.50 {
        Color::new(0x98, 0xC3, 0x79) // green
    } else if pct < 0.80 {
        Color::new(0xE5, 0xC0, 0x7B) // yellow
    } else {
        Color::new(0xE0, 0x6C, 0x75) // red
    }
}

// ── Widget registry ──────────────────────────────────────────────────────

/// A registry that maps field keys to widget instances.
pub struct Registry {
    widgets: Vec<Box<dyn Widget>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { widgets: Vec::new() }
    }

    pub fn register(&mut self, widget: Box<dyn Widget>) {
        self.widgets.push(widget);
    }

    pub fn get(&self, key: &str) -> Option<&dyn Widget> {
        self.widgets.iter().find(|w| w.key() == key).map(|w| w.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn Widget>] {
        &self.widgets
    }

    /// Build a registry from a list of enabled field definitions.
    pub fn from_fields<'a>(left: impl IntoIterator<Item = &'a FieldDef>, right: impl IntoIterator<Item = &'a FieldDef>) -> Self {
        let mut reg = Self::new();
        for fd in left.into_iter().chain(right).filter(|f| f.enabled) {
            reg.register(Box::new(FieldWidget::from_def(fd.clone())));
        }
        reg
    }
}
