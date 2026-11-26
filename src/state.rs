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
    /// Shuffle is on: the songs after the current one are in random order.
    pub shuffle: bool,
    /// The queue's order before shuffling, so turning shuffle off can put it back.
    unshuffled: Vec<TrackId>,
    pub current_track: Option<TrackId>,
    pub playback: PlaybackStatus,
}

impl AppState {
    pub fn new() -> Self {
        let tracks: Vec<Track> = Vec::new();
        let queue = tracks.iter().map(|track| track.id).collect();
        Self {
            screen: Screen::Home,
            search_query: String::new(),
            tracks,
            remote_results: Vec::new(),
            search_message: None,
            queue,
            shuffle: false,
            unshuffled: Vec::new(),
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

    /// Replaces the queue with these tracks (adding unknown ones to the track list) and
    /// returns the first one, if any.
    pub fn set_queue(&mut self, tracks: Vec<Track>) -> Option<TrackId> {
        let mut queue = Vec::with_capacity(tracks.len());
        for track in tracks {
            queue.push(track.id);
            if !self.tracks.iter().any(|existing| existing.id == track.id) {
                self.tracks.push(track);
            }
        }
        self.queue = queue;
        if self.shuffle {
            // Shuffle plays the song picked and then random songs from everywhere: the rest of
            // the list it came from is not kept, but it is remembered for when shuffle goes off.
            self.unshuffled = self.queue.clone();
            self.queue.truncate(1);
        }
        self.queue.first().copied()
    }

    /// Turns shuffle on or off. On: the songs waiting after the current one are set aside and
    /// random songs take their place. Off: the original order comes back (songs added meanwhile
    /// go at the end).
    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            self.unshuffled = self.queue.clone();
            let keep = self
                .current_track
                .and_then(|id| self.queue.iter().position(|queued| *queued == id))
                .map_or(0, |index| index + 1);
            self.queue.truncate(keep);
        } else {
            let mut restored = std::mem::take(&mut self.unshuffled);
            for id in &self.queue {
                if !restored.contains(id) {
                    restored.push(*id);
                }
            }
            self.queue = restored;
        }
    }

    /// Adds tracks at random places after the current song, skipping ones already queued, so new
    /// songs are mixed in with the ones waiting. Returns how many were added.
    pub fn insert_shuffled(&mut self, tracks: Vec<Track>) -> usize {
        let mut added = 0;
        for track in tracks {
            if self.queue.contains(&track.id) {
                continue;
            }
            let after = self
                .current_track
                .and_then(|id| self.queue.iter().position(|queued| *queued == id))
                .map_or(0, |index| index + 1);
            let at = after + random_below(self.queue.len() - after + 1);
            self.queue.insert(at, track.id);
            self.unshuffled.push(track.id);
            if !self.tracks.iter().any(|existing| existing.id == track.id) {
                self.tracks.push(track);
            }
            added += 1;
        }
        added
    }

    /// Adds tracks to the end of the queue, skipping ones already queued. Returns how many
    /// were added.
    pub fn append_to_queue(&mut self, tracks: Vec<Track>) -> usize {
        let mut added = 0;
        for track in tracks {
            if self.queue.contains(&track.id) {
                continue;
            }
            self.queue.push(track.id);
            if self.shuffle {
                self.unshuffled.push(track.id);
            }
            if !self.tracks.iter().any(|existing| existing.id == track.id) {
                self.tracks.push(track);
            }
            added += 1;
        }
        added
    }

    /// The YouTube tracks queued after the current one (at most `limit`), for the Up next list.
    pub fn up_next(&self, limit: usize) -> Vec<Track> {
        let Some(index) = self.current_track.and_then(|id| self.queue.iter().position(|queued| *queued == id)) else {
            return Vec::new();
        };
        self.queue
            .iter()
            .skip(index + 1)
            .filter_map(|id| self.tracks.iter().find(|track| track.id == *id && track.video_id.is_some()))
            .take(limit)
            .cloned()
            .collect()
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

/// A random number below `bound` from a small xorshift generator seeded from the clock
/// (plenty for shuffling music).
pub fn random_below(bound: usize) -> usize {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0);
    let mut seed = SEED.load(Ordering::Relaxed);
    if seed == 0 {
        seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |elapsed| elapsed.as_nanos() as u64)
            | 1;
    }
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    SEED.store(seed, Ordering::Relaxed);
    // The high bits are the better ones.
    ((seed >> 11) % bound.max(1) as u64) as usize
}

