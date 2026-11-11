//! Public playlist import through InnerTube's `browse` endpoint.

use std::collections::HashSet;

use serde_json::json;

use crate::{
    clients,
    models::{Playlist, TrackInfo},
    parse, transport, InnerTubeError,
};

const MAX_TRACKS: usize = 1000;
const MAX_PAGES: usize = 30;

/// True for a plausible playlist ID (letters, digits, `_` and `-`).
pub fn is_valid_playlist_id(id: &str) -> bool {
    (2..=64).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The `list=` playlist ID of a music.youtube.com / youtube.com link.
pub fn playlist_id_from_url(url: &str) -> Option<String> {
    let rest = ["https://music.youtube.com/", "https://www.youtube.com/", "https://youtube.com/"]
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let query = rest.split_once('?')?.1.split('#').next()?;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("list="))
        .filter(|id| is_valid_playlist_id(id))
        .map(str::to_owned)
}

/// Loads a public playlist (title and up to 1000 tracks), following continuation pages.
/// Blocks on the network, so call it from a worker thread.
pub fn playlist(playlist_id: &str) -> Result<Playlist, InnerTubeError> {
    if !is_valid_playlist_id(playlist_id) {
        return Err(InnerTubeError::Api("That is not a valid playlist ID.".to_owned()));
    }
    let browse_id = if playlist_id.starts_with("VL") { playlist_id.to_owned() } else { format!("VL{playlist_id}") };
    let profiles = clients::load();
    let usable = clients::eligible(&profiles, "browse");
    let client = *usable
        .first()
        .ok_or_else(|| InnerTubeError::Api("No client profile in clients.json is meant for browse.".to_owned()))?;
    let http = transport::http_client()?;
    let context = clients::build_context(client, None);

    let first = transport::post(&http, client, "browse", &json!({ "context": context, "browseId": browse_id }), None)?;
    let title = parse::playlist_title(&first).unwrap_or_else(|| "Imported playlist".to_owned());
    let mut seen = HashSet::new();
    let mut tracks: Vec<TrackInfo> = parse::collect_playlist_tracks(&first, &mut seen);
    let mut token = parse::continuation_token(&first);

    for _ in 0..MAX_PAGES {
        let Some(current) = token.take() else { break; };
        if tracks.len() >= MAX_TRACKS { break; }
        let page = transport::post(&http, client, "browse", &json!({ "context": context, "continuation": current }), None)?;
        let more = parse::collect_playlist_tracks(&page, &mut seen);
        let next = parse::continuation_token(&page);
        // A page with no new tracks ends the walk, so a repeated token can't loop forever.
        if more.is_empty() { break; }
        tracks.extend(more);
        token = next;
    }
    tracks.truncate(MAX_TRACKS);
    if tracks.is_empty() {
        return Err(InnerTubeError::Api("No playable tracks found in that playlist (is it public?).".to_owned()));
    }
    Ok(Playlist { title, tracks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_playlist_ids_from_youtube_links_only() {
        assert_eq!(playlist_id_from_url("https://music.youtube.com/playlist?list=PLabc_-1").as_deref(), Some("PLabc_-1"));
        assert_eq!(
            playlist_id_from_url("https://www.youtube.com/watch?v=abc123_XYZ0&list=PLabc&index=2#t").as_deref(),
            Some("PLabc")
        );
        assert_eq!(playlist_id_from_url("https://music.youtube.com/watch?v=abc123_XYZ0"), None);
        assert_eq!(playlist_id_from_url("https://evil.example/playlist?list=PLabc"), None);
        assert_eq!(playlist_id_from_url("https://music.youtube.com/playlist?list=a b"), None);
        assert_eq!(playlist_id_from_url("https://music.youtube.com/playlist?list="), None);
        assert_eq!(playlist_id_from_url("--exec"), None);
    }

    #[test]
    fn validates_playlist_ids() {
        assert!(is_valid_playlist_id("PLabc_-123"));
        assert!(!is_valid_playlist_id("x"));
        assert!(!is_valid_playlist_id("a/b"));
        assert!(!is_valid_playlist_id(&"a".repeat(65)));
    }
}
