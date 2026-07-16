use crate::config::Config;
use crate::info::SysInfo;
use crate::theme::Color;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MobileMode {
    Card,
    Bios,
    Companion,
    Ascii,
}

impl MobileMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "card" | "c" => Some(Self::Card),
            "bios" | "b" => Some(Self::Bios),
            "companion" | "comp" => Some(Self::Companion),
            "ascii" | "a" => Some(Self::Ascii),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn variants() -> &'static [&'static str] {
        &["card", "bios", "companion", "ascii"]
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Card => "Card System",
            Self::Bios => "BIOS / Cyber",
            Self::Companion => "Device Companion",
            Self::Ascii => "Responsive ASCII",
        }
    }
}

pub fn render(mode: &MobileMode, cfg: &Config, info: &SysInfo, _ascii_art: &str) -> String {
    match mode {
        MobileMode::Card => render_card(cfg, info),
        MobileMode::Bios => render_bios(cfg, info),
        MobileMode::Companion => render_companion(cfg, info),
        MobileMode::Ascii => render_responsive_ascii(cfg, info),
    }
}

fn terminal_width() -> usize {
    crate::layout::terminal_width()
}

fn accent_color(cfg: &Config) -> Color {
    if !cfg.logo.colors.is_empty() {
        cfg.logo.colors[0]
    } else {
        Color::new(255, 102, 146)
    }
}

// ── Card System ──────────────────────────────────────────────────────────

fn render_card(cfg: &Config, info: &SysInfo) -> String {
    let tw = terminal_width();
    let mut out = String::new();
    let ac = accent_color(cfg);

    let device_name = if !info.device.is_empty() { &info.device } else { &info.host };

    // Header card
    let header = format!("    ◢ ATLAS ◣   ");
    let sub = if !info.rom.is_empty() {
        format!("{}  |  {}", device_name, info.rom)
    } else {
        device_name.to_string()
    };
    let w = tw.min(50).max(20);

    out.push_str(&format!("{}╭{}╮{}\n", ac.fg_escape(), "─".repeat(w.saturating_sub(2)), RESET));
    out.push_str(&format!("{}│{}│{}\n", ac.fg_escape(),
        center_text(&header, w), RESET));
    out.push_str(&format!("{}│{}│{}\n", Color::new(200, 200, 200).fg_escape(),
        center_text(&sub, w), RESET));
    out.push_str(&format!("{}╰{}╯{}\n", ac.fg_escape(), "─".repeat(w.saturating_sub(2)), RESET));
    out.push('\n');

    // Data cards
    let cards = build_card_data(info);
    let card_inner = w.saturating_sub(4);
    for (title, items) in cards {
        let title_str = format!(" {} ", title);
        out.push_str(&format!("{}╭─{}─╮{}\n", Color::new(157, 133, 255).fg_escape(),
            title_str, RESET));
        for (label, value) in items {
            let line = format!(" {}: {} ", label, value);
            let line_trimmed = truncate(&line, card_inner);
            out.push_str(&format!("{}│ {}{}│{}\n",
                Color::new(157, 133, 255).fg_escape(),
                Color::new(180, 180, 200).fg_escape(),
                pad_right(&line_trimmed, card_inner),
                RESET));
        }
        out.push_str(&format!("{}╰{}╯{}\n", Color::new(157, 133, 255).fg_escape(),
            "─".repeat(w.saturating_sub(2)), RESET));
        out.push('\n');
    }

    out.push('\n');
    out
}

// ── BIOS / Cyber ─────────────────────────────────────────────────────────

