//! The app's brain without any UI: library, queue, search, today's pick and playback through mpv.
//! A front end (the Slint window) owns one `Engine`, calls `poll` and `tick` on a timer, and reads
//! the public fields to draw itself. Worker threads send their results back over channels, so
//! nothing here blocks the UI.

use std::{
    collections::HashSet,
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use twe_innertube::Stream;

use crate::{
    library::{self, Library, SavedTrack},
    models::{Track, TrackId},
    mpv::{MpvError, MpvPlayer},
    settings::{self, Settings},
    state::{AppState, PlaybackStatus},
    ytdlp, ytmusic,
};

/// How long to wait before looking for today's pick again after a failed attempt.
const PICK_RETRY: Duration = Duration::from_secs(120);
/// How often mpv is asked for its position.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// The song chosen for today, why, and similar songs fetched from search.
#[derive(Clone)]
pub struct DailyPick {
    pub day: u32,
    pub why: String,
    pub track: Track,
    pub more: Vec<Track>,
}

pub struct Engine {
    pub state: AppState,
    pub library: Library,
    pub settings: Settings,
    library_path: Option<PathBuf>,
    mpv: Option<MpvPlayer>,

    /// Transient status for the player ("Loading…", "Paused").
    pub message: Option<String>,
    pub position: Option<f64>,
    pub duration: Option<f64>,
    notices: Vec<String>,

    stream_rx: Option<Receiver<Result<Stream, String>>>,
    pending_track: Option<Track>,
    /// The song whose direct stream would not play, so it was retried through yt-dlp once.
    direct_failed: Option<TrackId>,
    related_rx: Option<Receiver<Result<Vec<Track>, String>>>,
    related_for: Option<TrackId>,
    played_something: bool,
    loading_since: Option<Instant>,
    last_poll: Instant,
    listen_carry: f32,

    search_rx: Option<Receiver<Result<Vec<Track>, String>>>,
    pub searching: bool,

    pub today: u32,
    pub hour: u32,
    clock_read: Option<Instant>,
    pub pick: Option<DailyPick>,
    pick_rx: Option<Receiver<Result<DailyPick, String>>>,
    pick_failed: Option<Instant>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let library_path = library::default_path();
        let library = library_path.as_deref().map(Library::load_from).unwrap_or_else(Library::new);
        let settings = settings::default_path().as_deref().map(Settings::load_from).unwrap_or_default();
        let mut engine = Self {
            state: AppState::new(),
            library,
            settings,
            library_path,
            mpv: None,
            message: None,
            position: None,
            duration: None,
            notices: Vec::new(),
            stream_rx: None,
            pending_track: None,
            direct_failed: None,
            related_rx: None,
            related_for: None,
            played_something: false,
            loading_since: None,
            last_poll: Instant::now(),
            listen_carry: 0.0,
            search_rx: None,
            searching: false,
            today: 0,
            hour: 12,
            clock_read: None,
            pick: None,
            pick_rx: None,
            pick_failed: None,
        };
        engine.read_clock();
        engine
    }

    // ---- Messages ------------------------------------------------------------------

    /// Messages for the user (errors, confirmations) since the last call.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.notices.push(message.into());
    }

    fn save_library(&mut self) {
        if let Some(path) = &self.library_path {
            if let Err(error) = self.library.save_to(path) {
                self.notices.push(format!("Could not save your library: {error}"));
            }
        }
    }

    // ---- What the UI reads -----------------------------------------------------------

    pub fn liked_tracks(&self) -> Vec<Track> {
        self.library.playlists.first().map(|playlist| playlist.tracks.iter().map(SavedTrack::to_track).collect()).unwrap_or_default()
    }

    pub fn history_tracks(&self) -> Vec<Track> {
        self.library.history.iter().map(SavedTrack::to_track).collect()
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.state.current_track()
    }

    pub fn is_playing(&self) -> bool {
        self.state.playback == PlaybackStatus::Playing
    }

    pub fn is_loading(&self) -> bool {
        self.stream_rx.is_some() || self.loading_since.is_some()
    }

    pub fn current_is_liked(&self) -> bool {
        self.state.current_track().is_some_and(|track| self.library.is_liked(track))
    }

    // ---- Playback ---------------------------------------------------------------------

    fn ensure_player(&mut self) -> Result<&mut MpvPlayer, MpvError> {
        if self.mpv.is_none() {
            self.mpv = Some(MpvPlayer::start()?);
        }
        Ok(self.mpv.as_mut().expect("player was just started"))
    }

    /// What yt-dlp should use when the direct lookup is blocked: no sign-in unless a browser is
    /// set by hand in settings.json.
    fn ytdlp_browser(&self) -> String {
        self.settings
            .ytdlp_browser
            .clone()
            .filter(|name| ytdlp::is_supported_browser(name))
            .unwrap_or_else(|| ytdlp::NO_SIGN_IN.to_owned())
    }

    /// Plays `tracks` as the queue, starting with the one at `index`.
    pub fn play_from(&mut self, mut tracks: Vec<Track>, index: usize) {
        if index >= tracks.len() {
            return;
        }
        tracks.rotate_left(index);
        let first = tracks[0].clone();
        self.state.set_queue(tracks);
        self.play_remote(&first);
    }

    /// Resolves a track's stream on a worker thread; `poll` then hands it to mpv.
    pub fn play_remote(&mut self, track: &Track) {
        let Some(video_id) = track.video_id.clone().filter(|id| ytmusic::youtube_music_url(id).is_some()) else {
            self.notify("Invalid video ID.");
            return;
        };
        let browser = self.ytdlp_browser();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(ytdlp::resolve(&video_id, Some(browser.as_str())));
        });
        // Replacing the receiver drops any older request; its result is simply ignored.
        self.stream_rx = Some(rx);
        self.pending_track = Some(track.clone());
        self.message = Some(format!("Loading {}…", track.title));
    }

    fn start_stream(&mut self, track: &Track, stream: &Stream) {
        let result = self.ensure_player().and_then(|player| player.load_stream(stream));
        match result {
            Ok(()) => {
                if !self.state.tracks.iter().any(|existing| existing.id == track.id) {
                    self.state.tracks.push(track.clone());
                }
                self.state.play_track(track.id);
                self.library.record_play(track);
                self.save_library();
                if self.state.adjacent_remote(true).is_none() {
                    self.start_related(track);
                }
                self.position = None;
                self.duration = None;
                self.played_something = false;
                self.loading_since = Some(Instant::now());
                self.message = Some(format!("Loading {}…", track.title));
            }
            Err(error) => {
                self.message = None;
                self.notify(error.to_string());
            }
        }
    }

    /// Fetches songs similar to `track` and queues them after it.
    fn start_related(&mut self, track: &Track) {
        let Some(video_id) = track.video_id.clone() else { return };
        if self.related_for == Some(track.id) {
            return;
        }
        self.related_for = Some(track.id);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(ytmusic::related_tracks(&video_id));
        });
        self.related_rx = Some(rx);
    }

    pub fn skip(&mut self, forward: bool) {
        let Some(id) = self.state.adjacent_remote(forward) else { return };
        if let Some(track) = self.state.tracks.iter().find(|track| track.id == id).cloned() {
            self.play_remote(&track);
        }
    }

    pub fn toggle_pause(&mut self) {
        let Some(player) = &mut self.mpv else {
            self.notify("Pick a song to start playback.");
            return;
        };
        match player.toggle_pause() {
            Ok(paused) => {
                self.state.playback = if paused { PlaybackStatus::Paused } else { PlaybackStatus::Playing };
                self.message = Some(if paused { "Paused" } else { "Playing" }.to_owned());
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    pub fn seek_to(&mut self, seconds: f64) {
        let Some(player) = self.mpv.as_mut() else { return };
        match player.seek_to(seconds) {
            Ok(()) => self.position = Some(seconds),
            Err(error) => self.notify(error.to_string()),
        }
    }

    /// Likes or unlikes the current song.
    pub fn toggle_like_current(&mut self) {
        let Some(track) = self.state.current_track().cloned() else { return };
        let liked = self.library.toggle_like(&track);
        self.save_library();
        self.notify(if liked { "Added to Liked songs." } else { "Removed from Liked songs." });
    }

    // ---- Search -----------------------------------------------------------------------

    pub fn start_search(&mut self, query: &str) {
        let query = query.trim().to_owned();
        if query.is_empty() || self.searching {
            return;
        }
        self.state.search_query = query.clone();
        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);
        self.searching = true;
        self.state.search_message = Some("Searching…".to_owned());
        thread::spawn(move || {
            let _ = tx.send(ytmusic::search_tracks(&query));
        });
    }

    // ---- Today's pick -------------------------------------------------------------------

    fn read_clock(&mut self) {
        self.hour = local_hour();
        self.today = local_date();
        self.clock_read = Some(Instant::now());
    }

    /// Looks for today's pick when there is none for today yet (and not just after a failure).
    pub fn ensure_pick(&mut self) {
        if self.clock_read.map_or(true, |read| read.elapsed() > Duration::from_secs(60)) {
            self.read_clock();
        }
        let day = self.today;
        let current = self.pick.as_ref().is_some_and(|pick| pick.day == day);
        let retry_later = self.pick_failed.is_some_and(|failed| failed.elapsed() < PICK_RETRY);
        if current || retry_later || self.pick_rx.is_some() {
            return;
        }
        let liked: Vec<SavedTrack> = self.library.playlists.first().map(|playlist| playlist.tracks.clone()).unwrap_or_default();
        let (query, why) = pick_plan(&self.library.history, &liked, day, self.hour);
        // Skip what was just heard, so the pick is something new.
        let skip: HashSet<String> = self.library.history.iter().take(10).map(|saved| saved.video_id.clone()).collect();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(fetch_pick(&query, why, day, &skip));
        });
        self.pick_rx = Some(rx);
    }

    // ---- Timer work ---------------------------------------------------------------------

    /// Collects finished worker results (stream, related songs, search, pick). Cheap; call often.
    pub fn poll(&mut self) {
        self.poll_stream();
        self.poll_related();
        self.poll_search();
        self.poll_pick();
    }

    fn poll_stream(&mut self) {
        let Some(rx) = &self.stream_rx else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Could not look up the stream.".to_owned()),
        };
        self.stream_rx = None;
        let Some(track) = self.pending_track.take() else { return };
        match result {
            Ok(stream) => self.start_stream(&track, &stream),
            Err(message) => {
                self.message = None;
                self.notify(message);
            }
        }
    }

    fn poll_related(&mut self) {
        let Some(rx) = &self.related_rx else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Ok(Vec::new()),
        };
        self.related_rx = None;
        // A failed lookup is not worth interrupting playback for; the queue just stays as it is.
        if let Ok(tracks) = result {
            self.state.append_to_queue(tracks);
        }
    }

    fn poll_search(&mut self) {
        let Some(rx) = &self.search_rx else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Search worker stopped unexpectedly.".to_owned()),
        };
        self.search_rx = None;
        self.searching = false;
        match result {
            Ok(tracks) => {
                let count = tracks.len();
                self.state.add_search_results(tracks);
                self.state.search_message = Some(if count == 0 { "No tracks found." } else { "Search complete." }.to_owned());
            }
            Err(error) => {
                self.state.remote_results.clear();
                self.state.search_message = Some(error);
            }
        }
    }

    fn poll_pick(&mut self) {
        let Some(rx) = &self.pick_rx else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The search stopped unexpectedly.".to_owned()),
        };
        self.pick_rx = None;
        match result {
            Ok(pick) => {
                self.pick = Some(pick);
                self.pick_failed = None;
            }
            Err(_) => self.pick_failed = Some(Instant::now()),
        }
    }

    /// Reads mpv's progress, notices the end of a track and moves on to the next one.
    pub fn tick(&mut self) {
        if self.state.playback != PlaybackStatus::Playing {
            self.last_poll = Instant::now();
            return;
        }
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return;
        }
        let elapsed = self.last_poll.elapsed();
        let Some(player) = self.mpv.as_mut() else { return };
        self.last_poll = Instant::now();
        let position = player.position();
        let duration = player.duration();
        let idle = player.is_idle();

        self.position = position;
        self.duration = duration;
        if position.is_some_and(|value| value > 0.0) {
            self.record_listening(elapsed);
            if !self.played_something {
                self.message = None;
            }
            self.played_something = true;
            self.loading_since = None;
        }
        if !idle {
            return;
        }
        if self.played_something {
            self.played_something = false;
            if self.state.adjacent_remote(true).is_some() {
                self.skip(true);
            } else {
                self.state.playback = PlaybackStatus::Stopped;
                self.position = None;
                self.duration = None;
            }
        } else if self.loading_since.is_some_and(|since| since.elapsed() > Duration::from_secs(2)) {
            self.loading_since = None;
            self.state.playback = PlaybackStatus::Stopped;
            self.retry_or_give_up();
        }
    }

    /// A fresh stream address is sometimes refused (403). Try once more through yt-dlp, which
    /// handles that, before giving up.
    fn retry_or_give_up(&mut self) {
        let current = self.state.current_track.and_then(|id| self.state.tracks.iter().find(|track| track.id == id).cloned());
        if let Some(track) = current {
            let video_id = track.video_id.clone().filter(|id| ytmusic::youtube_music_url(id).is_some());
            if let (Some(video_id), true) = (video_id, self.direct_failed != Some(track.id)) {
                self.direct_failed = Some(track.id);
                let browser = self.ytdlp_browser();
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let _ = tx.send(ytdlp::resolve_after_direct_failure(&video_id, &browser));
                });
                self.stream_rx = Some(rx);
                self.pending_track = Some(track.clone());
                self.message = Some(format!("Retrying {} another way…", track.title));
                return;
            }
        }
        let reason = self.mpv.as_ref().and_then(|player| player.last_problem());
        self.message = None;
        self.notify(match reason {
            Some(reason) => format!("Could not play this track. {reason}"),
            None => "Could not play this track, even through yt-dlp. Update yt-dlp (sudo pacman -Syu) and check that deno or node is installed.".to_owned(),
        });
    }

    fn record_listening(&mut self, elapsed: Duration) {
        self.listen_carry += elapsed.as_secs_f32().min(2.0);
        if self.listen_carry >= 30.0 {
            let seconds = self.listen_carry as u32;
            self.listen_carry -= seconds as f32;
            self.library.add_listening(unix_day(), seconds);
            self.save_library();
        }
    }
}

