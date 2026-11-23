//! The app's brain without any UI: library, queue, search, today's pick and playback through mpv.
//! A front end (the Slint window) owns one `Engine`, calls `poll` and `tick` on a timer, and reads
//! the public fields to draw itself. Worker threads send their results back over channels, so
//! nothing here blocks the UI.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use souvlaki::{MediaControlEvent, MediaPosition};
use twe_innertube::Stream;

use crate::{
    dynamic_color::{self, Transition},
    import,
    library::{self, Library, SavedTrack},
    media::Media,
    models::{Track, TrackId},
    mpv::{MpvError, MpvPlayer},
    lyrics::Lyrics,
    lyrics_net, notifications, prefetch,
    settings::{self, Settings},
    state::{AppState, PlaybackStatus},
    theme::ColorScheme,
    tray::{TrayCommand, TrayHandle},
    ytdlp, ytmusic,
};

/// How long to wait before looking for today's pick again after a failed attempt.
const PICK_RETRY: Duration = Duration::from_secs(120);
/// How long a looked-up stream address is reused (they last about six hours).
const CACHE_MAX_AGE: Duration = Duration::from_secs(4 * 60 * 60);
/// How many songs ahead of the playing one get their stream looked up in advance.
const LOOK_AHEAD: usize = 3;
/// How often mpv is asked for its position.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Things the engine asks the window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Bring the window back (tray icon clicked).
    ShowWindow,
    /// Quit the app (tray menu).
    Quit,
}

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
    settings_path: Option<PathBuf>,
    /// The playlist shown in the Library.
    pub selected_playlist: usize,
    import_rx: Option<Receiver<Result<(String, Vec<Track>), String>>>,
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

    // Gapless playback and stream refresh.
    stream_issued: Option<Instant>,
    refresh_rx: Option<(TrackId, Receiver<Result<Stream, String>>)>,
    refresh_backoff: Option<Instant>,
    resume_at: Option<f64>,
    prefetch_rx: Option<(TrackId, Receiver<Result<Stream, String>>)>,
    prefetch_tried: Option<TrackId>,
    queued_next: Option<(TrackId, Instant)>,
    /// Streams already looked up, with when they were issued: playing one of these is instant.
    stream_cache: HashMap<TrackId, (Stream, Instant)>,
    ahead_rx: Option<(TrackId, Receiver<Result<Stream, String>>)>,
    ahead_failed: HashSet<TrackId>,

    /// Lyrics of the current song, once found; `lyrics_for` is the song they were asked for.
    pub lyrics: Option<Lyrics>,
    pub lyrics_for: Option<TrackId>,
    pub lyrics_loading: bool,
    lyrics_rx: Option<Receiver<Result<Option<Lyrics>, String>>>,

    // Desktop integration.
    media: Option<Media>,
    media_rx: Receiver<MediaControlEvent>,
    media_synced: Option<(Option<TrackId>, PlaybackStatus)>,
    tray: Option<TrayHandle>,
    tray_tx: Sender<TrayCommand>,
    tray_rx: Receiver<TrayCommand>,
    tray_synced: Option<(Option<TrackId>, PlaybackStatus)>,
    actions: Vec<Action>,
    /// Set by the window: true while it is hidden to the tray, so notifications make sense.
    pub window_hidden: bool,
    announced: Option<TrackId>,

    // Dynamic colour from the album art.
    scheme: Transition,
    art_url: Option<String>,
    art_rx: Option<Receiver<(String, Option<f32>)>>,
    scheme_changed: bool,
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
        let settings_path = settings::default_path();
        let settings = settings_path.as_deref().map(Settings::load_from).unwrap_or_default();
        let (media_tx, media_rx) = mpsc::channel();
        let media = Media::new(media_tx, || {});
        let (tray_tx, tray_rx) = mpsc::channel();
        let mut engine = Self {
            state: AppState::new(),
            library,
            settings,
            library_path,
            settings_path,
            selected_playlist: 0,
            import_rx: None,
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
            stream_issued: None,
            refresh_rx: None,
            refresh_backoff: None,
            resume_at: None,
            prefetch_rx: None,
            prefetch_tried: None,
            queued_next: None,
            stream_cache: HashMap::new(),
            ahead_rx: None,
            ahead_failed: HashSet::new(),
            lyrics: None,
            lyrics_for: None,
            lyrics_loading: false,
            lyrics_rx: None,
            media,
            media_rx,
            media_synced: None,
            tray: None,
            tray_tx,
            tray_rx,
            tray_synced: None,
            actions: Vec::new(),
            window_hidden: false,
            announced: None,
            scheme: Transition::new(),
            art_url: None,
            art_rx: None,
            scheme_changed: false,
        };
        engine.read_clock();
        engine.apply_tray();
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

    /// The songs queued after the current one, for the Up next page.
    pub fn up_next_tracks(&self) -> Vec<Track> {
        self.state.up_next(40)
    }

    /// Jumps to a song in the Up next list; the rest of the queue stays as it is.
    pub fn play_up_next(&mut self, index: usize) {
        if let Some(track) = self.up_next_tracks().get(index).cloned() {
            self.state.play_track(track.id);
            self.play_remote(&track);
        }
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
        if let Some((stream, issued)) = self.cached_stream(track.id) {
            eprintln!("[timing] {video_id}: stream was looked up in advance");
            self.stream_rx = None;
            self.pending_track = None;
            self.start_stream(track, &stream);
            self.stream_issued = Some(issued);
            return;
        }
        // A look-up for this very song may already be running (ahead or for the queue): join it
        // instead of starting another.
        let running = if self.ahead_rx.as_ref().is_some_and(|(id, _)| *id == track.id) {
            self.ahead_rx.take().map(|(_, rx)| rx)
        } else if self.prefetch_rx.as_ref().is_some_and(|(id, _)| *id == track.id) {
            self.prefetch_rx.take().map(|(_, rx)| rx)
        } else {
            None
        };
        let rx = match running {
            Some(rx) => rx,
            None => {
                let browser = self.ytdlp_browser();
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let _ = tx.send(ytdlp::resolve(&video_id, Some(browser.as_str())));
                });
                rx
            }
        };
        // Replacing the receiver drops any older request; its result is simply ignored.
        self.stream_rx = Some(rx);
        self.pending_track = Some(track.clone());
        self.message = Some(format!("Loading {}…", track.title));
    }

    fn start_stream(&mut self, track: &Track, stream: &Stream) {
        let result = self.ensure_player().and_then(|player| player.load_stream(stream));
        eprintln!("[timing] handed {} to mpv: {}", track.title, if result.is_ok() { "ok" } else { "failed" });
        match result {
            Ok(()) => {
                if !self.state.tracks.iter().any(|existing| existing.id == track.id) {
                    self.state.tracks.push(track.clone());
                }
                self.state.play_track(track.id);
                self.library.record_play(track);
                self.save_library();
                self.stream_issued = Some(Instant::now());
                self.queued_next = None;
                self.prefetch_rx = None;
                self.prefetch_tried = None;
                self.refresh_rx = None;
                self.resume_at = None;
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
        // The next track is usually looked up and queued in mpv already: switch to it at once.
        if forward && self.state.playback == PlaybackStatus::Playing && self.queued_next.is_some_and(|(queued, _)| queued == id) {
            let jumped = self.mpv.as_mut().is_some_and(|player| player.advance_to_queued().is_ok());
            eprintln!("[timing] Next: jumped to the queued song: {jumped}");
            if jumped {
                self.finish_advance(id);
                return;
            }
        }
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

    // ---- Library ------------------------------------------------------------------------

    pub fn playlist_tracks(&self) -> Vec<Track> {
        self.library
            .playlists
            .get(self.selected_playlist)
            .map(|playlist| playlist.tracks.iter().map(SavedTrack::to_track).collect())
            .unwrap_or_default()
    }

    pub fn select_playlist(&mut self, index: usize) {
        if index < self.library.playlists.len() {
            self.selected_playlist = index;
        }
    }

    pub fn create_playlist(&mut self, name: &str) -> bool {
        match self.library.create(name) {
            Some(index) => {
                self.selected_playlist = index;
                self.save_library();
                true
            }
            None => {
                self.notify("Enter a playlist name that isn't already used.");
                false
            }
        }
    }

    /// Deletes the shown playlist (never "Liked songs").
    pub fn delete_selected_playlist(&mut self) {
        if self.library.delete(self.selected_playlist) {
            self.selected_playlist = 0;
            self.save_library();
        }
    }

    pub fn remove_from_selected(&mut self, index: usize) {
        let Some(saved) = self.library.playlists.get(self.selected_playlist).and_then(|playlist| playlist.tracks.get(index)) else { return };
        let video_id = saved.video_id.clone();
        self.library.remove(self.selected_playlist, &video_id);
        self.save_library();
    }

    /// Plays the shown playlist from its first song.
    pub fn play_selected_playlist(&mut self) {
        let tracks = self.playlist_tracks();
        if tracks.is_empty() {
            self.notify("That playlist is empty.");
        } else {
            self.play_from(tracks, 0);
        }
    }

    pub fn is_liked(&self, track: &Track) -> bool {
        self.library.is_liked(track)
    }

    /// Likes or unlikes any song (the heart on a search result).
    pub fn toggle_like(&mut self, track: &Track) {
        let liked = self.library.toggle_like(track);
        self.save_library();
        self.notify(if liked { "Added to Liked songs." } else { "Removed from Liked songs." });
    }

    pub fn start_import(&mut self, url: &str) {
        if self.import_rx.is_some() {
            return;
        }
        let url = url.trim().to_owned();
        if !import::is_supported_playlist_url(&url) {
            self.notify("Paste a public playlist link (it must contain list=).");
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.import_rx = Some(rx);
        self.notify("Importing playlist…");
        thread::spawn(move || {
            let _ = tx.send(import::import_playlist(&url));
        });
    }

    fn poll_import(&mut self) {
        let Some(rx) = &self.import_rx else { return };
        let outcome = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Import stopped unexpectedly.".to_owned()),
        };
        self.import_rx = None;
        match outcome {
            Ok((name, tracks)) => {
                let (index, added) = self.library.import(&name, &tracks);
                self.selected_playlist = index;
                self.save_library();
                let saved_name = self.library.playlists[index].name.clone();
                self.notify(format!("Imported {added} tracks into \"{saved_name}\"."));
            }
            Err(error) => self.notify(error),
        }
    }

    // ---- Settings ---------------------------------------------------------------------

    /// Changes a setting and saves the settings file.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings);
        if let Some(path) = &self.settings_path {
            if let Err(error) = self.settings.save_to(path) {
                self.notices.push(format!("Could not save settings: {error}"));
            }
        }
    }

    // ---- Search -----------------------------------------------------------------------

    /// Plays the search results as the queue, starting at `index`.
    pub fn play_result(&mut self, index: usize) {
        let results = self.state.remote_results.clone();
        self.play_from(results, index);
    }

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
        self.poll_import();
        self.poll_prefetch();
        self.poll_ahead();
        self.poll_lyrics();
        self.update_stream_refresh();
        self.poll_media();
        self.poll_tray();
    }

    /// Looks up lyrics whenever the current song changes, and collects them when they arrive.
    fn poll_lyrics(&mut self) {
        let current = self.state.current_track().map(|track| track.id);
        if current != self.lyrics_for {
            self.lyrics_for = current;
            self.lyrics = None;
            self.lyrics_rx = None;
            self.lyrics_loading = false;
            if let Some(track) = self.state.current_track().filter(|track| track.video_id.is_some()).cloned() {
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let seconds = track.duration.as_secs_f64();
                    let _ = tx.send(lyrics_net::fetch(&track.title, &track.artist, track.album.as_deref(), seconds));
                });
                self.lyrics_rx = Some(rx);
                self.lyrics_loading = true;
            }
        }
        let Some(rx) = &self.lyrics_rx else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.lyrics = result.ok().flatten();
                self.lyrics_rx = None;
                self.lyrics_loading = false;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.lyrics_rx = None;
                self.lyrics_loading = false;
            }
        }
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
            Ok(stream) => {
                self.stream_cache.insert(track.id, (stream.clone(), Instant::now()));
                self.start_stream(&track, &stream);
            }
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
            // Announce each track once, including one that followed gaplessly or was refreshed.
            let current = self.state.current_track().map(|track| track.id);
            if current != self.announced {
                self.announced = current;
                if self.settings.notifications && self.window_hidden {
                    if let Some(track) = self.state.current_track() {
                        notifications::show_track(track);
                    }
                }
            }
            // A refreshed stream starts from the beginning: go back to where it was.
            if position.is_some_and(|value| value < 3.0) {
                if let Some(at) = self.resume_at.take() {
                    if let Some(player) = self.mpv.as_mut() {
                        let _ = player.seek_to(at);
                    }
                }
            }
            if let Some(since) = self.loading_since {
                eprintln!("[timing] mpv opened the stream and started playing after {:.1}s", since.elapsed().as_secs_f32());
            }
            self.played_something = true;
            self.loading_since = None;
        }
        self.handle_gapless(position, duration);
        self.warm_ahead();
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
                self.stream_cache.remove(&track.id);
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


