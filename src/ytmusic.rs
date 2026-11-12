use crate::models::Track;

pub use twe_innertube::ids::{
    best_thumbnail, is_allowed_thumbnail_url, stable_id, upscale_thumbnail, youtube_music_url,
};

/// Fetches public search metadata only. This relies on an undocumented endpoint,
/// so it may stop working when YouTube changes its client protocol.
pub fn search_tracks(query: &str) -> Result<Vec<Track>, String> {
    twe_innertube::search::search(query)
        .map(|tracks| tracks.into_iter().map(Track::from).collect())
        .map_err(|error| error.to_string())
}

/// Songs similar to `video_id` (YouTube Music's radio), for the "Up next" queue.
pub fn related_tracks(video_id: &str) -> Result<Vec<Track>, String> {
    twe_innertube::related::up_next(video_id)
        .map(|tracks| tracks.into_iter().map(Track::from).collect())
        .map_err(|error| error.to_string())
}
