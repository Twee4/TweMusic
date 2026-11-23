//! Looks lyrics up on LRCLIB (https://lrclib.net), a free service: no account or key.

use std::time::Duration;

use serde_json::Value;

use crate::lyrics::{self, Lyrics};

const BASE: &str = "https://lrclib.net/api";

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("TweMusic (https://github.com/Twee4/TweMusic)")
        .build()
        .map_err(|error| error.to_string())
}

fn texts(value: &Value) -> Option<Lyrics> {
    lyrics::from_texts(value["syncedLyrics"].as_str(), value["plainLyrics"].as_str())
}

/// Finds lyrics for a song. `Ok(None)` means there are none; blocks on the network, so call it
/// from a worker thread.
pub fn fetch(title: &str, artist: &str, album: Option<&str>, seconds: f64) -> Result<Option<Lyrics>, String> {
    let http = client()?;
    // An exact match on the full details first.
    let mut query = vec![("track_name", title.to_owned()), ("artist_name", artist.to_owned())];
    if let Some(album) = album.filter(|album| !album.is_empty()) {
        query.push(("album_name", album.to_owned()));
    }
    if seconds > 0.0 {
        query.push(("duration", format!("{}", seconds.round() as u64)));
    }
    let response = http.get(format!("{BASE}/get")).query(&query).send().map_err(|error| error.to_string())?;
    if response.status().is_success() {
        let value: Value = response.json().map_err(|error| error.to_string())?;
        if let Some(found) = texts(&value) {
            return Ok(Some(found));
        }
    }
    // Otherwise search with a cleaned-up title and take the closest song in length.
    let cleaned = lyrics::clean_title(title);
    let response = http
        .get(format!("{BASE}/search"))
        .query(&[("track_name", cleaned.as_str()), ("artist_name", artist)])
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let results: Value = response.json().map_err(|error| error.to_string())?;
    let mut best: Option<(f64, Lyrics)> = None;
    for item in results.as_array().into_iter().flatten() {
        let Some(found) = texts(item) else { continue };
        let gap = (item["duration"].as_f64().unwrap_or(0.0) - seconds).abs();
        // A synced match close in length beats everything; far-off lengths are other versions.
        if seconds > 0.0 && gap > 8.0 {
            continue;
        }
        let score = gap - if found.synced { 2.0 } else { 0.0 };
        if best.as_ref().map_or(true, |(current, _)| score < *current) {
            best = Some((score, found));
        }
    }
    Ok(best.map(|(_, found)| found))
}
