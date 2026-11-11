//! Small pure helpers for YouTube identifiers and image URLs.

use serde_json::Value;

/// Build the official YouTube Music watch URL for a parsed video ID.
/// Only standard 11-character YouTube IDs are accepted.
pub fn youtube_music_url(video_id: &str) -> Option<String> {
    let valid = video_id.len() == 11
        && video_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    valid.then(|| format!("https://music.youtube.com/watch?v={video_id}"))
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
pub fn upscale_thumbnail(url: &str) -> String {
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
pub fn best_thumbnail(thumbnails: &Value) -> Option<String> {
    thumbnails
        .as_array()?
        .iter()
        .rev()
        .filter_map(|item| item.get("url").and_then(Value::as_str))
        .find(|url| is_allowed_thumbnail_url(url))
        .map(upscale_thumbnail)
}

pub fn stable_id(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
