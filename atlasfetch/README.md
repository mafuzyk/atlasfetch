<h1 align="center">atlasfetch</h1>

<p align="center">
  <b>A configurable fetch tool — centered ASCII art with powerline panels.</b><br>
  Zero external dependencies · Python ≥ 3.6 · Linux
</p>

<p align="center">
  <a href="#features">Features</a> · 
  <a href="#quick-start">Quick Start</a> · 
  <a href="#usage">Usage</a> · 
  <a href="#customization">Customization</a> · 
  <a href="#presets">Presets</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/python-%E2%89%A53.6-blue" alt="Python">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/status-stable-brightgreen" alt="Status">
</p>

---

## Features

| Pillar | Description |
|--------|-------------|
| **Centered ASCII** | Distro‑aware logos (18 distros included), auto‑adaptive to terminal width |
| **Powerline panels** | Left + right sidebars with Nerd Font icons, auto‑truncation, cascade shift |
| **Zero forks** | Pure Python stdlib — `/proc`, `/sys`, and `pci.ids` only. No `pip` needed. |
| **First‑run wizard** | Interactive palette + ASCII chooser on first launch (like hyfetch) |
| **~25 presets** | LGBTQ+ flags + themes (catppuccin, dracula, gruvbox, nord, tokyonight, …) |
| **Custom palettes** | Create and save your own color schemes in the wizard |
| **Multi‑distro** | Package counting for Arch, Debian, Ubuntu, Fedora, Void, Gentoo, NixOS, Alpine, Slackware, Flatpak, Snap |
| **Adaptive layout** | Hides ASCII on narrow terminals, keeps panels readable |

---

## Quick Start

```bash
git clone https://github.com/mafuzyk/atlasfetch.git
cd atlasfetch
cp atlasfetch ~/.local/bin/
atlasfetch
```

First run launches the interactive setup wizard — pick a palette and ASCII logo.

### Dependencies

- **Python** ≥ 3.6 (stdlib only — no pip packages)
- **Nerd Font** (optional — for icons in panels)
- **pci.ids** (optional — for detailed GPU names; falls back to vendor name)

---

## Usage

```
atlasfetch              → Render system info (or wizard on first run)
atlasfetch -i           → Reopen setup wizard
atlasfetch --preset <n> → Apply a preset palette
atlasfetch --list       → List all presets with color swatches
atlasfetch -h           → Show help with all available fields
atlasfetch -v           → Show version
```

### Wiring it up

**Fish** — add to `~/.config/fish/config.fish`:
```fish
if status is-interactive
    atlasfetch
end
```

**Bash** — add to `~/.bashrc`:
```bash
if [[ $- == *i* ]]; then
    atlasfetch
fi
```

---

## Customization

Edit `~/.config/atlasfetch/config.json` to rearrange fields, change icons, or tweak the layout.

| Field | Description |
|-------|-------------|
| `logo.path` | Path to custom ASCII art file |
| `logo.colors` | Array of hex colors for the palette |
| `title.format` | Title template (`{user}@{host}`) |
| `panel.left_pad` | Left margin for panels |
| `panel.max_shift` | Cascade shift intensity |
| `display.left` / `display.right` | Array of `[field, icon, label]` entries |

**Available fields:** `os`, `user`, `host`, `kernel`, `uptime`, `packages`, `shell`, `terminal`, `cpu`, `gpu`, `memory`, `disk`, `wm`, `load`, `processes`, `local_ip`, `resolution`, `de`, `font`

---

## Presets

Choose from **12 pride/flags** and **13 themes** via `--setup` or `--preset`.

| Flags | Themes |
|-------|--------|
| xenogender, trans, nb, genderfluid, pan, bi, ace, lesbian, gay, intersex, aromantic, agender | arch, catppuccin‑mocha, catppuccin‑latte, dracula, gruvbox, tokyonight, nord, everforest, solarized‑dark, monokai, one‑dark, rose‑pine, synthwave |

Create custom palettes in the wizard or add them directly in `config.json`:

```json
{
  "custom_palettes": {
    "my‑theme": ["#ff0000", "#00ff00", "#0000ff"]
  }
}
```

---

## Anatomy

```
  charlie@atlasbox                     ← title
  ──────────────────────────────        ← separator
                                    -`
       OS   CachyOS            .o+`            Up   10h 7m
      Usr   charlie           `ooo/            Term   kitty
     Krn   7.1.3-cachyos     `+oooo:         CPU   AMD Ryzen 3
     Pkg   1766              `+oooooo:        GPU   Radeon …
      Sh   fish              -+oooooo+:        Mem   3.2/7.6G
      WM   Hyprland        `/:-:++oooo+:       Dsk   28/58G
```

---

## Architecture

```
atlasfetch          → single Python script (no pip)
├── ATLAS_LOGO      → default ASCII (Arch Linux)
├── DISTRO_LOGOS    → 18 official neofetch logos
├── PRESETS         → 25 color presets
├── DEFAULT_CFG     → default config structure
├── _collect_info() → gathers all system fields from /proc, /sys, os.uname
├── render()        → centered ASCII + powerline panel engine
├── _run_setup()    → interactive wizard
└── main()          → CLI dispatch
```

---

## License

GPL‑3.0‑or-later
