use color_eyre::Result;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use crate::config::{Config, FieldDef};
use crate::info::SysInfo;
use crate::theme::Color;
use crate::widget::{FieldWidget, RenderCtx, Widget};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

// ── Styled output (used by TUI preview) ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct StyledLine {
    pub segments: Vec<StyledSegment>,
}

#[derive(Debug, Clone)]
pub struct StyledSegment {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

// ── Terminal output ──────────────────────────────────────────────────────

/// Render the full fetch output as an ANSI string for terminal display.
pub fn render(cfg: &Config, info: &SysInfo, ascii_art: &str) -> Result<String> {
    let term_width = terminal_width();
    let mut out = String::new();

    let logo_lines: Vec<&str> = if ascii_art.is_empty() {
        Vec::new()
    } else {
        ascii_art.lines().collect()
    };

    let left_fields: Vec<&FieldDef> = cfg.display.left.iter().filter(|f| f.enabled).collect();
    let right_fields: Vec<&FieldDef> = cfg.display.right.iter().filter(|f| f.enabled).collect();
    let n = left_fields.len().max(right_fields.len());

    let left_pad = cfg.panel.left_pad;
    let right_pad = cfg.panel.right_pad;
    let gap = cfg.panel.gap;
    let max_shift = cfg.panel.max_shift;

    // ── Logo fitting check (original Python logic) ──
    let logo_lines = if logo_lines.is_empty() {
        logo_lines
    } else {
        let lw_test = logo_lines.iter().map(|l| UnicodeWidthStr::width(*l)).max().unwrap_or(0);
        let origin_test = (term_width.saturating_sub(lw_test)) / 2;
        if origin_test < left_pad + gap + 8
            || term_width < origin_test + lw_test + gap + right_pad + max_shift + 8
        {
            Vec::new()
        } else {
            logo_lines
        }
    };

    let lh = logo_lines.len();
    let logo_width = logo_lines.iter().map(|l| UnicodeWidthStr::width(*l)).max().unwrap_or(0);
    let logo_origin = if lh > 0 && logo_width < term_width {
        (term_width.saturating_sub(logo_width)) / 2
    } else {
        0
    };
    let logo_origin_rel = logo_origin;

    // ── Title ──
    let title_color = Color::from_hex_opt(&cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let title_text = cfg.title.format.replace("{user}", &info.user).replace("{host}", &info.host);
    out.push_str(&format!("\n{}  {}{}{}{}\n", title_color.fg_escape(), BOLD, title_text, RESET, RESET));

    // ── Separator ──
    let sep_color = Color::from_hex_opt(&cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let sep_len = cfg.separator.length.min(term_width.saturating_sub(4));
    let sep_str: String = cfg.separator.char.repeat(sep_len);
    out.push_str(&format!("{}  {}{}{}\n", sep_color.fg_escape(), sep_str, RESET, RESET));

    // ── Body ──
    let n_iter = if lh == 0 { n } else { lh };
    let start_row = if lh > 0 { lh.saturating_sub(n) / 2 } else { 0 };

    for i in 0..n_iter {
        let in_range = if lh > 0 { i >= start_row && i < start_row + n } else { true };
        let shift = if lh > 0 && !in_range {
            let mid_idx = if n > 0 { i.saturating_sub(start_row) + n / 2 } else { 0 };
            cascade_offset(mid_idx, n, max_shift)
        } else if in_range {
            let idx = if lh > 0 { i.saturating_sub(start_row) } else { i };
            cascade_offset(idx, n, max_shift)
        } else {
            0
        };

        let logo_color = if lh > 0 && !cfg.logo.colors.is_empty() {
            cfg.logo.colors[i % cfg.logo.colors.len()]
        } else {
            Color::new(255, 255, 255)
        };

        if in_range {
            let idx = if lh > 0 { i.saturating_sub(start_row) } else { i };
            let left_def = left_fields.get(idx);
            let right_def = right_fields.get(idx);

            let logo_left_color = if !cfg.logo.colors.is_empty() {
                cfg.logo.colors[idx % cfg.logo.colors.len()]
            } else {
                Color::new(255, 255, 255)
            };
            let logo_right_color = if !cfg.logo.colors.is_empty() {
                cfg.logo.colors[(idx + 3) % cfg.logo.colors.len()]
            } else {
                Color::new(255, 255, 255)
            };

            // Build left panel
            let (left_text, left_vis) = if let Some(fd) = left_def {
                let avail = logo_origin_rel.saturating_sub(left_pad + shift + gap).max(4);
                build_panel(fd, info, &cfg.panel, logo_left_color, avail)
            } else {
                (String::new(), 0)
            };

            // Build right panel
            let (right_text, right_vis) = if let Some(fd) = right_def {
                let avail = term_width
                    .saturating_sub(logo_origin_rel + logo_width + gap + right_pad + max_shift)
                    .max(4);
                build_panel(fd, info, &cfg.panel, logo_right_color, avail)
            } else {
                (String::new(), 0)
            };

            // Build row
            let mut row = String::new();
            row.push_str(&" ".repeat(left_pad + shift));
            row.push_str(&left_text);

            let left_pad_extra = logo_origin_rel.saturating_sub(left_pad + shift + left_vis + gap);
            row.push_str(&" ".repeat(left_pad_extra));

            row.push_str(&" ".repeat(gap));

            // ASCII (block placement, no per-line centering)
            if lh > 0 && i < lh {
                let trimmed = logo_lines[i].trim_end();
                let padded = format!("{:width$}", trimmed, width = logo_width);
                for ch in padded.chars() {
                    if ch != ' ' {
                        row.push_str(&format!("{}{}", logo_color.fg_escape(), ch));
                    } else {
                        row.push(' ');
                    }
                }
            }

            // Right panel positioning
            if !right_text.trim().is_empty() {
                let r_target = term_width.saturating_sub(right_pad + shift);
                let gap_needed = r_target.saturating_sub(right_vis + visible_width(&row));
                if gap_needed > 0 {
                    row.push_str(&" ".repeat(gap_needed));
                }
                row.push_str(&right_text);
            }

            // Ensure row is exactly term_width chars
            let rv = visible_width(&row);
            if rv > term_width {
                row = trim_to_width(&row, term_width);
            } else if rv < term_width {
                row.push_str(&" ".repeat(term_width - rv));
            }

            row.push_str(RESET);
            row.push('\n');
            out.push_str(&row);
        } else {
            // No panels, just ASCII
            let mut row = String::new();
            row.push_str(&" ".repeat(logo_origin_rel));
            if i < lh {
                let trimmed = logo_lines[i].trim_end();
                let padded = format!("{:width$}", trimmed, width = logo_width);
                for ch in padded.chars() {
                    if ch != ' ' {
                        row.push_str(&format!("{}{}", logo_color.fg_escape(), ch));
                    } else {
                        row.push(' ');
                    }
                }
            }
            let rv = visible_width(&row);
            if rv < term_width {
                row.push_str(&" ".repeat(term_width - rv));
            }
            row.push_str(RESET);
            row.push('\n');
            out.push_str(&row);
        }
    }

    out.push('\n');
    Ok(out)
}

// ── Standalone ASCII render ──────────────────────────────────────────────

/// Render only the ASCII art, colored and centered, without any system info.
pub fn render_ascii_only(cfg: &Config, ascii_art: &str) -> String {
    let term_width = terminal_width();
    let raw: Vec<String> = ascii_art.lines().map(|l| l.to_string()).collect();
    let lines = dedent(&raw);

    if lines.is_empty() {
        return String::new();
    }

    let base = Color::new(255, 255, 255);
    let max_w = lines.iter().map(|l| l.trim_end().width()).max().unwrap_or(0);
    let center = term_width.saturating_sub(max_w) / 2;
    let is_vert = cfg.logo.color_dir == "vertical";

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end();
        out.push_str(&" ".repeat(center));
        let padded = format!("{:w$}", trimmed, w = max_w);
        for (ci, ch) in padded.chars().enumerate() {
            let idx = if is_vert { ci } else { i };
            let color = cfg.logo.colors.get(idx % cfg.logo.colors.len()).unwrap_or(&base);
            if ch != ' ' {
                out.push_str(&format!("{}{}", color.fg_escape(), ch));
            } else {
                out.push(' ');
            }
        }
        out.push_str(RESET);
        out.push('\n');
    }
    out
}

// ── Mobile ASCII compaction ──────────────────────────────────────────────

/// Strip common leading whitespace from all lines.
fn dedent(lines: &[String]) -> Vec<String> {
    let min_indent = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);
    lines.iter().map(|l| {
        l.chars().skip(min_indent).collect::<String>()
    }).collect()
}

/// Compact ASCII art for mobile: dedent lines for proper centering.
fn compact_for_mobile(ascii_art: &str) -> Vec<String> {
    let lines: Vec<&str> = ascii_art.lines().collect();
    let owned: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    dedent(&owned)
}

// ── Mobile renderer (ASCII left + panels right, or single column) ──────────

/// Render fetch output for mobile/narrow terminals.
pub fn render_mobile(cfg: &Config, info: &SysInfo, ascii_art: &str, is_narrow: bool) -> Result<String> {
    let term_width = terminal_width();
    let mut out = String::new();

    let compacted = compact_for_mobile(ascii_art);
    let logo_lines: Vec<&str> = if ascii_art.is_empty() || is_narrow {
        Vec::new()
    } else {
        compacted.iter().map(|s| s.as_str()).collect()
    };

    // Merge all enabled fields into a single list
    let all_fields: Vec<&FieldDef> = cfg.display.left.iter()
        .chain(cfg.display.right.iter())
        .filter(|f| f.enabled)
        .collect();
    // ── Title ──
    let title_color = Color::from_hex_opt(&cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let title_text = cfg.title.format.replace("{user}", &info.user).replace("{host}", &info.host);
    out.push_str(&format!("\n{}  {}{}{}{}\n", title_color.fg_escape(), BOLD, title_text, RESET, RESET));

    // ── Separator ──
    let sep_color = Color::from_hex_opt(&cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let sep_len = cfg.separator.length.min(term_width.saturating_sub(4));
    let sep_str: String = cfg.separator.char.repeat(sep_len);
    out.push_str(&format!("{}  {}{}{}\n", sep_color.fg_escape(), sep_str, RESET, RESET));

    // ── ASCII header block ──
    if !logo_lines.is_empty() {
        let logo_width = logo_lines.iter().map(|l| l.trim_end().width()).max().unwrap_or(0);
        let block_center = term_width.saturating_sub(logo_width) / 2;
        for (i, line) in logo_lines.iter().enumerate() {
            let logo_color = if !cfg.logo.colors.is_empty() {
                cfg.logo.colors[i % cfg.logo.colors.len()]
            } else {
                Color::new(255, 255, 255)
            };
            let trimmed = line.trim_end();
            let mut row = String::new();
            row.push_str(&" ".repeat(block_center));
            // Right-pad shorter lines to match block width
            let padded = format!("{:w$}", trimmed, w = logo_width);
            for ch in padded.chars() {
                if ch != ' ' {
                    row.push_str(&format!("{}{}", logo_color.fg_escape(), ch));
                } else {
                    row.push(' ');
                }
            }
            row.push_str(RESET);
            row.push('\n');
            out.push_str(&row);
        }
        out.push('\n');
    }

    // ── Info panels (single column, full width) ──
    for (i, fd) in all_fields.iter().enumerate() {
        let fg_color = if !cfg.logo.colors.is_empty() {
            cfg.logo.colors[i % cfg.logo.colors.len()]
        } else {
            Color::new(255, 255, 255)
        };
        let avail = term_width.saturating_sub(cfg.panel.left_pad + cfg.panel.right_pad + 2).max(4);
        let (panel_text, _) = build_panel(fd, info, &cfg.panel, fg_color, avail);
        let mut row = String::new();
        row.push_str(&" ".repeat(cfg.panel.left_pad));
        row.push_str(&panel_text);
        let rv = visible_width(&row);
        if rv < term_width {
            row.push_str(&" ".repeat(term_width.saturating_sub(rv)));
        }
        row.push_str(RESET);
        row.push('\n');
        out.push_str(&row);
    }

    out.push('\n');
    Ok(out)
}

/// Render mobile preview as styled lines for TUI.
pub fn render_mobile_preview(cfg: &Config, info: &SysInfo, ascii_art: &str, term_width: u16, is_narrow: bool) -> Vec<StyledLine> {
    let tw = term_width as usize;
    let mut lines = Vec::new();

    let compacted = compact_for_mobile(ascii_art);
    let logo_lines: Vec<&str> = if ascii_art.is_empty() || is_narrow {
        Vec::new()
    } else {
        compacted.iter().map(|s| s.as_str()).collect()
    };

    let all_fields: Vec<&FieldDef> = cfg.display.left.iter()
        .chain(cfg.display.right.iter())
        .filter(|f| f.enabled)
        .collect();

    // ── Title ──
    let title_color = Color::from_hex_opt(&cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let title_text = cfg.title.format.replace("{user}", &info.user).replace("{host}", &info.host);
    lines.push(StyledLine {
        segments: vec![
            StyledSegment { text: "  ".into(), fg: None, bg: None, bold: false },
            StyledSegment { text: title_text, fg: Some(title_color), bg: None, bold: true },
        ],
    });

    // ── Separator ──
    let sep_color = Color::from_hex_opt(&cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let sep_len = cfg.separator.length.min(tw.saturating_sub(4));
    let sep_str: String = cfg.separator.char.repeat(sep_len);
    lines.push(StyledLine {
        segments: vec![
            StyledSegment { text: "  ".into(), fg: None, bg: None, bold: false },
            StyledSegment { text: sep_str, fg: Some(sep_color), bg: None, bold: false },
        ],
    });

    // ── ASCII header block ──
    if !logo_lines.is_empty() {
        let logo_width = logo_lines.iter().map(|l| l.trim_end().width()).max().unwrap_or(0);
        let block_center = tw.saturating_sub(logo_width) / 2;
        for (i, line) in logo_lines.iter().enumerate() {
            let logo_color = if !cfg.logo.colors.is_empty() {
                cfg.logo.colors[i % cfg.logo.colors.len()]
            } else {
                Color::new(255, 255, 255)
            };
            let trimmed = line.trim_end();
            let mut segs = vec![
                StyledSegment { text: " ".repeat(block_center), fg: None, bg: None, bold: false },
            ];
            let padded = format!("{:w$}", trimmed, w = logo_width);
            for ch in padded.chars() {
                if ch != ' ' {
                    segs.push(StyledSegment { text: ch.to_string(), fg: Some(logo_color), bg: None, bold: false });
                } else {
                    segs.push(StyledSegment { text: " ".into(), fg: None, bg: None, bold: false });
                }
            }
            let cur_vis = visible_of_segs(&segs);
            if cur_vis < tw {
                segs.push(StyledSegment { text: " ".repeat(tw.saturating_sub(cur_vis)), fg: None, bg: None, bold: false });
            }
            lines.push(StyledLine { segments: segs });
        }
        // blank line after ASCII
        lines.push(StyledLine { segments: vec![StyledSegment { text: " ".into(), fg: None, bg: None, bold: false }] });
    }

    // ── Info panels (single column, full width) ──
    for (i, fd) in all_fields.iter().enumerate() {
        let fg_color = if !cfg.logo.colors.is_empty() {
            cfg.logo.colors[i % cfg.logo.colors.len()]
        } else {
            Color::new(255, 255, 255)
        };
        let avail = tw.saturating_sub(cfg.panel.left_pad + cfg.panel.right_pad + 2).max(4);
        let (parts, _) = build_panel_styled(fd, info, &cfg.panel, fg_color, avail);
        let mut segs = vec![
            StyledSegment { text: " ".repeat(cfg.panel.left_pad), fg: None, bg: None, bold: false },
        ];
        segs.extend(parts);
        let cur_vis = visible_of_segs(&segs);
        if cur_vis < tw {
            segs.push(StyledSegment { text: " ".repeat(tw.saturating_sub(cur_vis)), fg: None, bg: None, bold: false });
        }
        lines.push(StyledLine { segments: segs });
    }

    lines
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Format a panel field with truncation matching the original Python build().
fn build_panel(field: &FieldDef, info: &SysInfo, panel: &crate::config::PanelConfig, fg_color: Color, avail_w: usize) -> (String, usize) {
    let ctx = RenderCtx { info, panel_cfg: panel, max_width: avail_w, fg_color };
    let output = FieldWidget::from_def(field.clone()).render(&ctx);
    (output.ansi, output.width)
}

/// Format a panel field as StyledSegments (for the TUI preview).
fn build_panel_styled(field: &FieldDef, info: &SysInfo, panel: &crate::config::PanelConfig, fg_color: Color, avail_w: usize) -> (Vec<StyledSegment>, usize) {
    let ctx = RenderCtx { info, panel_cfg: panel, max_width: avail_w, fg_color };
    let output = FieldWidget::from_def(field.clone()).render(&ctx);
    (output.styled, output.width)
}

/// Compute the total visible width of a sequence of StyledSegments.
fn visible_of_segs(segs: &[StyledSegment]) -> usize {
    segs.iter().map(|s| UnicodeWidthStr::width(s.text.as_str())).sum()
}

/// Cascade offset for panel staggering.
fn cascade_offset(i: usize, total: usize, max_shift: usize) -> usize {
    if total <= 1 {
        return 0;
    }
    let mid = (total - 1) as f64 / 2.0;
    if mid <= 0.0 {
        return 0;
    }
    let rel = (i as f64 / mid - 1.0).abs();
    (rel * max_shift as f64).round() as usize
}

fn terminal_width() -> usize {
    match crossterm::terminal::size() {
        Ok((w, _)) => w as usize,
        Err(_) => 80,
    }
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        out.push(c);
    }
    out
}

fn visible_width(s: &str) -> usize {
    strip_ansi(s).width()
}

/// Trim an ANSI string so its visible width doesn't exceed `max_width`.
fn trim_to_width(s: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut vis = 0;
    let mut in_escape = false;
    let mut escape_buf = String::new();
    for c in s.chars() {
        if in_escape {
            escape_buf.push(c);
            if c == 'm' {
                in_escape = false;
                // keep the escape sequence even if we're past the limit
                out.push_str(&escape_buf);
                escape_buf.clear();
            }
            continue;
        }
        if c == '\x1b' {
            in_escape = true;
            escape_buf.clear();
            escape_buf.push(c);
            continue;
        }
        let w = c.width().unwrap_or(0);
        if vis + w > max_width {
            break;
        }
        vis += w;
        out.push(c);
    }
    // Close any open escape sequence
    if in_escape {
        out.push_str(&escape_buf);
    }
    out
}

#[allow(dead_code)]
pub fn styled_lines_to_ansi(lines: &[StyledLine]) -> String {
    let mut out = String::new();
    for line in lines {
        for seg in &line.segments {
            if seg.bold {
                out.push_str(BOLD);
            }
            if let Some(fg) = &seg.fg {
                out.push_str(&fg.fg_escape());
            }
            if let Some(bg) = &seg.bg {
                out.push_str(&bg.bg_escape());
            }
            out.push_str(&seg.text);
            out.push_str(RESET);
        }
        out.push('\n');
    }
    out
}
