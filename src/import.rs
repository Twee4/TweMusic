//! Imports a public YouTube / YouTube Music playlist by link, through InnerTube's browse
//! endpoint (metadata only; nothing is downloaded).

use crate::models::Track;

pub fn is_supported_playlist_url(url: &str) -> bool {
    twe_innertube::browse::playlist_id_from_url(url.trim()).is_some()
}

/// Looks up the playlist and returns its title and tracks. Blocks on the network.
pub fn import_playlist(url: &str) -> Result<(String, Vec<Track>), String> {
    let id = twe_innertube::browse::playlist_id_from_url(url.trim()).ok_or_else(|| {
        "Paste a public playlist link from music.youtube.com or youtube.com (it must contain list=).".to_owned()
    })?;
    let playlist = twe_innertube::browse::playlist(&id).map_err(|error| error.to_string())?;
    let tracks: Vec<Track> = playlist.tracks.into_iter().map(Track::from).collect();
    Ok((playlist.title, tracks))
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
}
