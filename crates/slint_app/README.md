# TweMusic, Slint front end (work in progress)

The same app logic as the egui version (`twe_music::engine`), drawn with Slint.

```bash
cargo run --release -p twe_slint
```

Stages 2 and 3 of the move: Home with real data (history, liked songs, today's pick and similar songs,
album art) and a working player (play/pause, previous/next, seek, like), plus Search (the box collapses when
you tap outside it), the Library (playlists, import, remove, play all) and Settings (saved to
settings.json), and (stage 4) media keys and desktop widgets (MPRIS), the tray icon (closing the
window hides it while music keeps playing), track notifications while hidden, dynamic colour
from the album art, gapless playback and stream refresh. The egui app
(`cargo run --release`) is unchanged.
