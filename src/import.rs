//! Imports a public YouTube / YouTube Music playlist by link, using `yt-dlp`'s
//! flat playlist listing (metadata only; nothing is downloaded).

use std::{
    collections::HashSet,
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::Value;

use crate::models::{Track, TrackId};

const MAX_ENTRIES: usize = 1000;

pub fn is_supported_playlist_url(url: &str) -> bool {
    ["https://music.youtube.com/", "https://www.youtube.com/", "https://youtube.com/"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
        && url.contains("list=")
}

/// Runs yt-dlp and returns the playlist's title and tracks.
pub fn import_playlist(url: &str) -> Result<(String, Vec<Track>), String> {
    let url = url.trim();
    if !is_supported_playlist_url(url) {
        return Err("Paste a public playlist link from music.youtube.com or youtube.com (it must contain list=).".to_owned());
    }
    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "-J", "--no-warnings", "--"])
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run yt-dlp (is it installed?): {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", stderr.lines().last().unwrap_or("unknown error")));
    }
    parse_playlist_json(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_playlist_json(text: &str) -> Result<(String, Vec<Track>), String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("Could not read yt-dlp output: {error}"))?;
    let title = value.get("title").and_then(Value::as_str).unwrap_or("Imported playlist").to_owned();
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("That link is not a playlist.")?;

    let mut seen = HashSet::new();
    let mut tracks = Vec::new();
    for entry in entries.iter().take(MAX_ENTRIES) {
        let Some(id) = entry.get("id").and_then(Value::as_str) else { continue; };
        if crate::ytmusic::youtube_music_url(id).is_none() || !seen.insert(id.to_owned()) {
            continue;
        }
        let title = entry.get("title").and_then(Value::as_str).unwrap_or("Unknown title");
        let artist = entry
            .get("uploader")
            .or_else(|| entry.get("channel"))
            .and_then(Value::as_str)
            .unwrap_or("Unknown artist");
        let seconds = entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u64;
        tracks.push(Track {
            id: TrackId(crate::ytmusic::stable_id(id)),
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: None,
            duration: Duration::from_secs(seconds),
            video_id: Some(id.to_owned()),
        });
    }
    if tracks.is_empty() {
        return Err("No playable tracks found in that playlist.".to_owned());
    }
    Ok((title, tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_youtube_playlist_links() {
        assert!(is_supported_playlist_url("https://music.youtube.com/playlist?list=PLabc"));
        assert!(is_supported_playlist_url("https://www.youtube.com/watch?v=abc123_XYZ0&list=PLabc"));
        assert!(!is_supported_playlist_url("https://music.youtube.com/watch?v=abc123_XYZ0"));
        assert!(!is_supported_playlist_url("https://evil.example/playlist?list=PLabc"));
        assert!(!is_supported_playlist_url("--exec"));
    }

    #[test]
    fn parses_flat_playlist_json() {
        let json = r#"{
            "title": "My Mix",
            "entries": [
                { "id": "abc123_XYZ0", "title": "One", "uploader": "Band", "duration": 200.0 },
                { "id": "abc123_XYZ0", "title": "Duplicate" },
                { "id": "bad id", "title": "Rejected" },
                { "id": "def456-UVW1", "title": "Two", "channel": "Other", "duration": 95 }
            ]
        }"#;
        let (title, tracks) = parse_playlist_json(json).unwrap();
        assert_eq!(title, "My Mix");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].artist, "Band");
        assert_eq!(tracks[0].duration.as_secs(), 200);
        assert_eq!(tracks[1].artist, "Other");
        assert!(parse_playlist_json(r#"{ "title": "x" }"#).is_err());
    }
}
