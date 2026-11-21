//! TweMusic with a Slint interface. The window only draws; `twe_music::engine::Engine` does the
//! work (library, search, today's pick, playback). A timer polls the engine and copies what
//! changed into the window.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use slint::{Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel};
use twe_music::{
    engine::{self, Engine},
    models::Track,
    ytmusic,
};

slint::include_modules!();

/// How long a message stays on screen.
const NOTICE_TIME: Duration = Duration::from_secs(4);

type Pixels = SharedPixelBuffer<Rgba8Pixel>;

/// Album art, downloaded once per address in the background. `version` goes up whenever a picture
/// arrives, so the window knows to rebuild its lists.
struct Art {
    cache: Arc<Mutex<HashMap<String, Option<Pixels>>>>,
    started: HashSet<String>,
    version: Arc<AtomicU64>,
}

impl Art {
    fn new() -> Self {
        Self { cache: Arc::default(), started: HashSet::new(), version: Arc::default() }
    }

    fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Starts downloading `url` if that has not been started yet.
    fn ensure(&mut self, url: &str) {
        if !self.started.insert(url.to_owned()) {
            return;
        }
        let url = url.to_owned();
        let cache = Arc::clone(&self.cache);
        let version = Arc::clone(&self.version);
        thread::spawn(move || {
            let pixels = download(&url);
            if let Ok(mut cache) = cache.lock() {
                cache.insert(url, pixels);
            }
            version.fetch_add(1, Ordering::Relaxed);
        });
    }

    /// The picture for `url`, or an empty image while it loads (or when it cannot be loaded).
    fn get(&self, url: &str) -> Image {
        let pixels = self.cache.lock().ok().and_then(|cache| cache.get(url).cloned().flatten());
        pixels.map(Image::from_rgba8).unwrap_or_default()
    }
}

/// Downloads and decodes one picture, shrunk to at most 360 px. Only the image hosts the library
/// allows are contacted.
fn download(url: &str) -> Option<Pixels> {
    if !ytmusic::is_allowed_thumbnail_url(url) {
        return None;
    }
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
    let bytes = client.get(url).send().ok()?.error_for_status().ok()?.bytes().ok()?;
    let picture = image::load_from_memory(&bytes).ok()?.thumbnail(360, 360).to_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(picture.as_raw(), picture.width(), picture.height()))
}

/// A colour for a song while its artwork loads, the same one every time for the same song.
fn tint_for(track: &Track) -> Color {
    const TINTS: [(u8, u8, u8); 8] =
        [(90, 143, 134), (107, 107, 120), (122, 90, 150), (138, 59, 59), (59, 106, 138), (150, 110, 60), (80, 120, 80), (140, 80, 120)];
    let (r, g, b) = TINTS[(track.id.0 % TINTS.len() as u64) as usize];
    Color::from_rgb_u8(r, g, b)
}

fn song_of(track: &Track, art: &mut Art) -> Song {
    let picture = match track.thumbnail_url.as_deref() {
        Some(url) => {
            art.ensure(url);
            art.get(url)
        }
        None => Image::default(),
    };
    Song { title: track.title.as_str().into(), artist: track.artist.as_str().into(), tint: tint_for(track), art: picture }
}

fn songs_of(tracks: &[Track], art: &mut Art) -> ModelRc<Song> {
    ModelRc::new(VecModel::from(tracks.iter().map(|track| song_of(track, art)).collect::<Vec<_>>()))
}

fn clock(seconds: Option<f64>) -> SharedString {
    let total = seconds.unwrap_or(0.0).max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60).into()
}

/// What the lists were built from. When it changes, the lists are rebuilt.
#[derive(Default, PartialEq)]
struct ListKey {
    history_len: usize,
    history_first: Option<String>,
    liked_len: usize,
    pick: Option<(u32, u64)>,
    art_version: u64,
}

