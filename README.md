# TweMusic

TweMusic is a Linux-first desktop music client prototype written in Rust, with a Material 3-inspired interface.

## Stages

1. Desktop shell and design system
2. App state, navigation, typed track models, local search, and queue controls
3. YouTube Music metadata search
4. Playback engine and Linux MPRIS/media-key integration
5. Packaging and polish

## Current stage: Stage 3

The Search screen can request public track metadata from YouTube Music. Search runs on a worker thread so the window remains responsive. Results are parsed into typed track models and can be added to the prototype's in-memory queue.

This uses an **undocumented YouTube Music endpoint**, which can change or stop working. It is metadata search only: this build does not authenticate, download, or play audio. Use the service in accordance with YouTube's terms and applicable law.

## Run

```bash
cargo run
```

Run the unit tests with:

```bash
cargo test
```

Search requires an internet connection. If it fails, check the message in the Search screen; endpoint or response changes may require updating `src/ytmusic.rs`.

## Linux build prerequisites

`eframe` uses native windowing and graphics libraries. Install the system packages required by the selected backend if Cargo reports missing `pkg-config` libraries.
