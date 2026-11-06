# TweMusic

TweMusic is a Linux-first music client prototype written in Rust.

## Current functionality

- Search YouTube Music for public metadata (unofficial endpoint; may change).
- Play a result: MPV streams the audio only (no video, no ads) using yt-dlp, with a queue, previous/next and auto-advance.
- Keep a library: Liked songs and your own playlists (saved in `~/.local/share/twe_music/library.json`), plus import of public playlists by link. Play a whole playlist in order.
- Material 3 design: baseline colour roles in light and dark (follows your system, or pick one in Settings), Material type and shape scales, state layers, filled / tonal / outlined / text buttons, filter chips, a search bar, list items, and an adaptive layout (navigation bar under 600dp wide, navigation rail above). Messages appear as snackbars.
- Seek bar in the player, and Linux media keys / desktop media widgets (MPRIS: play, pause, next, previous, seek).
- Open a result on the official YouTube Music website in your browser.
- Play local audio files with MPV.

Playing a YouTube result hands its URL to MPV, which resolves the audio stream through `yt-dlp`. TweMusic itself does not download or store audio. This may go against YouTube's terms of service, and yt-dlp needs regular updates (`yt-dlp -U`) as YouTube changes. Use it at your own risk.

## Run and test

The default workspace member is the native egui app. The Tauri webview app (which loads
`music.youtube.com`) is built separately and needs WebKitGTK system libraries
(for Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`):

```bash
cargo run -p twe_music_tauri
```

```bash
cargo run
cargo test
```

Install MPV and yt-dlp (MPV plays local files on its own; YouTube tracks also need yt-dlp):

```bash
sudo apt install mpv yt-dlp  # Debian/Ubuntu
sudo dnf install mpv yt-dlp  # Fedora
sudo pacman -S mpv yt-dlp    # Arch Linux
```

Search requires an internet connection. It uses an undocumented YouTube Music metadata endpoint and may need maintenance if the service changes. Use YouTube Music in accordance with its terms and applicable law.

## Development stages

1. Desktop shell and design system
2. Typed app state, search and queue models
3. YouTube Music metadata search
4. MPV local-file playback
5. Reliable official integration, Linux media controls, persistence, packaging, and UI polish
