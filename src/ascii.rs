// ASCII logo management.
//
// Logos are stored as plain text files in the logo_dir. The filename is the
// key used in config (e.g., "arch", "nixos", "ubuntu"). The logos/ directory
// lives next to the binary or under ~/.config/atlasfetch/logos/.
//
// On first run, logos are copied from the binary's adjacent logos/ directory
// into the user's config directory so that updates don't break existing configs.

use color_eyre::Result;
use std::fs;
use std::path::PathBuf;

use crate::config;

include!(concat!(env!("OUT_DIR"), "/logos_generated.rs"));

fn clean_ascii(art: &str) -> String {
    // Strip trailing whitespace per line and dedent common leading whitespace
    let lines: Vec<String> = art.lines().map(|l| l.trim_end().to_string()).collect();
    let min_lead = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\u{2800}').count())
        .min()
        .unwrap_or(0);
    if min_lead == 0 {
        lines.join("\n")
    } else {
        lines.iter().map(|l| l.chars().skip(min_lead).collect::<String>()).collect::<Vec<_>>().join("\n")
    }
}

fn filter_logo_keys(keys: &mut Vec<String>) {
    keys.sort();
    keys.retain(|k| !k.starts_with('.') && !k.ends_with("_small"));
}

/// All available built-in logo keys.
pub fn available_logos() -> Result<Vec<String>> {
    let dir = config::logo_dir()?;
    if !dir.exists() {
        let mut keys: Vec<String> = embedded_keys().iter().map(|s| s.to_string()).collect();
        filter_logo_keys(&mut keys);
        return Ok(keys);
    }
    let mut keys: Vec<String> = fs::read_dir(&dir)?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    if keys.is_empty() {
        let mut keys: Vec<String> = embedded_keys().iter().map(|s| s.to_string()).collect();
        filter_logo_keys(&mut keys);
        return Ok(keys);
    }
    filter_logo_keys(&mut keys);
    Ok(keys)
}

/// Load the ASCII art for the current config.
/// Auto-picks the `_small` variant when the terminal is narrower than 65 columns.
pub fn load(cfg: &config::Config) -> Result<String> {
    let key = if !cfg.logo.key.is_empty() {
        let term_w = crate::layout::terminal_width();
        let small_key = format!("{}_small", cfg.logo.key);
        if term_w < 65 {
            let dir = config::logo_dir()?;
            let small_on_fs = dir.join(&small_key).exists();
            let small_embedded = get_embedded(&small_key).is_some();
            if small_on_fs || small_embedded {
                small_key
            } else {
                cfg.logo.key.clone()
            }
        } else {
            cfg.logo.key.clone()
        }
    } else {
        String::new()
    };

    if !key.is_empty() {
        let dir = config::logo_dir()?;
        let path = dir.join(&key);
        if let Ok(art) = fs::read_to_string(&path) {
            return Ok(clean_ascii(&art));
        }
        if let Some(art) = get_embedded(&key) {
            return Ok(clean_ascii(art));
        }
    }

    // Fall back to logo path
    let logo_path = shellexpand(&cfg.logo.path)?;
    if let Ok(art) = fs::read_to_string(&logo_path) {
        let trimmed = art.trim_end().to_string();
        if !trimmed.is_empty() {
            return Ok(clean_ascii(&trimmed));
        }
    }

    // Ultimate fallback: a minimal Arch-like logo
    Ok(default_ascii())
}

/// Copy logos from the binary's adjacent directory to the user config dir.
pub fn ensure_logos() -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe_dir = exe.parent().unwrap_or(std::path::Path::new("/"));
    let src = exe_dir.join("logos");
    let dst = config::config_dir()?.join("logos");

    if dst.exists() {
        return Ok(());
    }
    if !src.exists() {
        return Ok(());
    }

    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(&src)? {
        let entry = entry?;
        let ftype = entry.file_type()?;
        if ftype.is_file() {
            let name = entry.file_name();
            fs::copy(entry.path(), dst.join(&name))?;
        }
    }
    Ok(())
}

fn shellexpand(s: &str) -> Result<PathBuf> {
    if let Some(rest) = s.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Ok(PathBuf::from(home).join(rest.trim_start_matches('/')))
    } else {
        Ok(PathBuf::from(s))
    }
}

fn default_ascii() -> String {
    r#"                    -`
                   .o+`
                  `ooo/
                 `+oooo:
                `+oooooo:
                -+oooooo+:
              `/:-:++oooo+:
             `/+++++/++++++:
            `/++++++++++++++:
           `/+++ooooooooooooo/`
          ./ooosssso++osssssso+`
        .oossssso-````/ossssss+`
       -osssssso.      :ssssssso.
      :osssssss/        osssso+++.
     /ossssssss/        +ssssooo/-
   `/ossssso+/:-        -:/+osssso+-
  `+sso+:-`                 `.-/+oso:
 `++:.                           `-/+/
 .`                                 `/`"#.into()
}
