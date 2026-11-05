# TweMusic

TweMusic is a Linux-first desktop music client prototype written in Rust, with a Material 3-inspired interface.

## Stages

1. Desktop shell and design system
2. App state, navigation, typed track models, search over a demo catalog, and queue controls
3. YouTube Music service client and account flow
4. Playback engine and Linux MPRIS/media-key integration
5. Packaging and polish

## Current stage: Stage 2

The app has typed track IDs and metadata, application state for navigation/search/queue/playback, case-insensitive local catalog search, and player controls wired to the in-memory queue. The catalog is demo data: there is no YouTube Music connection or real audio playback yet.

## Run

```bash
cargo run
```

Run the state-model tests with:

```bash
cargo test
```

## Linux build prerequisites

`eframe` uses native windowing and graphics libraries. On Debian/Ubuntu, install the system packages required by the selected backend before compiling if Cargo reports missing `pkg-config` libraries.
