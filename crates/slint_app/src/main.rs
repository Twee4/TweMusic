//! TweMusic with a Slint interface. The window only draws; `twe_music::engine::Engine` does the
//! work (library, search, today's pick, playback). A timer polls the engine and copies what
//! changed into the window.

#[allow(dead_code)] // the loader shapes and path output are kept for later
mod shape;

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

use slint::{
    CloseRequestResponse, Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel,
};
use twe_music::{
    engine::{self, Action, Engine},
    models::Track,
    theme::ColorScheme,
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
        self.pixels(url).map(Image::from_rgba8).unwrap_or_default()
    }

    fn pixels(&self, url: &str) -> Option<Pixels> {
        self.cache.lock().ok().and_then(|cache| cache.get(url).cloned().flatten())
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

fn entries_of(tracks: &[Track], art: &mut Art, liked: impl Fn(&Track) -> bool) -> ModelRc<Entry> {
    let rows: Vec<Entry> = tracks
        .iter()
        .map(|track| {
            let song = song_of(track, art);
            Entry {
                title: song.title,
                artist: song.artist,
                tint: song.tint,
                art: song.art,
                liked: liked(track),
                duration: track.duration_label().into(),
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// A cheap fingerprint of a song list, to notice when it changed.
fn fingerprint(tracks: &[Track]) -> u64 {
    tracks.iter().fold(tracks.len() as u64, |acc, track| acc.wrapping_mul(31).wrapping_add(track.id.0))
}

/// Turns one of the scheme's colours into a Slint colour.
macro_rules! col {
    ($color:expr) => {
        Color::from_rgb_u8($color.r(), $color.g(), $color.b())
    };
}

/// Gives the window the colours of `scheme` (they follow the album art of the current song).
fn apply_palette(window: &AppWindow, scheme: &ColorScheme) {
    let palette = window.global::<Palette>();
    palette.set_background(col!(scheme.surface));
    palette.set_accent(col!(scheme.primary_container));
    palette.set_primary(col!(scheme.primary));
    palette.set_on_primary(col!(scheme.on_primary));
    palette.set_aurora_a(col!(scheme.primary_container));
    palette.set_aurora_b(col!(scheme.tertiary_container));
    palette.set_on_surface(col!(scheme.on_surface));
    palette.set_on_surface_variant(col!(scheme.on_surface_variant));
    let tint = |alpha: u8| {
        let c = scheme.secondary_container;
        Color::from_argb_u8(alpha, c.r(), c.g(), c.b())
    };
    palette.set_glass(tint(115));
    palette.set_glass_hover(tint(180));
    palette.set_glass_solid(col!(scheme.surface_container_high));
}

/// Side of the picture the album-art outline is cut from (twice the 56 px it is shown at).
const ART_SIZE: u32 = 112;

/// `pixels` cropped to a square in the middle and shrunk to `size` x `size` RGBA bytes.
fn square_thumbnail(pixels: &Pixels, size: u32) -> Option<Vec<u8>> {
    let picture = image::RgbaImage::from_raw(pixels.width(), pixels.height(), pixels.as_bytes().to_vec())?;
    let side = picture.width().min(picture.height());
    let cropped = image::imageops::crop_imm(&picture, (picture.width() - side) / 2, (picture.height() - side) / 2, side, side).to_image();
    Some(image::imageops::resize(&cropped, size, size, image::imageops::FilterType::Triangle).into_raw())
}

/// The played part of the progress bar as a wave, `width` x 12 logical pixels (drawn at twice that
/// and white, so the window can colour it). `phase` moves the wave along.
fn wave_image(width: f32, amplitude: f32, phase: f32) -> Image {
    const SCALE: f32 = 2.0;
    const WAVELENGTH: f32 = 26.0;
    const HEIGHT: f32 = 12.0;
    const SWING: f32 = 3.0;
    const HALF_STROKE: f32 = 2.0;
    let (columns, rows) = ((width * SCALE).ceil().max(1.0) as u32, (HEIGHT * SCALE) as u32);
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(columns, rows);
    let pixels = buffer.make_mut_slice();
    for column in 0..columns {
        let x = (column as f32 + 0.5) / SCALE;
        let angle = x / WAVELENGTH * std::f32::consts::TAU - phase;
        let centre = HEIGHT / 2.0 + SWING * amplitude * angle.sin();
        let slope = SWING * amplitude * std::f32::consts::TAU / WAVELENGTH * angle.cos();
        let stretch = (1.0 + slope * slope).sqrt();
        // The ends are round: measure from the nearest point of the line's span.
        let along = x - x.clamp(HALF_STROKE, (width - HALF_STROKE).max(HALF_STROKE));
        for row in 0..rows {
            let y = (row as f32 + 0.5) / SCALE;
            let distance = (along.hypot(y - centre)) / stretch;
            let coverage = ((HALF_STROKE - distance) * SCALE + 0.5).clamp(0.0, 1.0);
            pixels[(row * columns + column) as usize] = Rgba8Pixel { r: 255, g: 255, b: 255, a: (coverage * 255.0) as u8 };
        }
    }
    Image::from_rgba8(buffer)
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
    results: u64,
    liked_marks: usize,
    playlist: u64,
    selected_playlist: usize,
    playlist_names: u64,
}

/// Copies the engine's state into the window. Cheap values are set every time; the song lists
/// only when something they show has changed.
fn sync(window: &AppWindow, engine: &Engine, art: &mut Art, key: &mut ListKey, now_pixels: &mut Option<Pixels>) {
    window.set_greeting(engine::greeting(engine.hour).into());

    let next = ListKey {
        history_len: engine.library.history.len(),
        history_first: engine.library.history.first().map(|saved| saved.video_id.clone()),
        liked_len: engine.library.playlists.first().map_or(0, |playlist| playlist.tracks.len()),
        pick: engine.pick.as_ref().map(|pick| (pick.day, pick.track.id.0)),
        art_version: art.version(),
        results: fingerprint(&engine.state.remote_results),
        liked_marks: engine.state.remote_results.iter().filter(|track| engine.is_liked(track)).count(),
        playlist: fingerprint(&engine.playlist_tracks()),
        selected_playlist: engine.selected_playlist,
        playlist_names: engine.library.playlists.iter().fold(0u64, |acc, playlist| {
            acc.wrapping_mul(31).wrapping_add(playlist.name.len() as u64).wrapping_add(playlist.tracks.len() as u64 * 7)
        }),
    };
    if *key != next {
        *key = next;
        let history = engine.history_tracks();
        window.set_recent(songs_of(&history, art));
        window.set_quick(songs_of(&history[..history.len().min(8)], art));
        window.set_liked(songs_of(&engine.liked_tracks(), art));
        window.set_results(entries_of(&engine.state.remote_results, art, |track| engine.is_liked(track)));
        window.set_playlists(ModelRc::new(VecModel::from(
            engine
                .library
                .playlists
                .iter()
                .map(|playlist| SharedString::from(format!("{} ({})", playlist.name, playlist.tracks.len())))
                .collect::<Vec<_>>(),
        )));
        window.set_selected_playlist(engine.selected_playlist as i32);
        window.set_playlist_title(engine.library.playlists.get(engine.selected_playlist).map_or("", |playlist| playlist.name.as_str()).into());
        window.set_library_rows(entries_of(&engine.playlist_tracks(), art, |_| false));
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

    window.set_search_status(if engine.searching { "Searching…" } else { engine.state.search_message.as_deref().unwrap_or("") }.into());

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
    *now_pixels = current.and_then(|track| track.thumbnail_url.as_deref()).and_then(|url| art.pixels(url));
    window.set_playing(engine.is_playing());
    window.set_loading(engine.is_loading());
    window.set_now_liked(engine.current_is_liked());
    window.set_progress(match (engine.position, engine.duration) {
        (Some(position), Some(duration)) if duration > 0.0 => (position / duration) as f32,
        _ => 0.0,
    });
    window.set_time_position(clock(engine.position));
    window.set_time_duration(clock(engine.duration));
    window.set_now_length(engine.duration.unwrap_or(0.0) as f32);
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

    {
        let engine = Rc::clone(&engine);
        window.on_search(move |query| engine.borrow_mut().start_search(&query));
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_result(move |index| engine.borrow_mut().play_result(index.max(0) as usize));
    }
    {
        let engine = Rc::clone(&engine);
        window.on_like_result(move |index| {
            let mut engine = engine.borrow_mut();
            if let Some(track) = engine.state.remote_results.get(index.max(0) as usize).cloned() {
                engine.toggle_like(&track);
            }
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_select_playlist(move |index| engine.borrow_mut().select_playlist(index.max(0) as usize));
    }
    {
        let engine = Rc::clone(&engine);
        let weak = window.as_weak();
        window.on_create_playlist(move |name| {
            if engine.borrow_mut().create_playlist(&name) {
                if let Some(window) = weak.upgrade() {
                    window.set_new_playlist(SharedString::new());
                }
            }
        });
    }
    {
        let engine = Rc::clone(&engine);
        let weak = window.as_weak();
        window.on_import_playlist(move |url| {
            engine.borrow_mut().start_import(&url);
            if let Some(window) = weak.upgrade() {
                window.set_import_url(SharedString::new());
            }
        });
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_playlist(move || engine.borrow_mut().play_selected_playlist());
    }
    {
        let engine = Rc::clone(&engine);
        window.on_delete_playlist(move || engine.borrow_mut().delete_selected_playlist());
    }
    {
        let engine = Rc::clone(&engine);
        window.on_remove_track(move |index| engine.borrow_mut().remove_from_selected(index.max(0) as usize));
    }
    {
        let engine = Rc::clone(&engine);
        window.on_play_library(move |index| {
            let mut engine = engine.borrow_mut();
            let tracks = engine.playlist_tracks();
            engine.play_from(tracks, index.max(0) as usize);
        });
    }
    {
        let engine = Rc::clone(&engine);
        let weak = window.as_weak();
        window.on_settings_changed(move |name, value| {
            let mut engine = engine.borrow_mut();
            engine.update_settings(|settings| match name.as_str() {
                "notifications" => settings.notifications = value,
                "tray" => settings.tray = value,
                "dynamic-color" => settings.dynamic_color = value,
                _ => {}
            });
            if name.as_str() == "tray" {
                engine.apply_tray();
                // If the desktop has no tray, the engine turned the setting back off.
                if let Some(window) = weak.upgrade() {
                    window.set_set_tray(engine.settings.tray);
                }
            }
        });
    }
    {
        let settings = engine.borrow().settings.clone();
        window.set_set_notifications(settings.notifications);
        window.set_set_tray(settings.tray);
        window.set_set_dynamic_color(settings.dynamic_color);
    }

    // Every 250 ms: collect finished background work, follow mpv, and refresh the window.
    // The current song's picture, shared with the animation timer below.
    let now_pixels: Rc<RefCell<Option<Pixels>>> = Rc::default();
    let timer = Timer::default();
    {
        let weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let mut art = Art::new();
        let now_pixels = Rc::clone(&now_pixels);
        let mut key = ListKey::default();
        let mut notice_until: Option<Instant> = None;
        timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let Some(window) = weak.upgrade() else { return };
            let mut engine = engine.borrow_mut();
            engine.ensure_pick();
            engine.poll();
            engine.tick();
            for action in engine.take_actions() {
                match action {
                    Action::ShowWindow => {
                        let _ = window.show();
                        engine.window_hidden = false;
                    }
                    Action::Quit => {
                        let _ = slint::quit_event_loop();
                    }
                }
            }
            if let Some(message) = engine.take_notices().pop() {
                window.set_notice(message.into());
                notice_until = Some(Instant::now() + NOTICE_TIME);
            }
            if notice_until.is_some_and(|until| Instant::now() >= until) {
                window.set_notice(SharedString::new());
                notice_until = None;
            }
            sync(&window, &engine, &mut art, &mut key, &mut now_pixels.borrow_mut());
        });
    }

    // Colours ease towards the current song's artwork, so this timer runs faster.
    let color_timer = Timer::default();
    {
        let weak = window.as_weak();
        let engine = Rc::clone(&engine);
        engine.borrow_mut().request_colors();
        let mut last = Instant::now();
        let now_pixels = Rc::clone(&now_pixels);
        let (cookie, squircle) = (shape::COOKIE_9.radii(), shape::squircle());
        let mut art_morph = shape::Morph::new();
        let mut art_key: Option<(usize, u32, u32)> = None;
        let mut art_small: Option<Vec<u8>> = None;
        let mut wave_phase = 0.0_f32;
        let mut wave_amplitude = 0.0_f32;
        color_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32();
            last = now;
            let Some(window) = weak.upgrade() else { return };
            // The album art is a rounded square while paused and a turning nine-sided cookie while
            // playing; the outline springs from one to the other.
            let playing = window.get_playing();
            let (target, spin) = if playing { (&cookie, 0.6) } else { (&squircle, 0.0) };
            art_morph.step(target, dt, spin);
            let shaped = match now_pixels.borrow().as_ref() {
                Some(pixels) if playing || !art_morph.settled(target) => {
                    if art_key != Some((pixels.as_bytes().as_ptr() as usize, pixels.width(), pixels.height())) {
                        art_key = Some((pixels.as_bytes().as_ptr() as usize, pixels.width(), pixels.height()));
                        art_small = square_thumbnail(pixels, ART_SIZE);
                    }
                    art_small.as_ref().map(|rgba| {
                        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&art_morph.mask(rgba, ART_SIZE), ART_SIZE, ART_SIZE)
                    })
                }
                _ => None,
            };
            window.set_now_art_shaped(shaped.map(Image::from_rgba8).unwrap_or_default());
            // The progress bar waves while the song plays and settles flat when it pauses.
            let target_wave = if playing { 1.0 } else { 0.0 };
            wave_amplitude += (target_wave - wave_amplitude) * (1.0 - (-dt * 8.0).exp());
            wave_phase = (wave_phase + dt * 5.0) % std::f32::consts::TAU;
            let played = window.get_seek_width() * window.get_progress().clamp(0.0, 1.0);
            if wave_amplitude > 0.02 && played > 6.0 {
                window.set_wave_extent(played);
                window.set_wave_image(wave_image(played, wave_amplitude, wave_phase));
            } else {
                window.set_wave_image(Image::default());
            }
            if let Some(scheme) = engine.borrow_mut().update_colors(dt) {
                apply_palette(&window, &scheme);
            }
        });
    }

    // Closing the window hides it when the tray is on (music keeps playing); otherwise it quits.
    {
        let engine = Rc::clone(&engine);
        window.window().on_close_requested(move || {
            let mut engine = engine.borrow_mut();
            if engine.tray_active() {
                engine.window_hidden = true;
            } else {
                let _ = slint::quit_event_loop();
            }
            CloseRequestResponse::HideWindow
        });
    }

    // The loop runs until `quit_event_loop`, so a hidden window (in the tray) keeps it alive.
    window.show()?;
    slint::run_event_loop_until_quit()?;
    engine.borrow_mut().shutdown();
    drop(timer);
    drop(color_timer);
    Ok(())
}
