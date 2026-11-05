# TweMusic

TweMusic is a Linux-first music client prototype written in Rust.

## Current functionality

- Search YouTube Music for public metadata (unofficial endpoint; may change).
- Open a result in the official YouTube Music website using the system browser.
- Play local audio files with MPV.

YouTube search results are **not played or downloaded by TweMusic**. Listening to a result opens `music.youtube.com`, where playback and account access are handled by YouTube. This keeps stream handling with the service's official player.

## Run and test

```bash
cargo run
cargo test
```

Install MPV for local file playback:

```bash
sudo apt install mpv  # Debian/Ubuntu
sudo dnf install mpv  # Fedora
sudo pacman -S mpv    # Arch Linux
```

Search requires an internet connection. It uses an undocumented YouTube Music metadata endpoint and may need maintenance if the service changes. Use YouTube Music in accordance with its terms and applicable law.

## Development stages

1. Desktop shell and design system
2. Typed app state, search and queue models
3. YouTube Music metadata search
4. MPV local-file playback
5. Reliable official integration, Linux media controls, persistence, packaging, and UI polish
