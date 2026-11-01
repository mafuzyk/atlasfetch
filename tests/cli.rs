//! Runs the built binary the way a script does: stdin closed and stdout captured.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A home directory with no configuration, unique to one test.
fn scratch_home(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("halofetch-cli-{name}-{}", std::process::id()))
}

fn halofetch(name: &str, args: &[&str], force_color: bool) -> Output {
    let home = scratch_home(name);
    let mut command = Command::new(env!("CARGO_BIN_EXE_halofetch"));
    command
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("APPDATA", &home)
        .stdin(Stdio::null());
    if force_color {
        command.env("FORCE_COLOR", "1");
    }
    command.output().unwrap()
}

fn has_escapes(bytes: &[u8]) -> bool {
    bytes.contains(&0x1b)
}

#[test]
fn redirected_fetch_has_no_escape_codes() {
    let output = halofetch("fetch", &["fetch"], false);
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(!has_escapes(&output.stdout));
}

#[test]
fn redirected_preset_list_has_no_escape_codes() {
    let output = halofetch("presets", &["preset", "list"], false);
    assert!(output.status.success());
    assert!(!has_escapes(&output.stdout));
}

// The Windows runner has no console, so ANSI support is unknown there.
#[cfg(unix)]
#[test]
fn force_color_keeps_escape_codes_in_a_pipe() {
    let output = halofetch("force", &["fetch"], true);
    assert!(output.status.success());
    assert!(has_escapes(&output.stdout));
}
