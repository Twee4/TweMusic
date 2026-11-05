use std::{collections::HashSet, time::Duration};

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::models::Track;

const SEARCH_URL: &str = "https://music.youtube.com/youtubei/v1/search?prettyPrint=false";
const CLIENT_VERSION: &str = "1.20250616.01.00";

#[derive(Debug)]
pub enum SearchError {
    Request(reqwest::Error),
    InvalidResponse(&'static str),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => write!(f, "YouTube Music request failed: {error}"),
            Self::InvalidResponse(message) => write!(f, "Could not read YouTube Music response: {message}"),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<reqwest::Error> for SearchError {
    fn from(error: reqwest::Error) -> Self { Self::Request(error) }
}

/// Fetches public search metadata only. This relies on an undocumented endpoint,
/// so it may stop working when YouTube changes its client protocol.
pub fn search_tracks(query: &str) -> Result<Vec<Track>, SearchError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36")
        .build()?;
    let response: Value = client
        .post(SEARCH_URL)
        .json(&json!({
            "context": { "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US"
            }},
            "query": query,
            "params": "EgWKAQIIAWoKEAkQChADEAQQBQ%3D%3D"
        }))
        .send()?
        .error_for_status()?
        .json()?;

    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    collect_video_renderers(&response, &mut tracks, &mut seen);
    Ok(tracks)
}

fn collect_video_renderers(value: &Value, tracks: &mut Vec<Track>, seen: &mut HashSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(renderer) = object.get("videoRenderer") {
                if let Some(track) = parse_video_renderer(renderer, seen) {
                    tracks.push(track);
                }
            }
            for child in object.values() {
                collect_video_renderers(child, tracks, seen);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_video_renderers(child, tracks, seen);
            }
        }
        _ => {}
    }
}

fn parse_video_renderer(renderer: &Value, seen: &mut HashSet<String>) -> Option<Track> {
    let video_id = renderer.get("videoId")?.as_str()?;
    if !seen.insert(video_id.to_owned()) {
        return None;
    }
    let title = text_field(renderer.get("title")?)?;
    let artist = renderer.get("longBylineText")
        .or_else(|| renderer.get("ownerText"))
        .and_then(text_field)
        .unwrap_or_else(|| "Unknown artist".to_owned());
    let duration = renderer.get("lengthText")
        .and_then(text_field)
        .and_then(|value| parse_duration(&value))
        .unwrap_or_default();

    Some(Track {
        id: crate::models::TrackId(stable_id(video_id)),
        title,
        artist,
        album: None,
        duration,
    })
}

fn text_field(value: &Value) -> Option<String> {
    value.get("simpleText").and_then(Value::as_str).map(str::to_owned)
        .or_else(|| value.get("runs")?.as_array()?.iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .map(str::to_owned)
            .reduce(|mut left, right| { left.push_str(&right); left }))
}

fn parse_duration(value: &str) -> Option<Duration> {
    let mut parts = value.split(':').rev();
    let seconds = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let hours = parts.next().map(str::parse::<u64>).transpose().ok()?.unwrap_or(0);
    Some(Duration::from_secs(hours.saturating_mul(3600).saturating_add(minutes.saturating_mul(60)).saturating_add(seconds)))
}

fn stable_id(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duration_with_minutes_and_hours() {
        assert_eq!(parse_duration("3:42").unwrap().as_secs(), 222);
        assert_eq!(parse_duration("1:02:03").unwrap().as_secs(), 3723);
        assert!(parse_duration("unknown").is_none());
    }

    #[test]
    fn extracts_renderer_metadata_and_deduplicates_ids() {
        let renderer = json!({
            "videoId": "abc123",
            "title": { "runs": [{ "text": "Example Song" }] },
            "longBylineText": { "simpleText": "Example Artist" },
            "lengthText": { "simpleText": "3:42" }
        });
        let mut seen = HashSet::new();
        let track = parse_video_renderer(&renderer, &mut seen).unwrap();
        assert_eq!(track.title, "Example Song");
        assert_eq!(track.artist, "Example Artist");
        assert_eq!(track.duration.as_secs(), 222);
        assert!(parse_video_renderer(&renderer, &mut seen).is_none());
    }
}
