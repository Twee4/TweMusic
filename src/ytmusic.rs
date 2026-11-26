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

const GENRES: [&str; 48] = [
    "pop", "rock", "indie", "hip hop", "r&b", "soul", "jazz", "blues", "country", "folk", "reggae",
    "dancehall", "afrobeats", "latin", "reggaeton", "salsa", "bossa nova", "electronic", "house",
    "techno", "lofi", "ambient", "classical", "opera", "metal", "punk", "disco", "funk", "gospel",
    "k-pop", "j-pop", "city pop", "mandopop", "bollywood", "punjabi", "arabic pop", "turkish pop",
    "dangdut", "lagu melayu", "lagu indonesia", "acoustic", "soundtrack", "synthwave", "shoegaze",
    "trap", "emo", "worship", "traditional",
];

/// Songs picked from all over, not tied to anything played: a random genre or language, often with
/// a random year, searched, then mixed up. There is no list of "every song" to draw from, so wide
/// and varied searches stand in for it.
pub fn random_tracks() -> Result<Vec<Track>, String> {
    use crate::state::{random_below, shuffle_slice};
    let genre = GENRES[random_below(GENRES.len())];
    let year = 1965 + random_below(61);
    let query = match random_below(3) {
        0 => format!("{genre} songs"),
        1 => format!("{genre} {year}"),
        _ => format!("best {genre} songs of {year}"),
    };
    let mut tracks = search_tracks(&query)?;
    shuffle_slice(&mut tracks);
    tracks.truncate(8);
    Ok(tracks)
}
