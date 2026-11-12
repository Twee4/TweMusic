//! Turns InnerTube JSON (search and browse responses) into [`TrackInfo`]s.

use std::collections::HashSet;

use serde_json::Value;

use crate::{
    ids::{best_thumbnail, youtube_music_url},
    models::TrackInfo,
};

pub(crate) fn text_field(value: &Value) -> Option<String> {
    value.get("simpleText").and_then(Value::as_str).map(str::to_owned)
        .or_else(|| value.get("runs")?.as_array()?.iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .map(str::to_owned)
            .reduce(|mut left, right| { left.push_str(&right); left }))
}

/// "3:42" or "1:02:03" in seconds.
pub(crate) fn parse_duration(value: &str) -> Option<u64> {
    let mut parts = value.split(':').rev();
    let seconds = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let hours = parts.next().map(str::parse::<u64>).transpose().ok()?.unwrap_or(0);
    Some(hours.saturating_mul(3600).saturating_add(minutes.saturating_mul(60)).saturating_add(seconds))
}

/// The video ID of a music list row, accepted only if it is a well-formed YouTube ID.
fn music_row_video_id(renderer: &Value) -> Option<&str> {
    renderer
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)
        .or_else(|| {
            renderer
                .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
                .and_then(Value::as_str)
        })
        .filter(|id| youtube_music_url(id).is_some())
}

fn row_thumbnail(renderer: &Value) -> Option<String> {
    renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(best_thumbnail)
}

fn flex_text(renderer: &Value, column: usize) -> Option<Value> {
    renderer
        .pointer(&format!("/flexColumns/{column}/musicResponsiveListItemFlexColumnRenderer/text"))
        .cloned()
}

fn parse_video_renderer(renderer: &Value, seen: &mut HashSet<String>) -> Option<TrackInfo> {
    let video_id = renderer.get("videoId")?.as_str()?;
    if !seen.insert(video_id.to_owned()) {
        return None;
    }
    let title = text_field(renderer.get("title")?)?;
    let artist = renderer.get("longBylineText")
        .or_else(|| renderer.get("ownerText"))
        .and_then(text_field)
        .unwrap_or_else(|| "Unknown artist".to_owned());
    let duration_secs = renderer.get("lengthText")
        .and_then(text_field)
        .and_then(|value| parse_duration(&value))
        .unwrap_or_default();

    Some(TrackInfo {
        video_id: video_id.to_owned(),
        title,
        artist,
        album: None,
        duration_secs,
        thumbnail_url: renderer.pointer("/thumbnail/thumbnails").and_then(best_thumbnail),
    })
}

/// Parses a YouTube Music search row (`musicResponsiveListItemRenderer`), the shape
/// returned by the WEB_REMIX client: title in the first flex column, then
/// "artist • album • duration" runs in the second.
fn parse_music_item(renderer: &Value, seen: &mut HashSet<String>) -> Option<TrackInfo> {
    let video_id = music_row_video_id(renderer)?;
    let title = flex_text(renderer, 0).as_ref().and_then(text_field)?;
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

    let duration_secs = match groups.last().and_then(|group| parse_duration(group.trim())) {
        Some(duration) => {
            groups.pop();
            duration
        }
        None => 0,
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

    Some(TrackInfo {
        video_id: video_id.to_owned(),
        title,
        artist,
        album,
        duration_secs,
        thumbnail_url: row_thumbnail(renderer),
    })
}

/// Parses a playlist row: title, artist and album are separate flex columns and the
/// duration is in the fixed column (no "•" separators, unlike search rows).
fn parse_playlist_item(renderer: &Value, seen: &mut HashSet<String>) -> Option<TrackInfo> {
    let video_id = music_row_video_id(renderer)?;
    let title = flex_text(renderer, 0).as_ref().and_then(text_field)?;
    if !seen.insert(video_id.to_owned()) {
        return None;
    }
    let artist = flex_text(renderer, 1)
        .as_ref()
        .and_then(text_field)
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Unknown artist".to_owned());
    let album = flex_text(renderer, 2)
        .as_ref()
        .and_then(text_field)
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    let duration_secs = renderer
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text")
        .and_then(text_field)
        .and_then(|text| parse_duration(text.trim()))
        .unwrap_or_default();

    Some(TrackInfo {
        video_id: video_id.to_owned(),
        title,
        artist,
        album,
        duration_secs,
        thumbnail_url: row_thumbnail(renderer),
    })
}

/// Collects every search result (video or music row) anywhere in a response.
pub fn collect_search_tracks(value: &Value) -> Vec<TrackInfo> {
    fn walk(value: &Value, tracks: &mut Vec<TrackInfo>, seen: &mut HashSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(track) = object.get("videoRenderer").and_then(|renderer| parse_video_renderer(renderer, seen)) {
                    tracks.push(track);
                }
                if let Some(track) = object.get("musicResponsiveListItemRenderer").and_then(|renderer| parse_music_item(renderer, seen)) {
                    tracks.push(track);
                }
                for child in object.values() {
                    walk(child, tracks, seen);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, tracks, seen);
                }
            }
            _ => {}
        }
    }
    let mut tracks = Vec::new();
    walk(value, &mut tracks, &mut HashSet::new());
    tracks
}

