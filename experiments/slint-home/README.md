# Slint Home trial

TweMusic's Home screen rebuilt in [Slint](https://slint.dev), with sample songs and no playback, to
compare against the egui version. It is a separate project: `cargo test` in the repository root
does not build it.

```bash
cd experiments/slint-home
cargo run --release
```

What it shows: the aurora backdrop, a floating glass navigation rail, the top bar, the Today's
pick card, the quick-access grid, a shelf, and the player card, all laid out with Slint's own
layouts (no hand-measured widths). Hover and press effects use `animate`.

Not in the trial: playback, search, the real library, dynamic colour from album art, real blur.
Slint is licensed under GPL-3.0, a royalty-free licence with conditions, or a commercial licence;
read the terms before shipping.