fn render_bios(cfg: &Config, info: &SysInfo) -> String {
    let tw = terminal_width();
    let mut out = String::new();
    let ac = accent_color(cfg);

    let device_name = if !info.device.is_empty() { &info.device } else { &info.host };

    out.push_str(&format!("\n{}  {}ATLASFETCH MOBILE{}\n", ac.fg_escape(), BOLD, RESET));
    out.push_str(&format!("{}  ──────────────────────{}\n\n", Color::new(100, 100, 120).fg_escape(), RESET));

    let sections = build_bios_data(info, device_name, &info.rom);

    for (title, items) in sections {
        out.push_str(&format!("{}  ═══ {} ═══{}\n", ac.fg_escape(), title.to_uppercase(), RESET));
        for (label, value) in items {
            let val_color = if value.contains("Unlocked") || value.contains("active") {
                Color::new(80, 200, 120)
            } else if value.contains("Locked") || value.contains("unknown") || value.is_empty() {
                Color::new(200, 100, 100)
            } else {
                Color::new(200, 200, 220)
            };
            let avail = tw.saturating_sub(12);
            let display_val = truncate(&value, avail);
            out.push_str(&format!("    > {:<12} {}{}{}\n",
                format!("{}:", label),
                val_color.fg_escape(),
                display_val,
                RESET));
        }
        out.push('\n');
    }

    // Footer
    out.push_str(&format!("{}  ──────────────────────{}\n", Color::new(100, 100, 120).fg_escape(), RESET));
    out.push_str(&format!("{}  AtlasFetch Mobile v2.0 — System Diagnostic{}\n", Color::new(80, 80, 100).fg_escape(), RESET));
    out.push('\n');
    out
}

// ── Device Companion ─────────────────────────────────────────────────────

fn render_companion(cfg: &Config, info: &SysInfo) -> String {
    let tw = terminal_width();
    let mut out = String::new();
    let ac = accent_color(cfg);

    let device_name = if !info.device.is_empty() { &info.device } else { &info.host };

    // Header
    out.push_str(&format!("\n{}{}  {}{}\n\n", BOLD, ac.fg_escape(), center_text("ATLAS", tw), RESET));
    out.push_str(&format!("{}\n\n", center_text("📱", tw)));
    out.push_str(&format!("{}{}{}\n\n", BOLD, center_text(device_name, tw), RESET));

    let bar_w = (tw.saturating_sub(10)).min(30).max(10);

    // Battery bar
    if !info.battery_level.is_empty() {
        let level = info.battery_level.trim_end_matches('%').parse::<f64>().unwrap_or(0.0) as usize;
        let filled = (level * bar_w / 100).min(bar_w);
        let empty = bar_w.saturating_sub(filled);
        let temp = if !info.battery_temp.is_empty() { format!("  {}", info.battery_temp) } else { String::new() };
        let status = if !info.battery_status.is_empty() { format!("  {}", info.battery_status) } else { String::new() };
        out.push_str(&format!("{}  Battery{}\n", ac.fg_escape(), RESET));
        out.push_str(&format!("  {}  {}%{}{}\n",
            progress_bar(filled, empty, ac),
            level, temp, status));
        out.push('\n');
    }

    // Memory bar
    if !info.memory.is_empty() {
        let used_pct = parse_memory_pct(&info.memory);
        let filled = (used_pct * bar_w / 100).min(bar_w);
        let empty = bar_w.saturating_sub(filled);
        let mem_color = if used_pct > 80 { Color::new(255, 100, 100) }
                        else if used_pct > 60 { Color::new(255, 200, 80) }
                        else { Color::new(100, 200, 255) };
        out.push_str(&format!("{}  Memory{}\n", Color::new(157, 133, 255).fg_escape(),
            RESET));
        out.push_str(&format!("  {}  {}\n",
            progress_bar(filled, empty, mem_color),
            info.memory));
        out.push('\n');
    }

    // Storage bar
    if !info.storage.is_empty() || !info.disk.is_empty() {
        let stor = if !info.storage.is_empty() { &info.storage } else { &info.disk };
        let used_pct = parse_disk_pct(stor);
        let filled = (used_pct * bar_w / 100).min(bar_w);
        let empty = bar_w.saturating_sub(filled);
        out.push_str(&format!("{}  Storage{}\n", Color::new(255, 184, 131).fg_escape(),
            RESET));
        out.push_str(&format!("  {}  {}\n",
            progress_bar(filled, empty, Color::new(255, 184, 131)),
            stor));
        out.push('\n');
    }

    // Quick info line
    let cpu_t = if !info.cpu_temp.is_empty() { format!("{}°C", info.cpu_temp) } else { String::new() };
    let quick = format!("Uptime: {}", info.uptime);
    let quick = if !cpu_t.is_empty() { format!("{}    CPU: {}", quick, cpu_t) } else { quick };
    let quick = if info.packages != "—" && !info.packages.is_empty() {
        format!("{}    Pkgs: {}", quick, info.packages)
    } else { quick };
    if quick.len() > 2 {
        out.push_str(&format!("  {}\n", quick));
    }

    out.push('\n');
    out
}