/// Parses an "up next" queue entry (`playlistPanelVideoRenderer`): the byline is
/// "artist • album • year" and the duration is in `lengthText`.
fn parse_panel_item(renderer: &Value, seen: &mut HashSet<String>) -> Option<TrackInfo> {
    let video_id = renderer
        .get("videoId")
        .and_then(Value::as_str)
        .filter(|id| youtube_music_url(id).is_some())?;
    let title = renderer.get("title").and_then(text_field).map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())?;
    if !seen.insert(video_id.to_owned()) {
        return None;
    }
    let mut groups: Vec<String> = vec![String::new()];
    if let Some(runs) = renderer.pointer("/longBylineText/runs").and_then(Value::as_array) {
        for run in runs {
            let text = run.get("text").and_then(Value::as_str).unwrap_or_default();
            if text.trim() == "•" {
                groups.push(String::new());
            } else if let Some(group) = groups.last_mut() {
                group.push_str(text);
            }
        }
    }
    let artist = groups
        .first()
        .map(|group| group.trim().to_owned())
        .filter(|group| !group.is_empty())
        .unwrap_or_else(|| "Unknown artist".to_owned());
    // The third group is usually the release year, which is not an album name.
    let album = groups
        .get(1)
        .map(|group| group.trim().to_owned())
        .filter(|group| !group.is_empty() && !group.chars().all(|ch| ch.is_ascii_digit()));
    let duration_secs = renderer
        .get("lengthText")
        .and_then(text_field)
        .and_then(|text| parse_duration(text.trim()))
        .unwrap_or_default();

    Some(TrackInfo {
        video_id: video_id.to_owned(),
        title,
        artist,
        album,
        duration_secs,
        thumbnail_url: renderer.pointer("/thumbnail/thumbnails").and_then(best_thumbnail),
    })
}

/// Collects the "up next" queue entries anywhere in a `next` response.
pub fn collect_queue_tracks(value: &Value) -> Vec<TrackInfo> {
    fn walk(value: &Value, tracks: &mut Vec<TrackInfo>, seen: &mut HashSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(track) = object.get("playlistPanelVideoRenderer").and_then(|renderer| parse_panel_item(renderer, seen)) {
                    tracks.push(track);
                }
                for child in object.values() {
                    walk(child, tracks, seen);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, tracks, seen);
                }
            }
            _ => {}
        }
    }
    let mut tracks = Vec::new();
    walk(value, &mut tracks, &mut HashSet::new());
    tracks
}

/// Collects every playlist row anywhere in a browse response, skipping IDs already in `seen`.
pub fn collect_playlist_tracks(value: &Value, seen: &mut HashSet<String>) -> Vec<TrackInfo> {
    fn walk(value: &Value, tracks: &mut Vec<TrackInfo>, seen: &mut HashSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(track) = object.get("musicResponsiveListItemRenderer").and_then(|renderer| parse_playlist_item(renderer, seen)) {
                    tracks.push(track);
                }
                for child in object.values() {
                    walk(child, tracks, seen);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, tracks, seen);
                }
            }
            _ => {}
        }
    }
    let mut tracks = Vec::new();
    walk(value, &mut tracks, seen);
    tracks
}

/// The playlist's title from its header, if the response has one.
pub fn playlist_title(value: &Value) -> Option<String> {
    const HEADERS: [&str; 4] = [
        "musicResponsiveHeaderRenderer",
        "musicDetailHeaderRenderer",
        "musicEditablePlaylistDetailHeaderRenderer",
        "musicImmersiveHeaderRenderer",
    ];
    match value {
        Value::Object(object) => {
            for key in HEADERS {
                if let Some(header) = object.get(key) {
                    // The editable header nests the real one.
                    let header = header.get("header").and_then(|inner| inner.as_object().and_then(|map| map.values().next())).unwrap_or(header);
                    if let Some(title) = header.get("title").and_then(text_field).filter(|title| !title.trim().is_empty()) {
                        return Some(title.trim().to_owned());
                    }
                }
            }
            object.values().find_map(playlist_title)
        }
        Value::Array(items) => items.iter().find_map(playlist_title),
        _ => None,
    }
}

