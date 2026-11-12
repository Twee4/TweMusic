# TweMusic

TweMusic is a Linux music client prototype written in Rust.

## Current functionality

- Search YouTube Music for public metadata (unofficial endpoint; may change).
- Play a result: TweMusic asks YouTube's InnerTube API for the audio-only stream and MPV plays it (no video, no ads), with a queue, previous/next and auto-advance.
- Keep a library: Liked songs and your own playlists (saved in `~/.local/share/twe_music/library.json`), plus import of public playlists by link (through InnerTube, no yt-dlp). Play a whole playlist in order.
- Dynamic colour: the whole app takes its colours from the album art of the playing song (dominant hue, Material "tonal spot" tone targets, eased on a spring). Turn it off in Settings. The idea follows [ShinChven/m3-expressive](https://github.com/ShinChven/m3-expressive); the code is written from scratch in Rust.
- Material 3 design: baseline colour roles in light and dark (follows your system, or pick one in Settings), Material type and shape scales, state layers, filled / tonal / outlined / text buttons, filter chips, a search bar, list items, and an adaptive layout (navigation bar under 600dp wide, navigation rail above). Messages appear as snackbars.
- Album art in track rows and the player, loaded only from YouTube's own image hosts (ytimg.com, googleusercontent.com, ggpht.com).
- Seek bar in the player, and Linux media keys / desktop media widgets (MPRIS: play, pause, next, previous, seek).
- Open a result on the official YouTube Music website in your browser.
- Play local audio files with MPV.

Playing a YouTube result looks up its audio stream with the same InnerTube `player` endpoint YouTube's own apps use (acting as the Android VR client, which returns direct stream URLs) and hands the URL to MPV. TweMusic itself does not download or store audio. This may go against YouTube's terms of service, and YouTube can change how streams work at any time, which would need an update to TweMusic. Use it at your own risk.

## Layout

```
Cargo workspace
├── crates/innertube   (pure Rust, no UI)  clients.json + clients.rs: client identities and rotation
│                                          transport.rs: HTTP + headers   player.rs: stream lookup
│                                          ids.rs: video IDs, thumbnail host allow-list
├── crates/player      (MPV JSON IPC wrapper)
├── src/               the native egui app: Material 3 UI, library, tray, notifications, MPRIS
└── src-tauri/         optional webview experiment (needs WebKitGTK; not built by default)
```

If YouTube stops accepting a client profile, edit `~/.config/twe_music/clients.json` (same format as
`crates/innertube/clients.json`) instead of waiting for a new release. Profiles marked
`"needsCipher": true` (the web clients) are kept in the file but skipped until signature deciphering exists.

## Run and test

The default workspace member is the native egui app. The Tauri webview app (which loads
`music.youtube.com`) is built separately and needs WebKitGTK system libraries
(for Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`):

```bash
cargo run -p twe_music_tauri
```

```bash
cargo test
cargo run
```

Install MPV (TweMusic needs nothing else; it no longer uses yt-dlp):

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
