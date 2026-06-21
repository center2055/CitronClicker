# Citron Clicker Premium

The premium successor to [Citron Clicker](https://github.com/center2055/CitronClicker) — rebuilt from the ground up in **Rust + [egui](https://github.com/emilk/egui)** instead of .NET.

## Why the rewrite

- **Lightweight.** One small native binary, no .NET runtime, no bundled browser. (The old self-contained .NET build was ~130 MB.)
- **Cross-platform.** The UI and config layer are portable across Windows, macOS and Linux. The clicker *engine* is OS-specific (Windows first; macOS needs Accessibility permission; Linux works on X11, Wayland blocks synthetic input).
- **Native feel, no webview.** egui repaints only on interaction, so the GUI won't steal frames from the game.

## Status

Early work in progress — **UI scaffold**. The interface (tabs, CPS sliders, toggles, sound/settings panels) is in place; the clicker engine and sound playback are not wired up yet.

## Building

Requires a recent Rust toolchain.

```bash
cargo run            # debug
cargo build --release
```

## Planned features

- Independent left / right clickers with min–max CPS ranges
- Humanized timing (natural variance + click bursts)
- **Custom click sounds** — built-in packs + load your own `.wav`, volume + pitch variance
- Jitter, Avoid-GUI, suspend key, toggle hotkeys, panic key
- Only-while-in-game gating
- Themeable accent colour, config import/export

## Layout

- **Left Click** / **Right Click** — per-button configuration (independent)
- **Sounds** — click-sound packs, custom files, volume
- **Settings** — accent, startup, tray, panic key, updates

---

made by center2055
