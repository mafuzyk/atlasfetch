use unicode_width::UnicodeWidthStr;

use crate::config::{FieldDef, PanelConfig};
use crate::info::SysInfo;
use crate::theme::Color;
use crate::render::StyledSegment;

const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone)]
pub struct RenderCtx<'a> {
    pub info: &'a SysInfo,
    pub panel_cfg: &'a PanelConfig,
    pub max_width: usize,
    pub fg_color: Color,
}

#[derive(Debug, Clone)]
pub struct WidgetOutput {
    pub ansi: String,
    pub styled: Vec<StyledSegment>,
    pub width: usize,
}

pub struct FieldWidget {
    pub def: FieldDef,
}

impl FieldWidget {
    pub fn from_def(def: FieldDef) -> Self {
        Self { def }
    }

    pub fn render_inherent(&self, ctx: &RenderCtx) -> WidgetOutput {
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
        WidgetOutput { ansi, styled, width }
    }
}
