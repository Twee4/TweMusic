//! Resolves a YouTube Music track to a direct audio stream URL by calling InnerTube's
//! `player` endpoint, so playback needs no yt-dlp.
//!
//! The request mimics the Android VR client, which (unlike the web clients) returns
//! plain stream URLs without signature or throttling challenges. The client profiles come
//! from `clients.json` and are tried in turn, because YouTube changes which ones work.

use serde_json::{json, Value};

use crate::{
    clients::{self, ClientProfile},
    ids, transport,
};

/// A resolved audio stream. The user agent must be sent with the media requests, because
/// the stream URL is bound to the client that asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub url: String,
    pub user_agent: String,
    pub mime_type: String,
    pub bitrate: u64,
}

/// Only Google's media hosts over HTTPS are ever handed to MPV.
pub fn is_allowed_stream_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false; };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    host.len() > ".googlevideo.com".len()
        && host.ends_with(".googlevideo.com")
        && host.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
}

/// Picks the best audio-only format that has a direct URL: highest bitrate, Opus on a tie.
pub fn pick_audio_format(response: &Value) -> Result<(String, String, u64), String> {
    let status = response
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if status != "OK" {
        let reason = response
            .pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("not playable");
        return Err(format!("{status}: {reason}"));
    }
    let formats = response
        .pointer("/streamingData/adaptiveFormats")
        .and_then(Value::as_array)
        .ok_or("the response has no streams")?;
    formats
        .iter()
        .filter_map(|format| {
            let mime = format.get("mimeType")?.as_str()?;
            if !mime.starts_with("audio/") { return None; }
            let url = format.get("url")?.as_str()?;
            if !is_allowed_stream_url(url) { return None; }
            let bitrate = format
                .get("averageBitrate")
                .or_else(|| format.get("bitrate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            Some((mime.to_owned(), url.to_owned(), bitrate))
        })
        .max_by_key(|(mime, _, bitrate)| (*bitrate, mime.contains("opus")))
        .map(|(mime, url, bitrate)| (url, mime, bitrate))
        .ok_or_else(|| "no audio stream with a direct URL".to_owned())
}

/// Finds the visitor ID in YouTube's `sw.js_data` payload (it looks like `"visitorData":"Cgt…"`
/// or a bare quoted ID after the `VISITOR_DATA` key). Returns None if it can't be found.
pub fn find_visitor_data(text: &str) -> Option<String> {
    for key in ["\"visitorData\"", "\"VISITOR_DATA\""] {
        let Some(start) = text.find(key) else { continue; };
        let rest = text[start + key.len()..].trim_start().strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        let value = &rest[..end];
        if !value.is_empty() && value.len() < 512 && value.chars().all(|ch| ch.is_ascii_alphanumeric() || "-_%=".contains(ch)) {
            return Some(value.to_owned());
        }
    }
    None
}

fn request_body(client: &ClientProfile, video_id: &str, visitor_data: Option<&str>) -> Value {
    let mut context = serde_json::Map::new();
    context.insert("clientName".into(), json!(client.name));
    context.insert("clientVersion".into(), json!(client.version));
    context.insert("userAgent".into(), json!(client.user_agent));
    context.insert("hl".into(), json!("en"));
    context.insert("gl".into(), json!("US"));
    for (key, value) in &client.extra {
        context.insert(key.clone(), json!(value));
    }
    if let Some(visitor_data) = visitor_data {
        context.insert("visitorData".into(), json!(visitor_data));
    }
    json!({
        "context": { "client": context },
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    })
}

/// Asks YouTube for the audio stream of `video_id`. Blocks on the network, so call it from
/// a worker thread.
pub fn resolve(video_id: &str) -> Result<Stream, String> {
    resolve_with(video_id, &clients::load())
}

pub fn resolve_with(video_id: &str, profiles: &[ClientProfile]) -> Result<Stream, String> {
    if ids::youtube_music_url(video_id).is_none() {
        return Err("YouTube returned an invalid video ID.".to_owned());
    }
    let http = transport::http_client()?;
    let visitor_data = transport::fetch_visitor_page(&http).and_then(|page| find_visitor_data(&page));

    let mut last_error = String::new();
    for index in clients::rotation(profiles.len()) {
        let client = &profiles[index];
        let body = request_body(client, video_id, visitor_data.as_deref());
        let attempt = transport::post_player(&http, client, &body, visitor_data.as_deref())
            .and_then(|value| pick_audio_format(&value));
        match attempt {
            Ok((url, mime_type, bitrate)) => {
                clients::remember_success(index);
                return Ok(Stream { url, user_agent: client.user_agent.clone(), mime_type, bitrate });
            }
            Err(error) => last_error = format!("{} {}: {error}", client.name, client.version),
        }
    }
    Err(format!("Could not get a stream from YouTube ({last_error})."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: &str, formats: Value) -> Value {
        json!({ "playabilityStatus": { "status": status, "reason": "Sign in to confirm your age" },
                "streamingData": { "adaptiveFormats": formats } })
    }

    #[test]
    fn only_google_media_hosts_are_allowed() {
        assert!(is_allowed_stream_url("https://rr1---sn-abc.googlevideo.com/videoplayback?expire=1"));
        assert!(!is_allowed_stream_url("http://rr1---sn-abc.googlevideo.com/videoplayback"));
        assert!(!is_allowed_stream_url("https://googlevideo.com.evil.example/videoplayback"));
        assert!(!is_allowed_stream_url("https://evil.example/?x=.googlevideo.com"));
        assert!(!is_allowed_stream_url("https://.googlevideo.com/x"));
        assert!(!is_allowed_stream_url("file:///etc/passwd"));
    }

    #[test]
    fn picks_the_highest_bitrate_audio_with_a_direct_url() {
        let value = response("OK", json!([
            { "mimeType": "video/mp4; codecs=\"avc1\"", "bitrate": 900000, "url": "https://a.googlevideo.com/v" },
            { "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 130000, "averageBitrate": 128000, "url": "https://a.googlevideo.com/aac" },
            { "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 160000, "averageBitrate": 128000, "url": "https://a.googlevideo.com/opus" },
            { "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 200000, "signatureCipher": "s=abc" },
            { "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 250000, "url": "https://evil.example/x" }
        ]));
        let (url, mime, _) = pick_audio_format(&value).unwrap();
        assert_eq!(url, "https://a.googlevideo.com/opus");
        assert!(mime.contains("opus"));
    }

    #[test]
    fn explains_unplayable_and_empty_responses() {
        let error = pick_audio_format(&response("LOGIN_REQUIRED", json!([]))).unwrap_err();
        assert!(error.contains("LOGIN_REQUIRED") && error.contains("Sign in"));
        assert!(pick_audio_format(&response("OK", json!([]))).is_err());
        assert!(pick_audio_format(&json!({ "playabilityStatus": { "status": "OK" } })).is_err());
    }

    #[test]
    fn finds_visitor_data() {
        assert_eq!(find_visitor_data(r#")]}' [["x","visitorData":"CgtAbc-_123%3D"]]"#).as_deref(), Some("CgtAbc-_123%3D"));
        assert_eq!(find_visitor_data(r#"{"VISITOR_DATA" : "Cgt9"}"#).as_deref(), Some("Cgt9"));
        assert_eq!(find_visitor_data("nothing here"), None);
        assert_eq!(find_visitor_data(r#""visitorData":"bad value""#), None);
    }

    #[test]
    fn request_body_carries_client_and_video() {
        let profiles = clients::builtin();
        let body = request_body(&profiles[0], "abc123_XYZ0", Some("Cgt"));
        assert_eq!(body["videoId"], "abc123_XYZ0");
        assert_eq!(body["context"]["client"]["clientName"], "ANDROID_VR");
        assert_eq!(body["context"]["client"]["osName"], "Android");
        assert_eq!(body["context"]["client"]["visitorData"], "Cgt");
        assert_eq!(request_body(&profiles[1], "abc123_XYZ0", None)["context"]["client"].get("visitorData"), None);
    }
}
