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

/// Build the official YouTube Music watch URL for a parsed video ID.
/// Only standard 11-character YouTube IDs are accepted.
pub fn youtube_music_url(video_id: &str) -> Option<String> {
    let valid = video_id.len() == 11
        && video_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    valid.then(|| format!("https://music.youtube.com/watch?v={video_id}"))
}

fn collect_video_renderers(value: &Value, tracks: &mut Vec<Track>, seen: &mut HashSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(renderer) = object.get("videoRenderer") {
                if let Some(track) = parse_video_renderer(renderer, seen) {
                    tracks.push(track);
                }
            }
            if let Some(renderer) = object.get("musicResponsiveListItemRenderer") {
                if let Some(track) = parse_music_item(renderer, seen) {
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
        video_id: Some(video_id.to_owned()),
        thumbnail_url: renderer.pointer("/thumbnail/thumbnails").and_then(best_thumbnail),
    })
}

/// Parses a YouTube Music search row (`musicResponsiveListItemRenderer`), the shape
/// returned by the WEB_REMIX client: title in the first flex column, then
/// "artist • album • duration" runs in the second.
fn parse_music_item(renderer: &Value, seen: &mut HashSet<String>) -> Option<Track> {
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)
        .or_else(|| {
            renderer
                .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
                .and_then(Value::as_str)
        })?;
    let title = renderer
        .pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text")
        .and_then(text_field)?;
    let runs = renderer
        .pointer("/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs")
        .and_then(Value::as_array)?;
    if !seen.insert(video_id.to_owned()) {
        return None;
    }

    let mut groups: Vec<String> = vec![String::new()];
    for run in runs {
        let text = run.get("text").and_then(Value::as_str).unwrap_or_default();
        if text.trim() == "•" {
            groups.push(String::new());
        } else if let Some(group) = groups.last_mut() {
            group.push_str(text);
        }
    }

    let duration = match groups.last().and_then(|group| parse_duration(group.trim())) {
        Some(duration) => {
            groups.pop();
            duration
        }
        None => Duration::default(),
    };
    if matches!(groups.first().map(|group| group.trim()), Some("Song" | "Video")) {
        groups.remove(0);
    }
    let artist = groups
        .first()
        .map(|group| group.trim().to_owned())
        .filter(|group| !group.is_empty())
        .unwrap_or_else(|| "Unknown artist".to_owned());
    let album = groups
        .get(1)
        .map(|group| group.trim().to_owned())
        .filter(|group| !group.is_empty());

    Some(Track {
        id: crate::models::TrackId(stable_id(video_id)),
        title,
        artist,
        album,
        duration,
        video_id: Some(video_id.to_owned()),
        thumbnail_url: renderer
            .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
            .and_then(best_thumbnail),
    })
}

/// Image hosts TweMusic is willing to fetch album art from.
const THUMBNAIL_HOSTS: [&str; 3] = ["ytimg.com", "googleusercontent.com", "ggpht.com"];

