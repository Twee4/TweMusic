//! Resolves a YouTube Music track to a direct audio stream URL by calling InnerTube's
//! `player` endpoint, so playback needs no yt-dlp.
//!
//! The request mimics the Android VR client, which (unlike the web clients) returns
//! plain stream URLs without signature or throttling challenges. The client profiles come
//! from `clients.json` and are tried in turn, because YouTube changes which ones work.

use std::{sync::Mutex, time::{Duration, Instant}};

use serde_json::{json, Value};

use crate::{
    clients::{self, ClientProfile},
    ids, transport, InnerTubeError,
};

/// Prints a diagnostic line when the `TWEMUSIC_DEBUG` environment variable is set.
fn trace(message: String) {
    if std::env::var("TWEMUSIC_DEBUG").is_ok_and(|value| value != "0") {
        eprintln!("[innertube] {message}");
    }
}

/// A resolved audio stream. The user agent must be sent with the media requests, because
/// the stream URL is bound to the client that asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub url: String,
    pub user_agent: String,
    pub mime_type: String,
    pub bitrate: u64,
}

/// How long to keep asking before giving up on an address that still answers 403.
const READY_PATIENCE: Duration = Duration::from_secs(14);
/// How long the direct lookup keeps trying more clients before the caller falls back.
const LOOKUP_BUDGET: Duration = Duration::from_secs(5);

/// YouTube now answers 403 for the first few seconds after it hands out a stream address
/// (yt-dlp sleeps about six seconds before downloading for this reason). A player that opens
/// the address at once fails, so ask for two bytes until the address starts working. Returns
/// as soon as it does; on any other answer or error it returns too and lets the player decide.
pub fn wait_until_playable(stream: &Stream) {
    wait_until_playable_within(stream, READY_PATIENCE);
}

/// Like `wait_until_playable`, but gives up after `patience`. Addresses that yt-dlp produced are
/// meant for its own headers, so this probe may be refused for good while the player is fine.
pub fn wait_until_playable_within(stream: &Stream, patience: Duration) {
    if !is_allowed_stream_url(&stream.url) {
        return;
    }
    let Ok(http) = crate::transport::http_client() else { return };
    let started = Instant::now();
    let deadline = started + patience;
    loop {
        let answer = http
            .get(&stream.url)
            .header("User-Agent", stream.user_agent.as_str())
            // Ask the way the player does (everything from the start, not just two bytes): an
            // address can answer the small request while still refusing the real one. The answer
            // is read from the headers only and the connection is dropped at once.
            .header("Range", "bytes=0-")
            .send()
            .map(|response| response.status().as_u16());
        match answer {
            Ok(status) if still_warming_up(status) && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            other => {
                trace(format!("address check: {other:?} after {:.1}s", started.elapsed().as_secs_f32()));
                return;
            }
        }
    }
}

