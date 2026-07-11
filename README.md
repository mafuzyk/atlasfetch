<h1 align="center">Atlas</h1>

<p align="center">
  <b>A spatial Wayland compositor — infinite canvas, zero compromise.</b><br>
  Built with Rust &nbsp;·&nbsp; Smithay &nbsp;·&nbsp; KDL config
</p>

<p align="center">
  <a href="#features">Features</a> &nbsp;·&nbsp;
  <a href="#quick-start">Quick Start</a> &nbsp;·&nbsp;
  <a href="#configuration">Configuration</a> &nbsp;·&nbsp;
  <a href="#controls">Controls</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-2021-blue" alt="Rust">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/status-pre--alpha-orange" alt="Status">
  <img src="https://img.shields.io/github/actions/workflow/status/mafuzyk/atlaswm/ci.yml" alt="Build">
</p>

<p align="center">
  <img src="" alt="Atlas demonstration" width="720">
</p>

---

## About

Atlas reimagines the desktop as an **infinite two‑dimensional plane** — a *Global Space* — where every window lives at real‑world coordinates in ℝ². Physical monitors are not containers; they are **viewports**, moving cameras that can pan, zoom, and roam freely across the canvas.

Traditional workspaces force you to compartmentalize: *Workspace 1 for code, Workspace 2 for browser, Workspace 3 for chat.* Atlas eliminates those walls. Your terminal stays at `(0, 0)`, your browser at `(1200, 400)`, your music player at `(800, -300)` — you never need to "switch workspace" again. Just look where you want.

This is **pre-alpha**. The architecture is being proven out crate by crate, but the compositor is already functional on the winit backend for safe, risk‑free testing inside your current desktop session.

---

## Features

