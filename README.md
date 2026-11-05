# TweMusic

TweMusic is a fresh Rust desktop app for Linux focused on a YouTube Music-inspired experience with a Material 3-inspired interface.

## Goals

- Linux-first desktop app shell
- Material 3-inspired design system
- Search, home, library, and settings screens
- Audio playback integration with MPV / MPRIS in later milestones
- Secure token storage and YouTube Music API integration

## Run

```bash
cargo run
```

## Tech stack

- Rust
- `eframe` + `egui` for the desktop UI
- Linux native integration planned: D-Bus, MPRIS, tray, media keys
