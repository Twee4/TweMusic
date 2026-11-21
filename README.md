# TweMusic

TweMusic is a native Linux music player written in Rust, with a Material 3 Expressive look.

## Features

- **Home:** a greeting, mood filters, quick-access cards and shelves for recently played songs, your playlists and liked songs.
- **Search and play:** find songs online and play the audio-only stream through MPV (no video, no ads), with a queue, previous/next and auto-advance. Playback is gapless, and streams are refreshed before they expire.
- **Library:** Liked songs and your own playlists, saved in `~/.local/share/twe_music/library.json`, plus import of public playlists by link. Recently played songs and daily listening time are kept there too.
- **Dynamic colour:** the whole app takes its colours from the album art of the playing song. Turn it off in Settings.
- **Material 3 design:** light and dark colour roles (follows your system, or pick one in Settings), Material type and shape scales, state layers, chips, list items, and an adaptive layout (navigation bar on narrow windows, navigation rail on wide ones). A glass look sits on a soft colour backdrop. Messages appear as snackbars.
- **Linux integration:** media keys and desktop media widgets (MPRIS), a tray icon and track notifications.
- **Fallback helper (optional):** if a direct lookup is refused, TweMusic can hand it to `yt-dlp` (install it together with `deno` or `node`). It runs without an account and TweMusic never stores or sends your cookies. Advanced: set `ytdlp_browser` in `settings.json` to `firefox`, `chrome`, `chromium`, `brave` or `vivaldi` to use that browser's session instead.

TweMusic itself does not download or store audio. It relies on undocumented online services that can change at any time, which may need an update to TweMusic. Use it at your own risk and in line with the terms of the services you use and the law where you live.

## Layout

```
Cargo workspace
├── crates/innertube   (pure Rust, no UI)  clients.json + clients.rs: client identities and rotation
│                                          transport.rs: HTTP + headers   player.rs: stream lookup
│                                          ids.rs: video IDs, thumbnail host allow-list
├── crates/player      (MPV JSON IPC wrapper)
├── src/               the engine: library, search, today's pick, playback, tray, notifications, MPRIS, colours
├── crates/slint_app   the window (Slint): Home, Library, Settings, Search, player
└── src-tauri/         optional webview experiment (needs WebKitGTK; not built by default)
```

If a client profile stops working, edit `~/.config/twe_music/clients.json` (same format as
`crates/innertube/clients.json`) instead of waiting for a new release. Profiles marked
`"needsCipher": true` are kept in the file but skipped until signature deciphering exists.

## Run and test

```bash
cargo test
cargo run
```

TweMusic needs MPV:

```bash
sudo apt install mpv  # Debian/Ubuntu
sudo dnf install mpv  # Fedora
sudo pacman -S mpv    # Arch Linux
```

Searching and streaming need an internet connection.

The optional webview app is built separately and needs WebKitGTK system libraries
(for Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`):

```bash
cargo run -p twe_music_tauri
```

## Development stages

1. Desktop shell and design system
2. Typed app state, search and queue models
3. Online metadata search
4. MPV playback, gapless and stream refresh
5. Linux media controls, persistence, Home screen and UI polish