/// Copies the engine's state into the window. Cheap values are set every time; the song lists
/// only when something they show has changed.
fn sync(window: &AppWindow, engine: &Engine, art: &mut Art, key: &mut ListKey) {
    window.set_greeting(engine::greeting(engine.hour).into());

    let next = ListKey {
        history_len: engine.library.history.len(),
        history_first: engine.library.history.first().map(|saved| saved.video_id.clone()),
        liked_len: engine.library.playlists.first().map_or(0, |playlist| playlist.tracks.len()),
        pick: engine.pick.as_ref().map(|pick| (pick.day, pick.track.id.0)),
        art_version: art.version(),
    };
    if *key != next {
        *key = next;
        let history = engine.history_tracks();
        window.set_recent(songs_of(&history, art));
        window.set_quick(songs_of(&history[..history.len().min(8)], art));
        window.set_liked(songs_of(&engine.liked_tracks(), art));
        match &engine.pick {
            Some(pick) => {
                window.set_pick(song_of(&pick.track, art));
                window.set_pick_why(pick.why.as_str().into());
                window.set_more(songs_of(&pick.more, art));
                window.set_has_pick(true);
            }
            None => window.set_has_pick(false),
        }
    }

    let current = engine.current_track();
    window.set_has_track(current.is_some());
    window.set_now_title(current.map_or("Nothing playing", |track| track.title.as_str()).into());
    window.set_now_artist(current.map_or("Pick a song to start", |track| track.artist.as_str()).into());
    window.set_now_message(engine.message.as_deref().unwrap_or("").into());
    let picture = match current.and_then(|track| track.thumbnail_url.as_deref()) {
        Some(url) => {
            art.ensure(url);
            art.get(url)
        }
        None => Image::default(),
    };
    window.set_now_art(picture);
    window.set_playing(engine.is_playing());
    window.set_now_liked(engine.current_is_liked());
    window.set_progress(match (engine.position, engine.duration) {
        (Some(position), Some(duration)) if duration > 0.0 => (position / duration) as f32,
        _ => 0.0,
    });
    window.set_time_position(clock(engine.position));
    window.set_time_duration(clock(engine.duration));
}

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    let engine = Rc::new(RefCell::new(Engine::new()));

    {
        let engine = Rc::clone(&engine);
        window.on_play_pick(move || {
            let mut engine = engine.borrow_mut();
            let Some(pick) = engine.pick.clone() else { return };
            let mut queue = vec![pick.track.clone()];
            queue.extend(pick.more.iter().cloned());
            engine.play_from(queue, 0);
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_more(move |index| {
            let mut engine = engine.borrow_mut();
            let Some(pick) = engine.pick.clone() else { return };
            engine.play_from(pick.more, index.max(0) as usize);
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_recent(move |index| {
            let mut engine = engine.borrow_mut();
            let tracks = engine.history_tracks();
            engine.play_from(tracks, index.max(0) as usize);
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_liked(move |index| {
            let mut engine = engine.borrow_mut();
            let tracks = engine.liked_tracks();
            engine.play_from(tracks, index.max(0) as usize);
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_toggle_pause(move || engine.borrow_mut().toggle_pause());
    }
    {
        let engine = Rc::clone(&engine);
        window.on_next(move || engine.borrow_mut().skip(true));
    }
    {
        let engine = Rc::clone(&engine);
        window.on_previous(move || engine.borrow_mut().skip(false));
    }
    {
        let engine = Rc::clone(&engine);
        window.on_seek(move |fraction| {
            let mut engine = engine.borrow_mut();
            if let Some(duration) = engine.duration {
                engine.seek_to(f64::from(fraction) * duration);
            }
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_toggle_like(move || engine.borrow_mut().toggle_like_current());
    }

    // Every 250 ms: collect finished background work, follow mpv, and refresh the window.
    let timer = Timer::default();
    {
        let weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let mut art = Art::new();
        let mut key = ListKey::default();
        let mut notice_until: Option<Instant> = None;
        timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let Some(window) = weak.upgrade() else { return };
            let mut engine = engine.borrow_mut();
            engine.ensure_pick();
            engine.poll();
            engine.tick();
            if let Some(message) = engine.take_notices().pop() {
                window.set_notice(message.into());
                notice_until = Some(Instant::now() + NOTICE_TIME);
            }
            if notice_until.is_some_and(|until| Instant::now() >= until) {
                window.set_notice(SharedString::new());
                notice_until = None;
            }
            sync(&window, &engine, &mut art, &mut key);
        });
    }

    window.run()
}