// ── Responsive ASCII ─────────────────────────────────────────────────────

fn render_responsive_ascii(cfg: &Config, info: &SysInfo) -> String {
    let tw = terminal_width();
    let mut out = String::new();
    let ac = accent_color(cfg);

    let device_name = if !info.device.is_empty() { &info.device } else { &info.host };

    // Choose ASCII art based on terminal width
    let ascii_art: Vec<&str> = if tw >= 40 {
        vec![
            "  ████  ",
            " █ AT █ ",
            " █  █ █ ",
            "  ████  ",
        ]
    } else {
        vec![
            " ██ ",
            "█AT█",
            " ██ ",
        ]
    };

    let lh = ascii_art.len();
    let logo_width = ascii_art.iter().map(|l| l.width()).max().unwrap_or(0);

    // Decide layout based on width
    if tw >= 55 {
        // ASCII left, info right
        let info_lines = build_ascii_info(info, device_name);
        let n = lh.max(info_lines.len());

        for i in 0..n {
            let mut row = String::new();
            // ASCII
            if i < lh {
                for ch in ascii_art[i].chars() {
                    if ch != ' ' {
                        row.push_str(&format!("{}{}", ac.fg_escape(), ch));
                    } else {
                        row.push(' ');
                    }
                }
            } else {
                row.push_str(&" ".repeat(logo_width));
            }
            row.push_str(&"  ");

            // Info
            if i < info_lines.len() {
                row.push_str(&format!("{}{}", Color::new(200, 200, 220).fg_escape(), info_lines[i]));
            }

            let rv = strip_ansi(&row).width();
            if rv < tw {
                row.push_str(&" ".repeat(tw.saturating_sub(rv)));
            }
            row.push_str(RESET);
            row.push('\n');
            out.push_str(&row);
        }
    } else {
        // ASCII centered, info below
        for line in &ascii_art {
            let center = (tw.saturating_sub(line.width())) / 2;
            let mut row = String::new();
            row.push_str(&" ".repeat(center));
            for ch in line.chars() {
                if ch != ' ' {
                    row.push_str(&format!("{}{}", ac.fg_escape(), ch));
                } else {
                    row.push(' ');
                }
            }
            row.push_str(RESET);
            row.push('\n');
            out.push_str(&row);
        }
        out.push('\n');

        // Info below
        let info_lines = build_ascii_info(info, device_name);
        for line in info_lines {
            out.push_str(&format!("{}{}{}\n",
                Color::new(200, 200, 220).fg_escape(),
                center_text(&line, tw),
                RESET));
        }
    }

    out.push('\n');
    out
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn center_text(s: &str, w: usize) -> String {
    let vis = s.width();
    if vis >= w { return s.to_string(); }
    let left = (w.saturating_sub(vis)) / 2;
    format!("{}{}", " ".repeat(left), s)
}

fn pad_right(s: &str, w: usize) -> String {
    let vis = s.width();
    if vis >= w { s.to_string() }
    else { format!("{}{}", s, " ".repeat(w.saturating_sub(vis))) }
}

fn truncate(s: &str, max: usize) -> String {
    let vis = s.width();
    if vis <= max { return s.to_string(); }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > max.saturating_sub(1) {
            out.push('…');
            break;
        }
        w += cw;
        out.push(ch);
    }
    out
}

fn progress_bar(filled: usize, empty: usize, color: Color) -> String {
    let block = "\u{2588}";
    let light = "\u{2591}";
    format!("{}{}{}{}{}",
        color.fg_escape(),
        block.repeat(filled),
        Color::new(80, 80, 100).fg_escape(),
        light.repeat(empty),
        RESET)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' { in_escape = false; }
            continue;
        }
        if c == '\x1b' { in_escape = true; continue; }
        out.push(c);
    }
    out
}