| Pillar | Description |
|--------|-------------|
| **Infinite Canvas** | Continuous ℝ² coordinate space — windows are not confined to monitor edges. Pan and zoom freely. |
| **Zero‑Leak Architecture** | 100% safe Rust built on Smithay. Every allocation is tracked; memory safety is guaranteed at compile time. |
| **KDL Live Reloading** | Expressive, tree‑structured configuration via [KDL](https://kdl.dev). No TOML/YAML maze. |
| **Layer‑Shell Native** | Full `wlr-layer-shell` support for panels (Waybar, Quickshell) with exclusive‑zone management. |
| **Dual Backend** | Winit backend for safe nested testing **now**; native DRM/udev backend under active development. |

---

## Status

| Question | Answer |
|----------|--------|
| **Multi‑monitor?** | Yes — each physical display is a viewport into the same infinite canvas. Native. |
| **XWayland?** | Planned. The satellite‑process architecture will host XWayland in an isolated plugin. |
| **Backends?** | `winit` (nested, stable for testing) ✅ · `udev`/DRM (native TTY, in development) 🚧 |

---

## Quick Start

```bash
git clone https://github.com/mafuzyk/atlaswm.git
cd atlaswm
RUST_LOG=info cargo run -p compositor
```

> **Safe by default.** With the winit backend, Atlas runs as an ordinary window inside your existing session — no TTY switch, no DRM takeover, no risk of locking yourself out.

### Dependencies

<details>
<summary><b>Debian / Ubuntu</b></summary>

```bash
sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev \
  libegl1-mesa-dev libgles2-mesa-dev libseat-dev libudev-dev
```

</details>

<details>
<summary><b>Arch Linux</b></summary>

```bash
sudo pacman -S base-devel pkgconf wayland wayland-protocols libxkbcommon \
  mesa libegl libglvnd seatd udev
```

</details>

---

## Configuration

The compositor loads `atlas.kdl` from the current working directory during development (`./atlas.kdl`).  
For system‑wide deployment the planned default path is `~/.config/atlas/atlas.kdl`.

### Full example

```kdl
canvas {
    // Solid background color (hex) — fallback when no wallpaper is set
    background-color "#1a1a2e"

    // Wallpaper image (planned syntax)
    // wallpaper "/path/to/wallpaper.jpg" scaling="fill"
}

decoration {
    // Border width in CSS‑style pixels
    border-width 3.0

    // Corner radius — 0.0 for sharp corners, larger values for rounded
    border-radius 0.0

    // Hex color for the focused window border
    active-color "#6699ff"

    // Hex color for unfocused window borders
    inactive-color "#4a4a4a"
}
```

### Customising the background

| Option | Syntax | Description |
|--------|--------|-------------|
| Solid colour | `background-color "#1a1a2e"` | Full‑screen flat colour in `#rrggbb` hex |
| Wallpaper | `wallpaper "/path/to/img.jpg" scaling="fill"` | Image background (planned) |

The compositor uses the solid `background-color` as a fallback when no wallpaper is configured. When both are set, the wallpaper is rendered first and the solid colour is used as a tint/blend layer.

### Window decoration

The `decoration` block controls the client‑side decoration borders rendered by the compositor:

| Property | Default | Description |
|----------|---------|-------------|
| `border-width` | `3.0` | Thickness of the window border in pixels |
| `border-radius` | `0.0` | Corner rounding radius (`0.0` = sharp) |
| `active-color` | `"#6699ff"` | Border colour of the currently focused window |
| `inactive-color` | `"#4a4a4a"` | Border colour of unfocused windows |

---

## Controls

| Input | Action |
|-------|--------|
| **Mod+Enter** | Spawn terminal (fish, gnome-terminal, alacritty, kitty, foot, weston-terminal, xterm) |
| **Mod+Q** | Close focused window |
| **Mod+Left‑Click** | Drag window |
| **Mod+Right‑Click** | Resize window |
| **Mod+Arrow** | Nudge focused window 20 canvas‑units |
| **Arrow keys** | Pan viewport |
| **Left‑Click** | Focus window |

The **Mod** key is the **Super / Windows** key (evdev 125).

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   compositor (bin)                   │
│  loads atlas.kdl, calls atlas_core::run()            │
├─────────────────────────────────────────────────────┤
│                    atlas-core                        │
│  event loop, state machine, Smithay handler impls    │
├────────┬──────┬──────┬──────┬──────┬──────┬─────────┤
│atlas-  │atlas- │atlas-│atlas-│atlas-│atlas-│atlas-   │
│space   │config │render│ input│output│ layout│  wm     │
│(Global │ (KDL  │(Gles │(evdev│(output│(floating│(window │
│Space + │parser)│pipeline)│seat) │mgt)  │tiling) │rules)  │
│Viewport)│      │      │      │      │       │        │
├────────┴──────┴──────┴──────┴──────┴──────┴─────────┤
│          atlas-plugin-api / atlas-plugin              │
│  WASM plugin runtime (wasmtime) + WIT interface       │
│                    atlas-ipc                          │
│  Unix‑socket IPC for external tools                   │
└──────────────────────────────────────────────────────┘
```

### Crates

| Crate | Status | Role |
|-------|--------|------|
| `compositor` | ✅ Live | Binary entry point |
| `atlas-core` | ✅ Live | Backends, state machine, Smithay handlers |
| `atlas-space` | ✅ Live | `GlobalSpace` — infinite canvas coordinate manager + `Viewport` |
| `atlas-config` | ✅ Live | KDL config parser (knuffel derive) |
| `atlas-render` | 📋 Planned | Custom render pipeline (border‑radius shader, damage tracking) |
| `atlas-input` | 📋 Planned | Keybind engine, pointer constraints, touch |
| `atlas-output` | 📋 Planned | Multi‑monitor output management |
| `atlas-layout` | 📋 Planned | Floating, tiling, snap clusters |
| `atlas-wm` | 📋 Planned | Window rules, workspaces |
| `atlas-animation` | 📋 Planned | Spring physics + easing system |
| `atlas-plugin-api` | 📋 Planned | Shared WIT types for WASM plugins |
| `atlas-plugin` | 📋 Planned | WASM runtime (wasmtime) |
| `atlas-ipc` | 📋 Planned | JSON‑over‑Unix‑socket IPC protocol |

---

## atlasfetch

A companion fetch tool with centered ASCII art and powerline panels, auto‑detecting your distro's logo and offering ~25 hand‑crafted color presets. Fully self‑contained — zero `pip` dependencies.

[github.com/mafuzyk/atlasfetch](https://github.com/mafuzyk/atlasfetch) — standalone repo. Zero deps, pure Python, 25 color presets, 18 distro logos, interactive setup wizard.

```bash
git clone https://github.com/mafuzyk/atlasfetch.git
cp atlasfetch/atlasfetch ~/.local/bin/
atlasfetch          # first run opens the interactive setup wizard
```

---

## Why "Atlas"?

Like the Titan who holds up the sky, Atlas holds your workspace — a potentially infinite canvas that doesn't constrain windows to the edges of physical displays.

---

## License

GPL‑3.0‑or-later
