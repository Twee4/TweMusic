# TweMusic

TweMusic is a Linux-first desktop music client prototype written in Rust, with a Material 3-inspired interface.

## Stages

1. Desktop shell and design system
2. App state, navigation, typed track models, local search, and queue controls
3. YouTube Music metadata search
4. MPV local-file playback
5. YouTube Music playback, MPRIS integration, packaging, and polish

## Current stage: Stage 4

TweMusic can play a local audio file through MPV. In **Settings**, enter the path to an audio file and choose **Play file**; the bottom player has a play/pause control. MPV is started on demand and controlled over its local Unix-domain JSON IPC socket.

Install MPV first, for example:

```bash
sudo apt install mpv  # Debian/Ubuntu
sudo dnf install mpv  # Fedora
sudo pacman -S mpv    # Arch Linux
```

YouTube Music search results are still metadata-only and cannot be played by this stage. No audio is downloaded or extracted from search results. The MPRIS desktop integration and YouTube Music playback remain future work; the current stage does not claim media-key support.

## Run and test

```bash
cargo run
cargo test
```

Search requires an internet connection and relies on an undocumented endpoint that may change. Local playback requires MPV. The project is a prototype; use services in accordance with their terms and applicable law.
