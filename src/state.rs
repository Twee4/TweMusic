use crate::models::{Screen, Track, TrackId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub screen: Screen,
    pub search_query: String,
    pub tracks: Vec<Track>,
    pub remote_results: Vec<Track>,
    pub search_message: Option<String>,
    pub queue: Vec<TrackId>,
    pub current_track: Option<TrackId>,
    pub playback: PlaybackStatus,
}

impl AppState {
    pub fn new() -> Self {
        let tracks = vec![
            Track::new(1, "Midnight Echo", "Nova Lane", Some("After Hours"), 222),
            Track::new(2, "Electric Bloom", "Iris City", Some("Glass Gardens"), 255),
            Track::new(3, "Silver Skyline", "Red Harbor", Some("Northern Lights"), 178),
            Track::new(4, "Night Shift", "Luna Kestrel", Some("After Hours"), 307),
            Track::new(5, "Golden Thread", "Northline", Some("Small Wonders"), 206),
            Track::new(6, "Moss and Neon", "Velvet Echo", Some("Soft Focus"), 282),
        ];
        let queue = tracks.iter().map(|track| track.id).collect();
        Self {
            screen: Screen::Home,
            search_query: String::new(),
            tracks,
            remote_results: Vec::new(),
            search_message: None,
            queue,
            current_track: None,
            playback: PlaybackStatus::Stopped,
        }
    }

    pub fn search_results(&self) -> Vec<&Track> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() { return Vec::new(); }
        self.tracks.iter().filter(|track| {
            track.title.to_lowercase().contains(&query)
                || track.artist.to_lowercase().contains(&query)
                || track.album.as_deref().unwrap_or_default().to_lowercase().contains(&query)
        }).collect()
    }

    pub fn current_track(&self) -> Option<&Track> {
        let id = self.current_track?;
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn add_search_results(&mut self, tracks: Vec<Track>) {
        self.remote_results = tracks;
        self.search_message = None;
        for track in &self.remote_results {
            if !self.tracks.iter().any(|existing| existing.id == track.id) {
                self.tracks.push(track.clone());
                self.queue.push(track.id);
            }
        }
    }

    pub fn play_track(&mut self, id: TrackId) {
        if self.tracks.iter().any(|track| track.id == id) {
            self.current_track = Some(id);
            self.playback = PlaybackStatus::Playing;
            if !self.queue.contains(&id) { self.queue.push(id); }
        }
    }

    pub fn toggle_playback(&mut self) {
        if self.current_track.is_none() {
            if let Some(first) = self.queue.first().copied() { self.play_track(first); }
            return;
        }
        self.playback = match self.playback {
            PlaybackStatus::Playing => PlaybackStatus::Paused,
            PlaybackStatus::Paused | PlaybackStatus::Stopped => PlaybackStatus::Playing,
        };
    }

    /// The nearest queued YouTube track before or after the current one, skipping demo tracks.
    pub fn adjacent_remote(&self, forward: bool) -> Option<TrackId> {
        let index = self.current_track.and_then(|id| self.queue.iter().position(|queued| *queued == id))?;
        let is_remote = |id: &TrackId| self.tracks.iter().any(|track| track.id == *id && track.video_id.is_some());
        if forward {
            self.queue.iter().skip(index + 1).find(|id| is_remote(*id)).copied()
        } else {
            self.queue[..index].iter().rev().find(|id| is_remote(*id)).copied()
        }
    }

    pub fn next_track(&mut self) {
        let next = match self.current_track.and_then(|id| self.queue.iter().position(|queued| *queued == id)) {
            Some(index) if index + 1 < self.queue.len() => self.queue[index + 1],
            _ => match self.queue.first() { Some(id) => *id, None => return },
        };
        self.play_track(next);
    }

    pub fn previous_track(&mut self) {
        let previous = match self.current_track.and_then(|id| self.queue.iter().position(|queued| *queued == id)) {
            Some(index) if index > 0 => self.queue[index - 1],
            _ => match self.queue.first() { Some(id) => *id, None => return },
        };
        self.play_track(previous);
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_title_artist_and_album_case_insensitively() {
        let mut state = AppState::new();
        for query in ["MIDNIGHT", "iris city", "soft focus"] {
            state.search_query = query.to_owned();
            assert_eq!(state.search_results().len(), 1, "query: {query}");
        }
    }

    #[test]
    fn playback_selects_track_and_advances_queue() {
        let mut state = AppState::new();
        state.play_track(TrackId(1));
        assert_eq!(state.playback, PlaybackStatus::Playing);
        state.next_track();
        assert_eq!(state.current_track, Some(TrackId(2)));
        state.previous_track();
        assert_eq!(state.current_track, Some(TrackId(1)));
    }

    #[test]
    fn playback_toggle_pauses_and_resumes() {
        let mut state = AppState::new();
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Playing);
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Paused);
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Playing);
    }

    #[test]
    fn adjacent_remote_skips_demo_tracks() {
        let mut state = AppState::new();
        let mut first = Track::new(100, "A", "X", None, 10);
        first.video_id = Some("aaaaaaaaaaa".to_owned());
        let mut second = Track::new(200, "B", "Y", None, 10);
        second.video_id = Some("bbbbbbbbbbb".to_owned());
        state.add_search_results(vec![first, second]);

        state.play_track(TrackId(100));
        assert_eq!(state.adjacent_remote(true), Some(TrackId(200)));
        assert_eq!(state.adjacent_remote(false), None);
        state.play_track(TrackId(200));
        assert_eq!(state.adjacent_remote(false), Some(TrackId(100)));
        assert_eq!(state.adjacent_remote(true), None);
    }
}
