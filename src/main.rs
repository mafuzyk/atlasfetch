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
mod mobile;
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
    // Try CWD first
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("Cargo.toml").exists() && cwd.join(".git").exists() {
            return cwd.to_string_lossy().to_string();
        }
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
    // Try common locations
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    for candidate in &[
        format!("{}/Projetos/atlasfetch", home),
        format!("{}/src/atlasfetch", home),
        format!("{}/atlasfetch", home),
        format!("{}/code/atlasfetch", home),
        format!("{}/dev/atlasfetch", home),
    ] {
        if std::path::Path::new(candidate).join("Cargo.toml").exists() && std::path::Path::new(candidate).join(".git").exists() {
            return candidate.clone();
        }
    }
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

    // Create parent directory if needed
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    println!("→ Installing to {}...", dest);
    // Use install(1) which handles running binaries via temp+rename
    // Fallback to cp -f + atomic rename
    let status = Command::new("install")
        .args(["-m", "755", &binary, &dest])
        .status()
        .or_else(|_| {
            // cp fallback: copy to temp then rename (atomic)
            let tmp = format!("{}.new", dest);
            Command::new("cp").args([&binary, &tmp]).status().and_then(|s| {
                if s.success() {
                    Command::new("mv").args([&tmp, &dest]).status()
                } else {
                    Ok(s)
                }
            })
        })?;

    if !status.success() {
        color_eyre::eyre::bail!(
            "Failed to install binary to {}. Make sure the directory exists and is writable.",
            dest
        );
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

    // --reset: delete config and launch setup wizard
    if args.reset {
        let path = config::config_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
            println!("Config removed.");
        }
        let mut cfg = config::Config::load()?;
        tui::run(&mut cfg)?;
        return Ok(());
    }

    // --mode: mobile rendering mode
    if let Some(ref mode_str) = args.mode {
        if let Some(mode) = mobile::MobileMode::from_str(mode_str) {
            let cfg = config::Config::load()?;
            let info = info::collect()?;
            let ascii_art = ascii::load(&cfg)?;
            print!("{}", mobile::render(&mode, &cfg, &info, &ascii_art));
            return Ok(());
        } else {
            eprintln!("Unknown mode '{}'. Available modes: {:?}", mode_str, mobile::MobileMode::variants());
            return Ok(());
        }
    }

    // --just-ascii: print only the ASCII art
    if args.just_ascii {
        ascii::ensure_logos()?;
        let cfg = config::Config::load()?;
        let ascii_art = ascii::load(&cfg)?;
        print!("{}", render::render_ascii_only(&cfg, &ascii_art));
        return Ok(());
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