/// A 403 on a fresh address means "not yet"; anything else is final.
fn still_warming_up(status: u16) -> bool {
    status == 403
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
pub fn pick_audio_format(response: &Value) -> Result<(String, String, u64), InnerTubeError> {
    let status = response
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if status != "OK" {
        let reason = response
            .pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("not playable");
        return Err(InnerTubeError::Refused(format!("{status}: {reason}")));
    }
    let formats = response
        .pointer("/streamingData/adaptiveFormats")
        .and_then(Value::as_array)
        .ok_or_else(|| InnerTubeError::Api("the response has no streams".to_owned()))?;
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
        .ok_or(InnerTubeError::NoFormat)
}

/// Finds the visitor ID in YouTube's `sw.js_data` payload. The page is a nested list in which the
/// ID is just a quoted string (it begins `Cgt` or `Cgs` and ends `%3D%3D`), so a key like
/// `"visitorData":` is tried first and then any quoted string of that shape. Returns None if
/// neither is found.
pub fn find_visitor_data(text: &str) -> Option<String> {
    let looks_right = |value: &str| {
        value.len() >= 16
            && value.len() < 512
            && value.chars().all(|ch| ch.is_ascii_alphanumeric() || "-_%=".contains(ch))
    };
    for key in ["\"visitorData\"", "\"VISITOR_DATA\""] {
        let Some(start) = text.find(key) else { continue };
        let Some(rest) = text[start + key.len()..].trim_start().strip_prefix(':') else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('"') else { continue };
        let Some(end) = rest.find('"') else { continue };
        if looks_right(&rest[..end]) || (!rest[..end].is_empty() && rest[..end].len() < 512 && rest[..end].chars().all(|ch| ch.is_ascii_alphanumeric() || "-_%=".contains(ch))) {
            return Some(rest[..end].to_owned());
        }
    }
    for prefix in ["\"Cgt", "\"Cgs"] {
        let mut from = 0;
        while let Some(found) = text[from..].find(prefix) {
            let start = from + found + 1;
            let Some(length) = text[start..].find('"') else { break };
            let value = &text[start..start + length];
            if looks_right(value) {
                return Some(value.to_owned());
            }
            from = start;
        }
    }
    None
}

fn request_body(client: &ClientProfile, video_id: &str, visitor_data: Option<&str>) -> Value {
    json!({
        "context": clients::build_context(client, visitor_data),
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    })
}

/// Asks YouTube for the audio stream of `video_id`. Blocks on the network, so call it from
/// a worker thread.
pub fn resolve(video_id: &str) -> Result<Stream, InnerTubeError> {
    resolve_with(video_id, &clients::load())
}

pub fn resolve_with(video_id: &str, profiles: &[ClientProfile]) -> Result<Stream, InnerTubeError> {
    if ids::youtube_music_url(video_id).is_none() {
        return Err(InnerTubeError::Api("Invalid video ID.".to_owned()));
    }
    let started = Instant::now();
    let http = transport::http_client()?;
    // TWEMUSIC_NO_VISITOR=1 skips the visitor ID, to test whether that ID is what gets flagged.
    let visitor_data = if std::env::var("TWEMUSIC_NO_VISITOR").is_ok_and(|value| value != "0") {
        None
    } else {
        cached_visitor_data(&http)
    };
    trace(format!("{video_id}: visitor ID {}", if visitor_data.is_some() { "found" } else { "not used" }));

    // Profiles that need signature deciphering are skipped: this crate can't do that yet.
    let usable = clients::eligible(profiles, "player");
    if usable.is_empty() {
        return Err(InnerTubeError::Api("No usable client profile in clients.json (all need deciphering).".to_owned()));
    }
    let mut failures: Vec<(String, InnerTubeError)> = Vec::new();
    for index in clients::rotation(usable.len()) {
        // When the first clients have all failed, going through the rest rarely helps and the
        // person is waiting: give up and let the caller fall back.
        if !failures.is_empty() && started.elapsed() > LOOKUP_BUDGET {
            break;
        }
        let client = usable[index];
        let body = request_body(client, video_id, visitor_data.as_deref());
        let attempt = transport::post_player(&http, client, &body, visitor_data.as_deref())
            .and_then(|value| pick_audio_format(&value));
        trace(format!(
            "{video_id}: {} {} after {:.1}s: {}",
            client.name,
            client.client_version,
            started.elapsed().as_secs_f32(),
            match &attempt {
                Ok((_, mime, bitrate)) => format!("ok ({mime}, {bitrate} bit/s)"),
                Err(error) => error.to_string(),
            }
        ));
        match attempt {
            Ok((url, mime_type, bitrate)) => {
                clients::remember_success(index);
                return Ok(Stream { url, user_agent: client.user_agent.clone(), mime_type, bitrate });
            }
            Err(error) => failures.push((format!("{} {}", client.name, client.client_version), error)),
        }
    }
    let error = summarize_failures(failures);
    if matches!(error, InnerTubeError::BotCheck) {
        // The visitor ID may be the flagged part: get a fresh one next time.
        forget_visitor_data();
    }
    Err(error)
}

/// How long a visitor ID is reused. One page load per hour instead of one per track keeps the
/// request pattern closer to a real client's and saves a round trip on every song.
const VISITOR_TTL: Duration = Duration::from_secs(60 * 60);
static VISITOR: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn cached_visitor_data(http: &reqwest::blocking::Client) -> Option<String> {
    if let Ok(slot) = VISITOR.lock() {
        if let Some((visitor, fetched)) = slot.as_ref() {
            if fetched.elapsed() < VISITOR_TTL {
                return Some(visitor.clone());
            }
        }
    }
    let page = transport::fetch_visitor_page(http);
    trace(match &page {
        Some(page) => format!("visitor page: {} bytes, ID {}", page.len(), if find_visitor_data(page).is_some() { "found" } else { "not found" }),
        None => "visitor page: request failed".to_owned(),
    });
    let fresh = page.and_then(|page| find_visitor_data(&page))?;
    if let Ok(mut slot) = VISITOR.lock() {
        *slot = Some((fresh.clone(), Instant::now()));
    }
    Some(fresh)
}

fn forget_visitor_data() {
    if let Ok(mut slot) = VISITOR.lock() {
        *slot = None;
    }
}

fn is_bot_check(error: &InnerTubeError) -> bool {
    matches!(error, InnerTubeError::Refused(message) if message.to_ascii_lowercase().contains("not a bot"))
}

/// Explains why every client failed: a network problem if none of them got an answer,
/// otherwise the last answer YouTube gave.
pub fn summarize_failures(failures: Vec<(String, InnerTubeError)>) -> InnerTubeError {
    if !failures.is_empty() && failures.iter().all(|(_, error)| matches!(error, InnerTubeError::Http(_))) {
        return InnerTubeError::Unreachable;
    }
    // Every client that answered said "prove you are not a bot": that is the connection's
    // reputation, not something one profile can fix, so say so plainly.
    let answered: Vec<_> = failures.iter().filter(|(_, error)| !matches!(error, InnerTubeError::Http(_))).collect();
    if !answered.is_empty() && answered.iter().all(|(_, error)| is_bot_check(error)) {
        return InnerTubeError::BotCheck;
    }
    let detail = failures
        .iter()
        .rev()
        .find(|(_, error)| !matches!(error, InnerTubeError::Http(_)))
        .map(|(label, error)| format!("{label}: {error}"))
        .unwrap_or_else(|| "no client answered".to_owned());
    InnerTubeError::Api(format!("Could not get a stream ({detail})."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: &str, formats: Value) -> Value {
        json!({ "playabilityStatus": { "status": status, "reason": "Sign in to confirm your age" },
                "streamingData": { "adaptiveFormats": formats } })
    }

    #[test]
    fn only_a_403_means_wait() {
        assert!(still_warming_up(403));
        assert!(!still_warming_up(200) && !still_warming_up(206) && !still_warming_up(404) && !still_warming_up(410));
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
        let error = pick_audio_format(&response("LOGIN_REQUIRED", json!([]))).unwrap_err().to_string();
        assert!(error.contains("LOGIN_REQUIRED") && error.contains("Sign in"));
        assert!(pick_audio_format(&response("OK", json!([]))).is_err());
        assert!(pick_audio_format(&json!({ "playabilityStatus": { "status": "OK" } })).is_err());
    }

    #[test]
    fn summarizes_why_every_client_failed() {
        let refused = |label: &str| (label.to_owned(), InnerTubeError::Refused("LOGIN_REQUIRED: Sign in".to_owned()));
        let message = summarize_failures(vec![refused("A 1"), refused("B 2")]).to_string();
        assert!(message.contains("B 2") && message.contains("LOGIN_REQUIRED"));
        assert!(summarize_failures(vec![]).to_string().contains("no client answered"));
        assert!(matches!(
            summarize_failures(vec![("A 1".to_owned(), InnerTubeError::NoFormat)]),
            InnerTubeError::Api(_)
        ));
    }

    #[test]
    fn recognises_the_bot_check() {
        let bot = |label: &str| (
            label.to_owned(),
            InnerTubeError::Refused("LOGIN_REQUIRED: Sign in to confirm you're not a bot".to_owned()),
        );
        assert!(matches!(summarize_failures(vec![bot("A 1"), bot("B 2")]), InnerTubeError::BotCheck));
        // One client giving a different answer means it is not (only) the bot check.
        let age = ("C 3".to_owned(), InnerTubeError::Refused("LOGIN_REQUIRED: Sign in to confirm your age".to_owned()));
        assert!(matches!(summarize_failures(vec![bot("A 1"), age]), InnerTubeError::Api(_)));
        assert!(summarize_failures(vec![bot("A 1")]).to_string().contains("network address"));
    }

    #[test]
    fn finds_visitor_data() {
        assert_eq!(find_visitor_data(r#")]}' [["x","visitorData":"CgtAbc-_123%3D"]]"#).as_deref(), Some("CgtAbc-_123%3D"));
        assert_eq!(find_visitor_data(r#"{"VISITOR_DATA" : "Cgt9"}"#).as_deref(), Some("Cgt9"));
        assert_eq!(find_visitor_data("nothing here"), None);
        assert_eq!(find_visitor_data(r#""visitorData":"bad value""#), None);
    }

    #[test]
    fn finds_a_bare_visitor_id_in_a_nested_list() {
        let page = r#")]}'
[[["x",null,[["a",[1,2],"Cgt4S1ZsT2Z3d0JSOCjV%3D%3D",3]],"b"]]]"#;
        assert_eq!(find_visitor_data(page).as_deref(), Some("Cgt4S1ZsT2Z3d0JSOCjV%3D%3D"));
        assert_eq!(find_visitor_data(r#"["Cgtshort"]"#), None);
    }

    #[test]
    fn request_body_carries_client_and_video() {
        let profiles = clients::builtin();
        let body = request_body(&profiles[0], "abc123_XYZ0", Some("Cgt"));
        assert_eq!(body["videoId"], "abc123_XYZ0");
        assert_eq!(body["context"]["client"]["clientName"], "ANDROID_VR");
        assert_eq!(body["context"]["client"]["osName"], "Android");
        assert_eq!(body["context"]["client"]["visitorData"], "Cgt");
        assert_eq!(body["context"]["user"]["lockedSafetyMode"], false);
        assert_eq!(request_body(&profiles[1], "abc123_XYZ0", None)["context"]["client"].get("visitorData"), None);
    }
}
