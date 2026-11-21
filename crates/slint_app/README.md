# TweMusic, Slint front end (work in progress)

The same app logic as the egui version (`twe_music::engine`), drawn with Slint.

```bash
cargo run --release -p twe_slint
```

Stage 2 of the move: Home with real data (history, liked songs, today's pick and similar songs,
album art) and a working player (play/pause, previous/next, seek, like). Search, Library,
Settings, the tray, media keys and dynamic colour come in later stages. The egui app
(`cargo run --release`) is unchanged.