// ---- Today's pick: choosing and fetching ---------------------------------------------------

/// What to search for today's pick, and the reason shown to the user: one of the three artists
/// heard most (history and likes), rotating by day; with no listening yet, a query for the hour.
pub fn pick_plan(history: &[SavedTrack], liked: &[SavedTrack], day: u32, hour: u32) -> (String, String) {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for saved in history.iter().chain(liked.iter()) {
        let artist = saved.artist.split(',').next().unwrap_or("").trim();
        if artist.is_empty() {
            continue;
        }
        match counts.iter_mut().find(|(name, _)| name == artist) {
            Some((_, count)) => *count += 1,
            None => counts.push((artist.to_owned(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts.truncate(3);
    if !counts.is_empty() {
        let artist = counts[day as usize % counts.len()].0.clone();
        let why = format!("Because you listen to {artist}");
        return (format!("{artist} songs"), why);
    }
    match hour {
        5..=11 => ("morning acoustic songs".to_owned(), "For your morning".to_owned()),
        12..=17 => ("feel good pop hits".to_owned(), "For your afternoon".to_owned()),
        18..=22 => ("chill evening songs".to_owned(), "For your evening".to_owned()),
        _ => ("late night lofi songs".to_owned(), "For tonight".to_owned()),
    }
}

/// Searches for the pick and similar songs. Blocks on the network: call from a worker thread.
pub fn fetch_pick(query: &str, why: String, day: u32, skip: &HashSet<String>) -> Result<DailyPick, String> {
    let found: Vec<Track> = ytmusic::search_tracks(query)?.into_iter().filter(|track| track.video_id.is_some()).collect();
    let fresh: Vec<&Track> = found.iter().filter(|track| track.video_id.as_ref().is_some_and(|id| !skip.contains(id))).collect();
    let candidates: Vec<&Track> = if fresh.is_empty() { found.iter().collect() } else { fresh };
    if candidates.is_empty() {
        return Err("No songs were found for today's pick.".to_owned());
    }
    let track = candidates[day as usize % candidates.len().min(5)].clone();
    let mut more = ytmusic::related_tracks(track.video_id.as_deref().unwrap_or_default()).unwrap_or_default();
    more.extend(found.iter().cloned());
    let mut seen = HashSet::new();
    seen.insert(track.id);
    more.retain(|other| other.video_id.is_some() && seen.insert(other.id));
    more.truncate(15);
    Ok(DailyPick { day, why, track, more })
}

// ---- Clock -------------------------------------------------------------------------------

pub fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

fn date_command(format: &str) -> Option<u32> {
    Command::new("date")
        .arg(format)
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u32>().ok())
}

/// The local hour (0-23); falls back to the UTC hour when `date` is not available.
fn local_hour() -> u32 {
    date_command("+%H").filter(|hour| *hour < 24).unwrap_or_else(|| {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|since| ((since.as_secs() / 3600) % 24) as u32).unwrap_or(12)
    })
}

/// Today's local date as yyyymmdd (falls back to the UTC day count).
fn local_date() -> u32 {
    date_command("+%Y%m%d").filter(|date| *date > 19_000_101).unwrap_or_else(|| unix_day().max(1))
}

fn unix_day() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| (since.as_secs() / 86_400) as u32).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(artist: &str) -> SavedTrack {
        SavedTrack {
            video_id: "abc123_XYZ0".to_owned(),
            title: "Song".to_owned(),
            artist: artist.to_owned(),
            album: None,
            duration_secs: 180,
            thumbnail_url: None,
        }
    }

    #[test]
    fn pick_follows_the_most_played_artists() {
        let history = vec![saved("A"), saved("A"), saved("B"), saved("A, C")];
        let (query, why) = pick_plan(&history, &[], 0, 9);
        assert_eq!(query, "A songs");
        assert_eq!(why, "Because you listen to A");
    }

    #[test]
    fn pick_without_history_follows_the_hour() {
        assert_eq!(pick_plan(&[], &[], 5, 8).1, "For your morning");
        assert_eq!(pick_plan(&[], &[], 5, 2).1, "For tonight");
    }

    #[test]
    fn greeting_follows_the_hour() {
        assert_eq!(greeting(8), "Good morning");
        assert_eq!(greeting(14), "Good afternoon");
        assert_eq!(greeting(21), "Good evening");
    }
}