/// True for https URLs on YouTube's own image hosts. Anything else is never fetched.
pub fn is_allowed_thumbnail_url(url: &str) -> bool {
    if url.len() > 2048 {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else { return false; };
    let authority = rest.split(|c| c == '/' || c == '?' || c == '#').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || authority.contains(':') {
        return false;
    }
    let host = authority.to_ascii_lowercase();
    THUMBNAIL_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// Asks Google's image servers for a size that stays sharp at 48dp on HiDPI screens.
fn upscale_thumbnail(url: &str) -> String {
    const SIZED: &str = "=w226-h226-l90-rj";
    let is_google_image = url.contains("googleusercontent.com/") || url.contains("ggpht.com/");
    if let (true, Some(index)) = (is_google_image, url.rfind('=')) {
        let suffix = &url[index + 1..];
        let looks_sized = suffix.starts_with('w') && suffix[1..].starts_with(|c: char| c.is_ascii_digit());
        if looks_sized {
            return format!("{}{}", &url[..index], SIZED);
        }
    }
    url.to_owned()
}

/// The largest allowed thumbnail in a `thumbnails` array (YouTube lists smallest first).
pub(crate) fn best_thumbnail(thumbnails: &Value) -> Option<String> {
    thumbnails
        .as_array()?
        .iter()
        .rev()
        .filter_map(|item| item.get("url").and_then(Value::as_str))
        .find(|url| is_allowed_thumbnail_url(url))
        .map(upscale_thumbnail)
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

pub(crate) fn stable_id(value: &str) -> u64 {
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
            "videoId": "abc123_XYZ0",
            "title": { "runs": [{ "text": "Example Song" }] },
            "longBylineText": { "simpleText": "Example Artist" },
            "lengthText": { "simpleText": "3:42" }
        });
        let mut seen = HashSet::new();
        let track = parse_video_renderer(&renderer, &mut seen).unwrap();
        assert_eq!(track.title, "Example Song");
        assert_eq!(track.artist, "Example Artist");
        assert_eq!(track.duration.as_secs(), 222);
        assert_eq!(track.video_id.as_deref(), Some("abc123_XYZ0"));
        assert!(parse_video_renderer(&renderer, &mut seen).is_none());
    }

    #[test]
    fn parses_music_search_rows() {
        let row = json!({
            "playlistItemData": { "videoId": "abc123_XYZ0" },
            "flexColumns": [
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Example Song" }] } } },
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                    { "text": "Example Artist" }, { "text": " • " },
                    { "text": "Example Album" }, { "text": " • " },
                    { "text": "3:42" }
                ] } } }
            ]
        });
        let response = json!({ "contents": [{ "musicResponsiveListItemRenderer": row }] });
        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        collect_video_renderers(&response, &mut tracks, &mut seen);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Example Song");
        assert_eq!(tracks[0].artist, "Example Artist");
        assert_eq!(tracks[0].album.as_deref(), Some("Example Album"));
        assert_eq!(tracks[0].duration.as_secs(), 222);
        assert_eq!(tracks[0].video_id.as_deref(), Some("abc123_XYZ0"));
    }

    #[test]
    fn thumbnail_urls_are_limited_to_youtube_image_hosts() {
        assert!(is_allowed_thumbnail_url("https://i.ytimg.com/vi/abc/hq720.jpg"));
        assert!(is_allowed_thumbnail_url("https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"));
        assert!(!is_allowed_thumbnail_url("http://i.ytimg.com/vi/abc/hq720.jpg"));
        assert!(!is_allowed_thumbnail_url("https://evil.example/ytimg.com/x.jpg"));
        assert!(!is_allowed_thumbnail_url("https://ytimg.com.evil.example/x.jpg"));
        assert!(!is_allowed_thumbnail_url("https://user@i.ytimg.com/x.jpg"));
        assert!(!is_allowed_thumbnail_url("https://i.ytimg.com:8080/x.jpg"));
        assert!(!is_allowed_thumbnail_url("file:///etc/passwd"));
    }

    #[test]
    fn best_thumbnail_prefers_the_largest_allowed_and_resizes_google_images() {
        let thumbnails = json!([
            { "url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj", "width": 60, "height": 60 },
            { "url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj", "width": 120, "height": 120 },
            { "url": "https://tracker.example/pixel.png", "width": 500, "height": 500 }
        ]);
        assert_eq!(
            best_thumbnail(&thumbnails).as_deref(),
            Some("https://lh3.googleusercontent.com/abc=w226-h226-l90-rj")
        );
        assert_eq!(best_thumbnail(&json!([{ "url": "https://tracker.example/x.png" }])), None);
        assert_eq!(best_thumbnail(&json!("nope")), None);
        // Video thumbnails on ytimg.com are used as they are.
        let video = json!([{ "url": "https://i.ytimg.com/vi/abc/hq720.jpg?sqp=x" }]);
        assert_eq!(best_thumbnail(&video).as_deref(), Some("https://i.ytimg.com/vi/abc/hq720.jpg?sqp=x"));
    }

    #[test]
    fn youtube_music_url_rejects_malformed_ids() {
        assert_eq!(youtube_music_url("abc123_XYZ0"), Some("https://music.youtube.com/watch?v=abc123_XYZ0".to_owned()));
        assert!(youtube_music_url("not a video id").is_none());
    }
}
