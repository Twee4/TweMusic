//! Plain data returned by the InnerTube calls.

/// One track as listed by search or a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: u64,
    /// Only ever a https URL on YouTube's own image hosts.
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub title: String,
    pub tracks: Vec<TrackInfo>,
}
