# Citron Clicker Premium

The premium successor to [Citron Clicker](https://github.com/center2055/CitronClicker) — rebuilt from the ground up in **Rust + [egui](https://github.com/emilk/egui)** instead of .NET.

## Why the rewrite

- **Lightweight.** One small native binary, no .NET runtime, no bundled browser. (The old self-contained .NET build was ~130 MB.)
- **Cross-platform.** The UI and config layer are portable across Windows, macOS and Linux. The clicker *engine* is OS-specific (Windows first; macOS needs Accessibility permission; Linux works on X11, Wayland blocks synthetic input).
- **Native feel, no webview.** egui repaints only on interaction, so the GUI won't steal frames from the game.

## Status

Work in progress — the **interface is built**: branded title bar with the citron wordmark, tabbed layout, dual-handle CPS range sliders, click-distribution histogram, custom toggles, sound + settings panels, and a themeable accent colour. Still to come: the clicker engine, sound playback, key rebinding, and config persistence.

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

## Credits

- UI framework: [egui / eframe](https://github.com/emilk/egui)
- Font: [Poppins](https://fonts.google.com/specimen/Poppins) (SIL Open Font License)
- Icons: [Lucide](https://lucide.dev) (ISC License)

---

made by center2055
