//! Small user settings (tray and notifications), saved as `settings.json` next to the library.

use std::{fs, io, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Show a desktop notification when a new track starts while the window is in the background.
    pub notifications: bool,
    /// Keep a tray icon and keep playing when the window is closed.
    pub tray: bool,
    /// Tint the app with colours taken from the album art of the current song.
    pub dynamic_color: bool,
    /// When YouTube's bot check blocks the direct lookup: the browser whose signed-in cookies
    /// yt-dlp should use instead, or `ytdlp::NO_SIGN_IN` for yt-dlp without cookies (None = never
    /// use yt-dlp). See `ytdlp::BROWSERS`.
    pub ytdlp_browser: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { notifications: true, tray: false, dynamic_color: true, ytdlp_browser: None }
    }
}

pub fn default_path() -> Option<PathBuf> {
    crate::library::default_path().map(|path| path.with_file_name("settings.json"))
}

impl Settings {
    /// A missing or unreadable file gives the defaults.
    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes atomically (temp file, then rename).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(&temp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_files() {
        assert_eq!(Settings::default(), Settings { notifications: true, tray: false, dynamic_color: true, ytdlp_browser: None });
        let partial: Settings = serde_json::from_str(r#"{ "tray": true }"#).unwrap();
        assert_eq!(partial, Settings { notifications: true, tray: true, dynamic_color: true, ytdlp_browser: None });
    }

    #[test]
    fn round_trip_and_bad_file() {
        let dir = std::env::temp_dir().join(format!("twe-music-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let settings = Settings { notifications: false, tray: true, dynamic_color: false, ytdlp_browser: Some("firefox".to_owned()) };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
        fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        let _ = fs::remove_dir_all(&dir);
    }
}
