//! The user's local library: Liked songs and playlists, saved as JSON on this computer.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::models::{Track, TrackId};

/// Name of the built-in first playlist. It always exists and cannot be deleted.
pub const LIKED: &str = "Liked songs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTrack {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: u64,
    /// Added later; older library files don't have it.
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

impl SavedTrack {
    /// Only tracks that came from YouTube (have a video ID) can be saved.
    pub fn from_track(track: &Track) -> Option<Self> {
        Some(Self {
            video_id: track.video_id.clone()?,
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_secs: track.duration.as_secs(),
            thumbnail_url: track
                .thumbnail_url
                .clone()
                .filter(|url| crate::ytmusic::is_allowed_thumbnail_url(url)),
        })
    }

    pub fn to_track(&self) -> Track {
        Track {
            id: TrackId(crate::ytmusic::stable_id(&self.video_id)),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration: Duration::from_secs(self.duration_secs),
            video_id: Some(self.video_id.clone()),
            // Re-checked on load, since the library file can be edited by hand.
            thumbnail_url: self
                .thumbnail_url
                .clone()
                .filter(|url| crate::ytmusic::is_allowed_thumbnail_url(url)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<SavedTrack>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    pub playlists: Vec<Playlist>,
    /// Recently played tracks, newest first. Added later; older files don't have it.
    #[serde(default)]
    pub history: Vec<SavedTrack>,
    /// Seconds listened per day (days since 1970-01-01). Added later, like `history`.
    #[serde(default)]
    pub listening: BTreeMap<u32, u32>,
}

/// How many recently played tracks are kept.
pub const HISTORY_LIMIT: usize = 40;
/// How many days of listening time are kept.
const LISTENING_DAYS: u32 = 60;

/// `$XDG_DATA_HOME/twe_music/library.json`, falling back to `~/.local/share`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(base.join("twe_music").join("library.json"))
}

/// Returns `items` in a random order (xorshift Fisher-Yates; `seed` picks the order).
pub fn shuffled<T>(mut items: Vec<T>, seed: u64) -> Vec<T> {
    let mut state = seed | 1;
    for end in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        items.swap(end, (state % (end as u64 + 1)) as usize);
    }
    items
}

impl Library {
    pub fn new() -> Self {
        let mut library = Self::default();
        library.ensure_liked();
        library
    }

    fn ensure_liked(&mut self) {
        if self.playlists.first().map(|playlist| playlist.name.as_str()) != Some(LIKED) {
            self.playlists.insert(0, Playlist { name: LIKED.to_owned(), tracks: Vec::new() });
        }
    }

    /// Loads the library. A missing file gives an empty library; an unreadable one is
    /// copied to `library.json.bak` first so a later save can't destroy it.
    pub fn load_from(path: &Path) -> Self {
        let mut library = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Library>(&text).unwrap_or_else(|_| {
                let _ = fs::copy(path, path.with_extension("json.bak"));
                Library::default()
            }),
            Err(_) => Library::default(),
        };
        library.ensure_liked();
        library
    }

