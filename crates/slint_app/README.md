# TweMusic, Slint app

The window of TweMusic. It only draws; `twe_music::engine` does the work (library, search, today's
pick, playback, tray, notifications, media keys, dynamic colour).

```bash
cargo run --release
```

(or `cargo run --release -p twe_slint`). The interface is in `ui/app.slint`; `src/main.rs` copies
what the engine reports into the window.