// ---- Gapless playback, stream refresh, desktop integration, colour ----------------------------

impl Engine {
    /// Resolves `track`'s stream on a worker thread, for prefetching or refreshing.
    fn resolve_in_background(&self, track: &Track) -> Option<Receiver<Result<Stream, String>>> {
        let video_id = track.video_id.clone().filter(|id| ytmusic::youtube_music_url(id).is_some())?;
        let browser = self.ytdlp_browser();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(ytdlp::resolve(&video_id, Some(browser.as_str())));
        });
        Some(rx)
    }

    /// Keeps playback gapless: shortly before a track ends, looks up the next one and queues
    /// it in mpv, and notices when mpv has moved on to it by itself.
    fn handle_gapless(&mut self, position: Option<f64>, duration: Option<f64>) {
        if self.stream_issued.is_none() {
            return;
        }
        let next = self.state.adjacent_remote(true);
        if let Some((queued, _)) = self.queued_next {
            if self.mpv.as_mut().and_then(|player| player.playlist_pos()) == Some(1) {
                self.finish_advance(queued);
            } else if next != Some(queued) {
                // The queue changed under the prefetched entry: drop it and look again.
                if let Some(player) = self.mpv.as_mut() {
                    player.clear_queued();
                }
                self.queued_next = None;
                self.prefetch_tried = None;
            }
            return;
        }
        if self.prefetch_rx.is_some() || !prefetch::should_prefetch(position, duration) {
            return;
        }
        let Some(next_id) = next else { return };
        if self.prefetch_tried == Some(next_id) {
            return;
        }
        let Some(track) = self.state.tracks.iter().find(|track| track.id == next_id).cloned() else { return };
        // Already being looked up ahead of time: wait for that one instead of asking twice.
        if self.ahead_rx.as_ref().is_some_and(|(id, _)| *id == next_id) {
            return;
        }
        self.prefetch_tried = Some(next_id);
        if let Some((stream, issued)) = self.cached_stream(next_id) {
            if let Some(player) = self.mpv.as_mut() {
                if player.queue_stream(&stream).is_ok() {
                    self.queued_next = Some((next_id, issued));
                }
            }
            return;
        }
        self.prefetch_rx = self.resolve_in_background(&track).map(|rx| (next_id, rx));
    }

    /// A stream looked up earlier that is still good, with when it was issued.
    fn cached_stream(&self, id: TrackId) -> Option<(Stream, Instant)> {
        self.stream_cache.get(&id).filter(|(_, issued)| issued.elapsed() < CACHE_MAX_AGE).cloned()
    }

    /// While a song plays, looks up the streams of the next few songs one at a time, so Next
    /// (even pressed several times in a row) and replays start at once.
    fn warm_ahead(&mut self) {
        if self.ahead_rx.is_some() || self.prefetch_rx.is_some() {
            return;
        }
        self.stream_cache.retain(|_, (_, issued)| issued.elapsed() < CACHE_MAX_AGE);
        let target = self.state.up_next(LOOK_AHEAD).into_iter().find(|track| {
            !self.stream_cache.contains_key(&track.id) && !self.ahead_failed.contains(&track.id)
        });
        let Some(track) = target else { return };
        self.ahead_rx = self.resolve_in_background(&track).map(|rx| (track.id, rx));
        if self.ahead_rx.is_none() {
            self.ahead_failed.insert(track.id);
        }
    }

    fn poll_ahead(&mut self) {
        let Some((id, rx)) = &self.ahead_rx else { return };
        let id = *id;
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(String::new()),
        };
        self.ahead_rx = None;
        match result {
            Ok(stream) => {
                self.stream_cache.insert(id, (stream, Instant::now()));
            }
            Err(_) => {
                self.ahead_failed.insert(id);
            }
        }
    }

    fn poll_prefetch(&mut self) {
        let Some((id, rx)) = &self.prefetch_rx else { return };
        let id = *id;
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(String::new()),
        };
        self.prefetch_rx = None;
        // A failed lookup just means the normal path (resolve at the end of the track) is used.
        let Ok(stream) = result else { return };
        self.stream_cache.insert(id, (stream.clone(), Instant::now()));
        if self.state.adjacent_remote(true) != Some(id) {
            return;
        }
        if let Some(player) = self.mpv.as_mut() {
            if player.queue_stream(&stream).is_ok() {
                self.queued_next = Some((id, Instant::now()));
            }
        }
    }

    /// mpv has started the queued track by itself: make it the current one.
    fn finish_advance(&mut self, id: TrackId) {
        let issued = self.queued_next.take().map(|(_, issued)| issued);
        if let Some(player) = self.mpv.as_mut() {
            player.drop_finished_entry();
        }
        let Some(track) = self.state.tracks.iter().find(|track| track.id == id).cloned() else { return };
        self.state.play_track(id);
        self.library.record_play(&track);
        self.save_library();
        self.stream_issued = issued.or(Some(Instant::now()));
        self.prefetch_tried = None;
        self.position = None;
        self.duration = None;
        if self.state.adjacent_remote(true).is_none() {
            self.start_related(&track);
        }
    }

    /// Looks the current stream up again when its address is about to expire, then resumes from
    /// the same position (also while paused, so resuming later still works).
    fn update_stream_refresh(&mut self) {
        if let Some((id, rx)) = &self.refresh_rx {
            let id = *id;
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(String::new())),
            };
            if let Some(result) = result {
                self.refresh_rx = None;
                if let (Ok(stream), true) = (result, self.state.current_track().map(|track| track.id) == Some(id)) {
                    if let Some(player) = self.mpv.as_mut() {
                        let at = player.position();
                        if player.load_stream(&stream).is_ok() {
                            self.resume_at = at.filter(|value| *value > 1.0);
                            self.stream_issued = Some(Instant::now());
                            self.queued_next = None;
                            self.prefetch_rx = None;
                            self.prefetch_tried = None;
                            self.played_something = false;
                            self.loading_since = Some(Instant::now());
                        }
                    }
                }
            }
        }
        let Some(issued) = self.stream_issued else { return };
        let active = matches!(self.state.playback, PlaybackStatus::Playing | PlaybackStatus::Paused);
        if !active || self.refresh_rx.is_some() || !prefetch::stream_is_stale(issued.elapsed()) {
            return;
        }
        if self.refresh_backoff.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        let Some(track) = self.state.current_track().cloned() else { return };
        self.refresh_backoff = Some(Instant::now());
        self.refresh_rx = self.resolve_in_background(&track).map(|rx| (track.id, rx));
    }

    // ---- Media keys and desktop widgets (MPRIS) ---------------------------------------------

    fn poll_media(&mut self) {
        while let Ok(event) = self.media_rx.try_recv() {
            match event {
                MediaControlEvent::Play => {
                    if self.state.playback == PlaybackStatus::Paused {
                        self.toggle_pause();
                    }
                }
                MediaControlEvent::Pause | MediaControlEvent::Stop => {
                    if self.state.playback == PlaybackStatus::Playing {
                        self.toggle_pause();
                    }
                }
                MediaControlEvent::Toggle => self.toggle_pause(),
                MediaControlEvent::Next => self.skip(true),
                MediaControlEvent::Previous => self.skip(false),
                MediaControlEvent::SetPosition(MediaPosition(position)) => self.seek_to(position.as_secs_f64()),
                _ => {}
            }
        }
        // Tell the desktop about the current track and state, only when they change.
        let key = (self.state.current_track, self.state.playback);
        if self.media_synced != Some(key) {
            self.media_synced = Some(key);
            if let Some(media) = self.media.as_mut() {
                media.update(self.state.current_track(), self.state.playback, self.position);
            }
        }
    }

    // ---- Tray ----------------------------------------------------------------------------

    /// Starts or stops the tray icon to match the setting. If the desktop has no tray, says so
    /// and turns the setting off again.
    pub fn apply_tray(&mut self) {
        if self.settings.tray {
            if self.tray.is_some() {
                return;
            }
            let tx = self.tray_tx.clone();
            match TrayHandle::spawn(tx, || {}) {
                Ok(handle) => {
                    self.tray = Some(handle);
                    self.tray_synced = None;
                }
                Err(message) => {
                    self.notify(message);
                    self.update_settings(|settings| settings.tray = false);
                }
            }
        } else if let Some(handle) = self.tray.take() {
            handle.shutdown();
        }
    }

    /// True while a tray icon is running, so closing the window should only hide it.
    pub fn tray_active(&self) -> bool {
        self.tray.is_some()
    }

    fn poll_tray(&mut self) {
        while let Ok(command) = self.tray_rx.try_recv() {
            match command {
                TrayCommand::Show => self.actions.push(Action::ShowWindow),
                TrayCommand::PlayPause => self.toggle_pause(),
                TrayCommand::Next => self.skip(true),
                TrayCommand::Previous => self.skip(false),
                TrayCommand::Quit => self.actions.push(Action::Quit),
            }
        }
        // Keep the tray's play/pause label and tooltip in step with playback.
        let key = (self.state.current_track, self.state.playback);
        if self.tray_synced != Some(key) {
            if let Some(tray) = self.tray.as_ref() {
                self.tray_synced = Some(key);
                let name = self.state.current_track().map(|track| format!("{} - {}", track.title, track.artist)).unwrap_or_default();
                tray.update(self.state.playback == PlaybackStatus::Playing, &name);
            }
        }
    }

    /// Requests from the engine to the window since the last call.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    // ---- Dynamic colour --------------------------------------------------------------------

    /// Follows the current song's artwork: fetches its dominant hue when the song changes, then
    /// eases the colours towards the matching scheme. `dt` is the time since the last call, in
    /// seconds. Returns the dark colour scheme to draw with when it changed.
    pub fn update_colors(&mut self, dt: f32) -> Option<ColorScheme> {
        let wanted = if self.settings.dynamic_color {
            self.state.current_track().and_then(|track| track.thumbnail_url.clone())
        } else {
            None
        };
        if wanted != self.art_url {
            self.art_url = wanted.clone();
            self.art_rx = None;
            match wanted {
                Some(url) => {
                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move || {
                        let hue = dynamic_color::hue_from_url(&url);
                        let _ = tx.send((url, hue));
                    });
                    self.art_rx = Some(rx);
                }
                None => self.scheme.retarget(None),
            }
        }
        if let Some(Ok((url, hue))) = self.art_rx.as_ref().map(|rx| rx.try_recv()) {
            self.art_rx = None;
            // Ignore a late answer for a song that is no longer playing.
            if self.art_url.as_deref() == Some(url.as_str()) {
                self.scheme.retarget(hue);
            }
        }
        let changed = self.scheme.step(dt) || self.scheme_changed;
        self.scheme_changed = false;
        changed.then(|| self.scheme.current().1)
    }

    /// Stops the tray icon and playback. Call once when the app quits.
    pub fn shutdown(&mut self) {
        if let Some(handle) = self.tray.take() {
            handle.shutdown();
        }
        self.mpv = None;
    }

    /// Forces the next `update_colors` to return the scheme (used once at start-up).
    pub fn request_colors(&mut self) {
        self.scheme_changed = true;
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
