// atlasfetch — centered ASCII art with powerline panels
//
// Design: The binary has two modes. The default mode prints system info
// instantly. The `setup` subcommand launches a TUI configurator. Both share
// the same rendering engine so the preview in setup is identical to real
// terminal output.

mod ascii;
mod cli;
mod config;
mod info;
mod layout;
mod render;
mod theme;
mod tui;

use clap::Parser;
use color_eyre::Result;
use std::process::Command;

fn detect_src_dir() -> String {
    if let Ok(dir) = std::env::var("ATLASFETCH_SRC") {
        return dir;
    }
    // Try to find source from the binary's own path
    if let Ok(exe) = std::env::current_exe() {
        let mut path = exe.parent().unwrap_or(std::path::Path::new("/"));
        for _ in 0..5 {
            if path.join("Cargo.toml").exists() && path.join(".git").exists() {
                return path.to_string_lossy().to_string();
            }
            if let Some(parent) = path.parent() {
                path = parent;
            } else {
                break;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{}/atlasfetch", home)
}

fn update_atlasfetch() -> Result<()> {
    let src = detect_src_dir();
    println!("📦 atlasfetch update — source: {}", src);

    // git pull
    println!("→ Pulling latest source...");
    let status = Command::new("git")
        .args(["-C", &src, "pull", "--rebase", "--autostash"])
        .status()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to run git: {}. Is git installed?", e))?;

    if !status.success() {
        color_eyre::eyre::bail!(
            "git pull failed. Make sure '{}' is a git clone of https://github.com/mafuzyk/atlasfetch",
            src
        );
    }

    // cargo build
    println!("→ Building release binary...");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&src)
        .status()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to run cargo: {}. Is Rust installed?", e))?;

    if !status.success() {
        color_eyre::eyre::bail!("cargo build failed.");
    }

    // determine install path
    let binary = format!("{}/target/release/atlasfetch", src);
    let dest = if info::is_android() {
        let prefix = std::env::var("PREFIX").unwrap_or_else(|_| "/data/data/com.termux/files/usr".into());
        format!("{}/bin/atlasfetch", prefix)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{}/.local/bin/atlasfetch", home)
    };

    println!("→ Installing to {}...", dest);
    let status = Command::new("cp")
        .args([&binary, &dest])
        .status()?;

    if !status.success() {
        color_eyre::eyre::bail!("Failed to copy binary to {}. Try with 'sudo' or 'doas'.", dest);
    }

    println!("✅ Updated to latest version!");
    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let args = cli::Args::parse();

    // --list-presets: print and exit
    if args.list_presets {
        let themes = theme::all_themes();
        println!("Available presets:");
        for t in &themes {
            let swatch: String = t
                .colors
                .iter()
                .map(|c| format!("\x1b[48;2;{};{};{}m  \x1b[0m", c.r, c.g, c.b))
                .collect();
            println!("  {:20} {}", t.name, swatch);
        }
        return Ok(());
    }

    // --preset: apply and exit
    if let Some(ref name) = args.preset {
        let themes = theme::all_themes();
        if let Some(t) = themes.iter().find(|t| t.name == *name) {
            let mut cfg = config::Config::load()?;
            cfg.logo.colors = t.colors.clone();
            cfg.save()?;
            println!("Preset \"{}\" applied.", name);
        } else {
            eprintln!("Preset \"{}\" not found. Use --list-presets.", name);
        }
        return Ok(());
    }

    // --update: pull, build, install
    if args.update {
        return update_atlasfetch();
    }

    // setup: launch TUI configurator
    if args.setup {
        let mut cfg = config::Config::load()?;
        tui::run(&mut cfg)?;
        return Ok(());
    }

    // first run: launch setup TUI
    if !config::config_path()?.exists() {
        let mut cfg = config::Config::load()?;
        tui::run(&mut cfg)?;
        return Ok(());
    }

    // default: print fetch output
    ascii::ensure_logos()?;
    let cfg = config::Config::load()?;
    let info = info::collect()?;
    let ascii_art = ascii::load(&cfg)?;

    let term_width = layout::terminal_width();
    let is_mobile = info::is_android() || term_width < 80;

    let output = if is_mobile && term_width < 55 {
        render::render_mobile(&cfg, &info, &ascii_art, true)?
    } else if is_mobile {
        render::render_mobile(&cfg, &info, &ascii_art, false)?
    } else {
        render::render(&cfg, &info, &ascii_art)?
    };

    print!("{}", output);
    Ok(())
}
