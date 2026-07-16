# AtlasFetch — Documentação Completa do Projeto

## O que é

Ferramenta de system info fetch com ASCII art, powerline panels, e TUI configurator. Roda no PC (Linux) e no Android (Termux). Suporta 32 bits.

---

## Arquitetura

```
src/
  main.rs           — Entry point, dispatch de flags
  cli.rs            — CLI args (clap derive)
  config.rs         — Config struct, load/save JSON, FieldDef
  info.rs           — SysInfo struct, coleta de dados do sistema
  ascii.rs          — Carregamento de arte ASCII (arquivo ou embutida)
  theme.rs          — Cores/palettes
  layout.rs         — Layout variants (Mobile, MobileNarrow, etc.)
  render.rs         — Renderização ANSI + StyledSegments (TUI preview)
  widget.rs         — Widget trait, FieldWidget, Registry
  layout_engine.rs  — Layout enum (Classic/Stack/Minimal/Compact), trait LayoutEngine
  mobile.rs         — Mobile render modes (card, bios, companion, ascii) — ANSI output
  tui/
    mod.rs          — Dispatching: is_android() → mobile::run, senão app::run
    app.rs          — TUI wizard PC (~1960 linhas, 6 steps)
    mobile.rs       — TUI wizard mobile (~686 linhas, 4 steps)
    editor.rs       — Novo editor interativo com preview e layout switcher
```

---

## Flags CLI

| Flag | Descrição |
|------|-----------|
| `--setup` / `-i` | Wizard TUI antigo |
| `--editor` | Novo editor com live preview |
| `--preset <name>` | Aplica palette de cores |
| `--list-presets` | Lista palettes disponíveis |
| `--update` | Pull, build, install com `install(1)` |
| `--mode <mode>` | Mobile render mode (card/bios/companion/ascii) |
| `--reset` | Deleta config.json e abre setup |
| `--just-ascii` | Printa só a arte ASCII colorida |

---

## Mobile Detection

`info::is_android()` checa `TERMUX_VERSION` env var ou `/system/build.prop`. Um único binário funciona nos dois ambientes.

---

## Layout na tela (render.rs)

### PC Render

Classic fetch: ASCII à esquerda, info panels à direita. Título `user@host`, separador, corpo com panels lado a lado. Logo fit check: se terminal for estreito demais pra caber ASCII + panels, a arte é suprimida.

### Mobile Render

ASCII centralizada no topo (bloco inteiro, não linha por linha), info panels em coluna única abaixo. Title + separator iguais ao PC.

### Centering

`dedent()` remove espaços comuns à esquerda da ASCII. `block_center = (term_width - max_logo_width) / 2`. Linhas mais curtas são right-padded pra largura máxima.

---

## Widget System (widget.rs)

Cada campo de informação é um `FieldWidget` que implementa `Widget trait`:

```rust
pub trait Widget: Send + Sync {
    fn key(&self) -> &str;
    fn label(&self) -> &str;
    fn icon(&self) -> &str;
    fn render(&self, ctx: &RenderCtx) -> WidgetOutput;
    fn min_width(&self) -> usize { 4 }
}
```

`FieldWidget` contém um `FieldDef` (field key, icon, label, enabled). O `render()` produz um `WidgetOutput { ansi, styled, width }`.

`Registry` mapeia field keys pra widgets. `Registry::from_fields(left, right)` constrói a partir de config.

`build_panel` e `build_panel_styled` em render.rs agora delegam pra `FieldWidget::render()`.

---

## Layout Engine (layout_engine.rs)

4 layouts implementados via `LayoutEngine` trait:

| Layout | Descrição |
|--------|-----------|
| Classic | ASCII left, info right (fetch tradicional) |
| Stack | ASCII centrado topo, info abaixo |
| Minimal | Só info, sem ASCII |
| Compact | Info apertada, sem ASCII nem título |

`engine_for(layout)` factory function. Cada engine implementa:

```rust
fn arrange(widgets, ascii_lines, cfg, info, term_width) -> LayoutOutput { title, separator, rows }
```

---

## Editor TUI (tui/editor.rs — --editor)

Novo editor com layout side-by-side (em terminal largo) ou empilhado (estreito):

- **Sidebar esquerda**: seletor de layout (↑/↓), lista de widgets habilitados
- **Preview direito**: renderização ao vivo usando layout engine
- **Footer**: nome do layout + descrição
- **Atalhos**: ↑/↓ troca layout, q/Esc sai

---

## Mobile Render Modes (mobile.rs)

4 modos ANSI puro (sem TUI):

| Mode | Descrição |
|------|-----------|
| card | Box-drawing cards com bordas |
| bios | Estilo terminal/engenharia |
| companion | Progress bars pra battery/RAM/storage |
| ascii | ASCII + info, responsivo |

---

## Config (config.rs)

`Config` struct serializada como JSON em `~/.config/atlasfetch/config.json`:

- `logo.key/path/colors` — arte e palette
- `title.format/color` — `"user@host"`
- `separator.char/length/color`
- `panel.left_pad/right_pad/gap/max_shift/max_val_width/sep_color/val_color`
- `display.left/right` — `Vec<FieldDef>` com field/icon/label/enabled
- `Config::mobile_default()` pra Android

---

## Info Collection (info.rs)

`SysInfo` com campos: user, host, os, kernel, uptime, packages, shell, cpu, gpu, memory, storage, terminal, de, wm, fonts, +18 campos mobile (device, rom, soc, arch, battery_*, root_status, bootloader, selinux, cpu_temp, brightness, refresh_rate, signal, wifi_ssid, security_patch, uptime_days).

`SysInfo::get(field: &str) -> Option<&str>` accessor.

---

## Dependências

clap, serde, serde_json, ratatui + crossterm, color-eyre, libc, directories, unicode-width, regex. Tudo crate Rust, sem dependências de sistema.

---

## Mobile Info (Android)

Detection: `/proc/cpuinfo` (Hardware, Processor), GPU via `/sys/kernel/gpu/` ou DRM, battery via termux-battery-status ou sysfs, root via Magisk/APatch/KernelSU, storage via `df`, sensores via sysfs.

---

## --update

`install(1)` pra substituição atômica do binário em execução. Fallback: `cp` + `mv`. Busca source dir em CWD, caminho do binário, e diretórios comuns (`~/Projetos/`, `~/src/`, etc.).

---

## 32-bit Compatibility

Usa `usize` pra widths/sizes (correto em 32 e 64 bits). `u64` só onde necessário (timestamps, bytes). Sem transmute, asm, ou código arch-specific.