/// The token for the next page of a playlist, if there is one.
pub fn continuation_token(value: &Value) -> Option<String> {
    match value {
        Value::Object(object) => {
            let token = object
                .get("nextContinuationData")
                .and_then(|data| data.get("continuation"))
                .or_else(|| object.get("continuationCommand").and_then(|command| command.get("token")))
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty() && token.len() < 4096);
            match token {
                Some(token) => Some(token.to_owned()),
                None => object.values().find_map(continuation_token),
            }
        }
        Value::Array(items) => items.iter().find_map(continuation_token),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_duration_with_minutes_and_hours() {
        assert_eq!(parse_duration("3:42"), Some(222));
        assert_eq!(parse_duration("1:02:03"), Some(3723));
        assert_eq!(parse_duration("unknown"), None);
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
        assert_eq!(track.duration_secs, 222);
        assert_eq!(track.video_id, "abc123_XYZ0");
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
        let tracks = collect_search_tracks(&response);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Example Song");
        assert_eq!(tracks[0].artist, "Example Artist");
        assert_eq!(tracks[0].album.as_deref(), Some("Example Album"));
        assert_eq!(tracks[0].duration_secs, 222);
        assert_eq!(tracks[0].video_id, "abc123_XYZ0");
    }

    fn playlist_row(video_id: &str, title: &str, artist: &str, album: &str, duration: &str) -> Value {
        json!({ "musicResponsiveListItemRenderer": {
            "playlistItemData": { "videoId": video_id },
            "flexColumns": [
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": title }] } } },
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": artist }] } } },
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": album }] } } }
            ],
            "fixedColumns": [
                { "musicResponsiveListItemFixedColumnRenderer": { "text": { "runs": [{ "text": duration }] } } }
            ],
            "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [
                { "url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj" }
            ] } } }
        } })
    }

    #[test]
    fn parses_playlist_rows_skipping_bad_ids_and_duplicates() {
        let response = json!({ "contents": { "musicPlaylistShelfRenderer": { "contents": [
            playlist_row("abc123_XYZ0", "One", "Band", "Album", "3:42"),
            playlist_row("abc123_XYZ0", "Duplicate", "Band", "Album", "1:00"),
            playlist_row("bad id", "Rejected", "Band", "Album", "1:00"),
            playlist_row("def456-UVW1", "Two", "Other", "", "0:59"),
        ] } } });
        let tracks = collect_playlist_tracks(&response, &mut HashSet::new());
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "One");
        assert_eq!(tracks[0].artist, "Band");
        assert_eq!(tracks[0].album.as_deref(), Some("Album"));
        assert_eq!(tracks[0].duration_secs, 222);
        assert_eq!(
            tracks[0].thumbnail_url.as_deref(),
            Some("https://lh3.googleusercontent.com/abc=w226-h226-l90-rj")
        );
        assert_eq!(tracks[1].album, None);
        assert_eq!(tracks[1].duration_secs, 59);
    }

    fn panel_item(video_id: &str, title: &str, byline: &[&str], length: &str) -> Value {
        let runs: Vec<Value> = byline.iter().map(|text| json!({ "text": text })).collect();
        json!({ "playlistPanelVideoRenderer": {
            "videoId": video_id,
            "title": { "runs": [{ "text": title }] },
            "longBylineText": { "runs": runs },
            "lengthText": { "runs": [{ "text": length }] },
            "thumbnail": { "thumbnails": [ { "url": "https://i.ytimg.com/vi/abc123_XYZ0/sddefault.jpg" } ] }
        } })
    }

    #[test]
    fn parses_up_next_entries() {
        let response = json!({ "contents": { "playlistPanelRenderer": { "contents": [
            panel_item("abc123_XYZ0", "Seed", &["Band", " • ", "Album", " • ", "2020"], "3:42"),
            panel_item("def456-UVW1", "Related", &["Other", " • ", "2019"], "4:01"),
            panel_item("abc123_XYZ0", "Duplicate", &["Band"], "1:00"),
            panel_item("bad id", "Rejected", &["Band"], "1:00"),
            { "automixPreviewVideoRenderer": { "content": {} } },
        ] } } });
        let tracks = collect_queue_tracks(&response);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].album.as_deref(), Some("Album"));
        assert_eq!(tracks[0].duration_secs, 222);
        assert_eq!(tracks[1].title, "Related");
        assert_eq!(tracks[1].artist, "Other");
        assert_eq!(tracks[1].album, None); // "2019" is a year, not an album
        assert_eq!(tracks[1].duration_secs, 241);
        assert!(tracks[1].thumbnail_url.is_some());
    }

    #[test]
    fn finds_the_playlist_title_and_continuation() {
        let response = json!({ "header": { "musicResponsiveHeaderRenderer": { "title": { "runs": [{ "text": "  My Mix " }] } } },
            "contents": [ { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "TOKEN123" } } } } ] });
        assert_eq!(playlist_title(&response).as_deref(), Some("My Mix"));
        assert_eq!(continuation_token(&response).as_deref(), Some("TOKEN123"));
        let old = json!({ "continuations": [ { "nextContinuationData": { "continuation": "OLD1" } } ] });
        assert_eq!(continuation_token(&old).as_deref(), Some("OLD1"));
        assert_eq!(continuation_token(&json!({ "contents": [] })), None);
        assert_eq!(playlist_title(&json!({})), None);
    }
}