/// Fisher-Yates.
pub fn shuffle_slice<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, random_below(i + 1));
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: u64, title: &str, artist: &str, album: &str) -> Track {
        let mut track = Track::new(id, title, artist, Some(album), 100);
        track.video_id = Some(format!("{id:011}"));
        track
    }

    /// A state holding three queued songs.
    fn state_with_songs() -> AppState {
        let mut state = AppState::new();
        state.add_search_results(vec![
            song(1, "Midnight Echo", "Nova Lane", "After Hours"),
            song(2, "Electric Bloom", "Iris City", "Glass Gardens"),
            song(3, "Silver Skyline", "Red Harbor", "Soft Focus"),
        ]);
        state
    }

    #[test]
    fn starts_empty() {
        let state = AppState::new();
        assert!(state.tracks.is_empty() && state.queue.is_empty());
    }

    #[test]
    fn search_matches_title_artist_and_album_case_insensitively() {
        let mut state = state_with_songs();
        for query in ["MIDNIGHT", "iris city", "soft focus"] {
            state.search_query = query.to_owned();
            assert_eq!(state.search_results().len(), 1, "query: {query}");
        }
    }

    #[test]
    fn playback_selects_track_and_advances_queue() {
        let mut state = state_with_songs();
        state.play_track(TrackId(1));
        assert_eq!(state.playback, PlaybackStatus::Playing);
        state.next_track();
        assert_eq!(state.current_track, Some(TrackId(2)));
        state.previous_track();
        assert_eq!(state.current_track, Some(TrackId(1)));
    }

    #[test]
    fn playback_toggle_pauses_and_resumes() {
        let mut state = state_with_songs();
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Playing);
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Paused);
        state.toggle_playback();
        assert_eq!(state.playback, PlaybackStatus::Playing);
    }

    #[test]
    fn set_queue_replaces_queue_and_returns_first() {
        let mut state = state_with_songs();
        assert_eq!(state.set_queue(Vec::new()), None);
        let mut track = Track::new(300, "C", "Z", None, 10);
        track.video_id = Some("ccccccccccc".to_owned());
        assert_eq!(state.set_queue(vec![track]), Some(TrackId(300)));
        assert_eq!(state.queue, vec![TrackId(300)]);
        assert!(state.tracks.iter().any(|existing| existing.id == TrackId(300)));
    }

    #[test]
    fn append_to_queue_skips_duplicates_and_feeds_up_next() {
        let mut state = AppState::new();
        let remote = |id: u64, video: &str| {
            let mut track = Track::new(id, "T", "A", None, 10);
            track.video_id = Some(video.to_owned());
            track
        };
        state.set_queue(vec![remote(100, "aaaaaaaaaaa")]);
        state.play_track(TrackId(100));
        assert!(state.up_next(10).is_empty());
        assert_eq!(state.append_to_queue(vec![remote(200, "bbbbbbbbbbb"), remote(100, "aaaaaaaaaaa"), remote(300, "ccccccccccc")]), 2);
        assert_eq!(state.adjacent_remote(true), Some(TrackId(200)));
        let upcoming: Vec<u64> = state.up_next(10).iter().map(|track| track.id.0).collect();
        assert_eq!(upcoming, vec![200, 300]);
        assert_eq!(state.up_next(1).len(), 1);
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

    #[test]
    fn shuffle_keeps_only_the_current_song_and_restores_the_order() {
        let mut state = state_with_songs();
        state.play_track(TrackId(1));
        let original = state.queue.clone();
        state.set_shuffle(true);
        assert_eq!(state.queue, vec![TrackId(1)]);
        state.set_shuffle(false);
        assert_eq!(state.queue, original);
    }
}
