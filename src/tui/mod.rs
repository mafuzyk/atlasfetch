// TUI setup configurator — launched via `atlasfetch setup`.
//
// Built with ratatui + crossterm. Dispatches to PC or mobile TUI
// depending on platform detection.

mod editor;
mod mobile;

use color_eyre::Result;

pub fn run(cfg: &mut crate::config::Config) -> Result<()> {
    if crate::info::is_android() {
        mobile::run(cfg)
    } else {
        editor::run(cfg)
    }
}