fn parse_memory_pct(mem: &str) -> usize {
    let parts: Vec<&str> = mem.split('/').collect();
    if parts.len() != 2 { return 50; }
    let used = parts[0].parse::<f64>().unwrap_or(1.0);
    let total = parts[1].trim_end_matches('G').parse::<f64>().unwrap_or(1.0);
    if total <= 0.0 { return 50; }
    ((used / total) * 100.0).round() as usize
}

fn parse_disk_pct(disk: &str) -> usize {
    let clean = disk.trim_end_matches('G');
    let parts: Vec<&str> = clean.split('/').collect();
    if parts.len() != 2 { return 50; }
    let used = parts[0].parse::<f64>().unwrap_or(1.0);
    let total = parts[1].parse::<f64>().unwrap_or(1.0);
    if total <= 0.0 { return 50; }
    ((used / total) * 100.0).round() as usize
}

// ── Data builders ────────────────────────────────────────────────────────

type Section = Vec<(String, String)>;

fn build_card_data(info: &SysInfo) -> Vec<(&'static str, Section)> {
    let mut cards = Vec::new();

    let mut system = Vec::new();
    system.push(("Android".into(), if !info.os.is_empty() { info.os.clone() } else { "—".into() }));
    system.push(("Kernel".into(), if !info.kernel.is_empty() { info.kernel.clone() } else { "—".into() }));
    if !info.root_status.is_empty() {
        system.push(("Root".into(), info.root_status.clone()));
    }
    if !info.selinux.is_empty() {
        system.push(("SELinux".into(), info.selinux.clone()));
    }
    if !info.security_patch.is_empty() {
        system.push(("Sec. Patch".into(), info.security_patch.clone()));
    }
    cards.push(("System", system));

    let mut hardware = Vec::new();
    if !info.soc.is_empty() && info.soc != "unknown" {
        hardware.push(("SoC".into(), info.soc.clone()));
    }
    if !info.cpu.is_empty() && info.cpu != "unknown" {
        hardware.push(("CPU".into(), info.cpu.clone()));
    }
    if !info.gpu.is_empty() && info.gpu != "unknown" {
        hardware.push(("GPU".into(), info.gpu.clone()));
    }
    if !info.arch.is_empty() {
        hardware.push(("Arch".into(), info.arch.clone()));
    }
    hardware.push(("RAM".into(), if !info.memory.is_empty() { info.memory.clone() } else { "—".into() }));
    hardware.push(("Storage".into(), if !info.storage.is_empty() { info.storage.clone() } else if !info.disk.is_empty() { info.disk.clone() } else { "—".into() }));
    if !info.resolution.is_empty() {
        hardware.push(("Display".into(), info.resolution.clone()));
    }
    if !info.vram.is_empty() {
        hardware.push(("VRAM".into(), info.vram.clone()));
    }
    cards.push(("Hardware", hardware));

    let mut battery = Vec::new();
    battery.push(("Level".into(), if !info.battery_level.is_empty() { info.battery_level.clone() } else { "—".into() }));
    battery.push(("Temp".into(), if !info.battery_temp.is_empty() { info.battery_temp.clone() } else { "—".into() }));
    if !info.battery_health.is_empty() {
        battery.push(("Health".into(), info.battery_health.clone()));
    }
    battery.push(("Status".into(), if !info.battery_status.is_empty() { info.battery_status.clone() } else { "—".into() }));
    battery.push(("Uptime".into(), info.uptime.clone()));
    cards.push(("Battery", battery));

    let mut network = Vec::new();
    if !info.wifi_ssid.is_empty() {
        network.push(("WiFi".into(), info.wifi_ssid.clone()));
    }
    if !info.signal.is_empty() {
        network.push(("Signal".into(), info.signal.clone()));
    }
    if !info.local_ip.is_empty() {
        network.push(("IP".into(), info.local_ip.clone()));
    }
    if !info.packages.is_empty() && info.packages != "—" {
        network.push(("Packages".into(), info.packages.clone()));
    }
    if !network.is_empty() {
        cards.push(("Network", network));
    }

    cards
}

