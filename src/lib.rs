pub mod dynamic_color;
pub mod icons;
pub mod import;
pub mod library;
pub mod m3;
pub mod media;
pub mod models;
pub mod notifications;
pub mod prefetch;
pub mod settings;
pub mod state;
pub mod theme;
pub mod tray;
pub mod ytmusic;

// The player and the InnerTube client live in their own crates; these keep the old paths.
pub use twe_innertube::player as stream;
pub use twe_player::mpv;
