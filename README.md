# TweMusic

TweMusic is a native Linux YouTube Music client built from scratch in Rust with a Material 3-inspired interface.

## Current stage

This repository contains the initial desktop app shell for a Linux-first YouTube Music client.

## Stage plan

1. Desktop shell and Material 3-inspired design system
2. App state, navigation, and typed models
3. YouTube Music API client and auth flow
4. MPV playback and MPRIS/Linux integration
5. Packaging, polishing, and release builds

## Run

```bash
cargo run
```

## Tech stack

- Rust
- `eframe` / `egui` for the desktop UI
- Linux native integration planned: D-Bus, MPRIS, media keys, tray
