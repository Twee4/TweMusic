pub mod desktop;
pub mod engine;
pub mod dynamic_color;
pub mod import;
pub mod library;
pub mod lyrics;
pub mod lyrics_net;
pub mod media;
pub mod models;
pub mod notifications;
pub mod prefetch;
pub mod settings;
pub mod state;
pub mod theme;
pub mod tray;
pub mod ytdlp;
pub mod ytmusic;

// The player and the InnerTube client live in their own crates; these keep the old paths.
pub use twe_innertube::player as stream;
pub use twe_player::mpv;

/// True when the `TWEMUSIC_DEBUG` environment variable is set (to anything but `0`).
pub fn debug_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("TWEMUSIC_DEBUG").is_ok_and(|value| value != "0"))
}

/// Prints a diagnostic line to the terminal, only when `TWEMUSIC_DEBUG` is set.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if $crate::debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}
