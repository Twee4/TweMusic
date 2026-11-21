# TweMusic, Slint front end (work in progress)

The same app logic as the egui version (`twe_music::engine`), drawn with Slint.

```bash
cargo run --release -p twe_slint
```

Stages 2 and 3 of the move: Home with real data (history, liked songs, today's pick and similar songs,
album art) and a working player (play/pause, previous/next, seek, like), plus Search (the box collapses when
you tap outside it), the Library (playlists, import, remove, play all) and Settings (saved to
settings.json). The tray, media keys, notifications, dynamic colour and gapless playback come in
the next stage. The egui app
(`cargo run --release`) is unchanged.