    /// Writes the library atomically (temp file, then rename).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(&temp, path)
    }

    /// Notes that a track started playing: it moves to the front of the history.
    pub fn record_play(&mut self, track: &Track) {
        let Some(saved) = SavedTrack::from_track(track) else { return; };
        self.history.retain(|existing| existing.video_id != saved.video_id);
        self.history.insert(0, saved);
        self.history.truncate(HISTORY_LIMIT);
    }

    /// Adds listening time to `day` and forgets days older than the retention window.
    pub fn add_listening(&mut self, day: u32, seconds: u32) {
        *self.listening.entry(day).or_insert(0) += seconds;
        let oldest = day.saturating_sub(LISTENING_DAYS);
        self.listening.retain(|kept, _| *kept >= oldest);
    }

    /// Seconds listened on each of the last `days` days ending at `today`, oldest first.
    pub fn listening_week(&self, today: u32, days: u32) -> Vec<u32> {
        (0..days)
            .map(|back| {
                let day = today.saturating_sub(days - 1 - back);
                self.listening.get(&day).copied().unwrap_or(0)
            })
            .collect()
    }

    pub fn is_liked(&self, track: &Track) -> bool {
        match (self.playlists.first(), track.video_id.as_deref()) {
            (Some(liked), Some(video_id)) => liked.tracks.iter().any(|saved| saved.video_id == video_id),
            _ => false,
        }
    }

    /// Likes or unlikes a track and returns whether it is liked afterwards.
    pub fn toggle_like(&mut self, track: &Track) -> bool {
        if self.is_liked(track) {
            if let Some(video_id) = track.video_id.as_deref() {
                self.remove(0, video_id);
            }
            false
        } else {
            self.add(0, track)
        }
    }

    /// Creates a playlist and returns its index. Blank or duplicate names are rejected.
    pub fn create(&mut self, name: &str) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() || self.playlists.iter().any(|playlist| playlist.name.eq_ignore_ascii_case(name)) {
            return None;
        }
        self.playlists.push(Playlist { name: name.to_owned(), tracks: Vec::new() });
        Some(self.playlists.len() - 1)
    }

    /// Deletes a playlist. Index 0 (Liked songs) cannot be deleted.
    pub fn delete(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.playlists.len() {
            return false;
        }
        self.playlists.remove(index);
        true
    }

    /// Adds a track to a playlist. Returns false if it was already there or can't be saved.
    pub fn add(&mut self, index: usize, track: &Track) -> bool {
        let Some(saved) = SavedTrack::from_track(track) else { return false; };
        let Some(playlist) = self.playlists.get_mut(index) else { return false; };
        if playlist.tracks.iter().any(|existing| existing.video_id == saved.video_id) {
            return false;
        }
        playlist.tracks.push(saved);
        true
    }

    pub fn remove(&mut self, index: usize, video_id: &str) {
        if let Some(playlist) = self.playlists.get_mut(index) {
            playlist.tracks.retain(|saved| saved.video_id != video_id);
        }
    }

    /// Creates a new playlist (adding " (2)" and so on if the name is taken) holding the
    /// tracks. Returns the playlist's index and how many tracks were added.
    pub fn import(&mut self, name: &str, tracks: &[Track]) -> (usize, usize) {
        let base = match name.trim() {
            "" => "Imported playlist",
            trimmed => trimmed,
        };
        let mut unique = base.to_owned();
        let mut suffix = 2;
        while self.playlists.iter().any(|playlist| playlist.name.eq_ignore_ascii_case(&unique)) {
            unique = format!("{base} ({suffix})");
            suffix += 1;
        }
        self.playlists.push(Playlist { name: unique, tracks: Vec::new() });
        let index = self.playlists.len() - 1;
        let added = tracks.iter().filter(|track| self.add(index, track)).count();
        (index, added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(video_id: &str) -> Track {
        Track {
            id: TrackId(crate::ytmusic::stable_id(video_id)),
            title: "Title".to_owned(),
            artist: "Artist".to_owned(),
            album: Some("Album".to_owned()),
            duration: Duration::from_secs(61),
            video_id: Some(video_id.to_owned()),
            thumbnail_url: None,
        }
    }

    #[test]
    fn thumbnails_round_trip_and_old_files_still_load() {
        let mut track = sample("aaaaaaaaaaa");
        track.thumbnail_url = Some("https://i.ytimg.com/vi/aaaaaaaaaaa/hq720.jpg".to_owned());
        let saved = SavedTrack::from_track(&track).unwrap();
        assert_eq!(saved.to_track().thumbnail_url, track.thumbnail_url);

        track.thumbnail_url = Some("https://evil.example/x.png".to_owned());
        assert_eq!(SavedTrack::from_track(&track).unwrap().thumbnail_url, None);

        let old = r#"{ "video_id": "aaaaaaaaaaa", "title": "T", "artist": "A", "album": null, "duration_secs": 5 }"#;
        let loaded: SavedTrack = serde_json::from_str(old).unwrap();
        assert_eq!(loaded.thumbnail_url, None);
    }

    #[test]
    fn history_is_deduplicated_newest_first_and_capped() {
        let mut library = Library::new();
        library.record_play(&sample("aaaaaaaaaaa"));
        library.record_play(&sample("bbbbbbbbbbb"));
        library.record_play(&sample("aaaaaaaaaaa"));
        let ids: Vec<&str> = library.history.iter().map(|saved| saved.video_id.as_str()).collect();
        assert_eq!(ids, ["aaaaaaaaaaa", "bbbbbbbbbbb"]);
        library.record_play(&Track::new(1, "Demo", "Nobody", None, 10));
        assert_eq!(library.history.len(), 2);
        for index in 0..60 {
            library.record_play(&sample(&format!("{index:011}")));
        }
        assert_eq!(library.history.len(), HISTORY_LIMIT);
    }

    #[test]
    fn listening_adds_up_per_day_and_old_files_still_load() {
        let mut library = Library::new();
        library.add_listening(100, 30);
        library.add_listening(100, 20);
        library.add_listening(98, 7);
        assert_eq!(library.listening_week(100, 4), vec![0, 7, 0, 50]);
        library.add_listening(300, 1);
        assert!(!library.listening.contains_key(&100));
        let old = r#"{ "playlists": [] }"#;
        let loaded: Library = serde_json::from_str(old).unwrap();
        assert!(loaded.history.is_empty() && loaded.listening.is_empty());
    }

    #[test]
    fn shuffled_keeps_every_item() {
        let items: Vec<u32> = (0..50).collect();
        let mixed = shuffled(items.clone(), 12345);
        assert_ne!(mixed, items);
        let mut sorted = mixed;
        sorted.sort_unstable();
        assert_eq!(sorted, items);
        assert!(shuffled(Vec::<u32>::new(), 1).is_empty());
    }

    #[test]
    fn liking_toggles() {
        let mut library = Library::new();
        let track = sample("aaaaaaaaaaa");
        assert!(!library.is_liked(&track));
        assert!(library.toggle_like(&track));
        assert!(library.is_liked(&track));
        assert!(!library.toggle_like(&track));
        assert!(!library.is_liked(&track));
    }

    #[test]
    fn add_skips_duplicates_and_demo_tracks_and_remove_works() {
        let mut library = Library::new();
        let index = library.create("Road trip").unwrap();
        let track = sample("aaaaaaaaaaa");
        assert!(library.add(index, &track));
        assert!(!library.add(index, &track));
        assert!(!library.add(index, &Track::new(1, "Demo", "Nobody", None, 10)));
        library.remove(index, "aaaaaaaaaaa");
        assert!(library.playlists[index].tracks.is_empty());
    }

    #[test]
    fn create_and_delete_rules() {
        let mut library = Library::new();
        assert!(library.create("  ").is_none());
        assert!(library.create("liked SONGS").is_none());
        let index = library.create("Focus").unwrap();
        assert!(library.create("focus").is_none());
        assert!(!library.delete(0));
        assert!(library.delete(index));
        assert_eq!(library.playlists.len(), 1);
    }

    #[test]
    fn import_makes_unique_names() {
        let mut library = Library::new();
        let tracks = vec![sample("aaaaaaaaaaa"), sample("bbbbbbbbbbb"), sample("aaaaaaaaaaa")];
        let (first, added) = library.import("Mix", &tracks);
        assert_eq!(added, 2);
        let (second, _) = library.import("Mix", &tracks);
        assert_eq!(library.playlists[first].name, "Mix");
        assert_eq!(library.playlists[second].name, "Mix (2)");
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("twe-music-test-{}", std::process::id()));
        let path = dir.join("library.json");
        let mut library = Library::new();
        library.toggle_like(&sample("aaaaaaaaaaa"));
        library.save_to(&path).unwrap();
        let loaded = Library::load_from(&path);
        assert_eq!(loaded.playlists, library.playlists);
        assert_eq!(loaded.playlists[0].tracks[0].to_track().id, sample("aaaaaaaaaaa").id);
        let _ = fs::remove_dir_all(&dir);
    }
}
