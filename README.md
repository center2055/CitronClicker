# Citron v2

Version 2 of Citron Clicker, rebuilt from the ground up in **Rust + [egui](https://github.com/emilk/egui)** instead of .NET. (V1 was a .NET app; its source was sold and is no longer published here.)

## Why the rewrite

- **Lightweight.** One small native binary, no .NET runtime, no bundled browser. (The old self-contained .NET build was ~130 MB.)
- **Cross-platform.** The UI and config layer are portable across Windows, macOS and Linux. The clicker *engine* is OS-specific (Windows first; macOS needs Accessibility permission; Linux works on X11, Wayland blocks synthetic input).
- **Native feel, no webview.** egui repaints only on interaction, so the GUI won't steal frames from the game.

## Building

Requires a recent Rust toolchain.

```bash
cargo run            # debug
cargo build --release
```

## Layout

- **Left Click** / **Right Click** — per-button configuration (independent)
- **Sounds** — click-sound packs, custom files, volume
- **Settings** — accent, startup, tray, panic key, updates

## Credits

- UI framework: [egui / eframe](https://github.com/emilk/egui)
- Font: [Poppins](https://fonts.google.com/specimen/Poppins) (SIL Open Font License)
- Icons: [Lucide](https://lucide.dev) (ISC License)
- Default click sound: [Arsenic Client](https://github.com/ArsenicClient/Arsenic) (MIT License)