fn build_bios_data<'a>(info: &'a SysInfo, device_name: &str, rom: &str) -> Vec<(&'static str, Vec<(String, String)>)> {
    let mut sections = Vec::new();

    let mut device = Vec::new();
    device.push(("Model".into(), device_name.to_string()));
    if !info.arch.is_empty() {
        device.push(("Arch".into(), info.arch.clone()));
    }
    if !info.soc.is_empty() && info.soc != "unknown" {
        device.push(("SoC".into(), info.soc.clone()));
    }
    sections.push(("Device", device));

    let mut sys = Vec::new();
    sys.push(("OS".into(), if !info.os.is_empty() { info.os.clone() } else { "—".into() }));
    if !rom.is_empty() { sys.push(("ROM".into(), rom.to_string())); }
    sys.push(("Kernel".into(), if !info.kernel.is_empty() { info.kernel.clone() } else { "—".into() }));
    if !info.security_patch.is_empty() {
        sys.push(("Security".into(), info.security_patch.clone()));
    }
    sys.push(("Uptime".into(), info.uptime.clone()));
    if !info.packages.is_empty() && info.packages != "—" {
        sys.push(("Packages".into(), info.packages.clone()));
    }
    sections.push(("System", sys));

    let mut security = Vec::new();
    security.push(("Root".into(), if !info.root_status.is_empty() { info.root_status.clone() } else { "None".into() }));
    security.push(("Bootloader".into(), if !info.bootloader.is_empty() { info.bootloader.clone() } else { "Unknown".into() }));
    security.push(("SELinux".into(), if !info.selinux.is_empty() { info.selinux.clone() } else { "Unknown".into() }));
    sections.push(("Security", security));

    let mut hw = Vec::new();
    hw.push(("RAM".into(), if !info.memory.is_empty() { info.memory.clone() } else { "—".into() }));
    hw.push(("Storage".into(), if !info.storage.is_empty() { info.storage.clone() } else if !info.disk.is_empty() { info.disk.clone() } else { "—".into() }));
    if !info.cpu_temp.is_empty() { hw.push(("CPU Temp".into(), info.cpu_temp.clone())); }
    if !info.brightness.is_empty() { hw.push(("Brightness".into(), info.brightness.clone())); }
    if !info.refresh_rate.is_empty() { hw.push(("Refresh".into(), info.refresh_rate.clone())); }
    sections.push(("Hardware", hw));

    let mut bat = Vec::new();
    bat.push(("Level".into(), if !info.battery_level.is_empty() { info.battery_level.clone() } else { "—".into() }));
    bat.push(("Temp".into(), if !info.battery_temp.is_empty() { info.battery_temp.clone() } else { "—".into() }));
    if !info.battery_health.is_empty() { bat.push(("Health".into(), info.battery_health.clone())); }
    bat.push(("Status".into(), if !info.battery_status.is_empty() { info.battery_status.clone() } else { "—".into() }));
    sections.push(("Battery", bat));

    sections
}

fn build_ascii_info(info: &SysInfo, device_name: &str) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("{}  {}", device_name,
        if !info.os.is_empty() { &info.os } else { "" }));
    if !info.rom.is_empty() {
        lines.push(info.rom.clone());
    }
    if !info.kernel.is_empty() {
        lines.push(info.kernel.clone());
    }
    if !info.battery_level.is_empty() {
        let temp = if !info.battery_temp.is_empty() { format!(" | {}", info.battery_temp) } else { String::new() };
        lines.push(format!("Battery: {}{}", info.battery_level, temp));
    }
    if !info.memory.is_empty() {
        lines.push(format!("RAM: {}", info.memory));
    }
    if !info.storage.is_empty() || !info.disk.is_empty() {
        let stor = if !info.storage.is_empty() { &info.storage } else { &info.disk };
        lines.push(format!("Storage: {}", stor));
    }
    if !info.root_status.is_empty() {
        lines.push(format!("Root: {}", info.root_status));
    }
    lines
}
