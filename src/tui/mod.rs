// TUI setup configurator — launched via `atlasfetch setup`.
//
// Built with ratatui + crossterm. Dispatches to PC or mobile TUI
// depending on platform detection.

mod app;
mod editor;
mod mobile;

use color_eyre::Result;

pub fn run(cfg: &mut crate::config::Config) -> Result<()> {
    if crate::info::is_android() {
        mobile::run(cfg)
    } else {
        app::run(cfg)
    }
}

/// Launch the new interactive editor with live preview.
pub fn run_editor(cfg: &mut crate::config::Config) -> Result<()> {
    editor::run(cfg)
}
