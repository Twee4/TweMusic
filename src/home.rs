//! The Home screen, laid out like Spotify's: a greeting, filter chips, a two-column grid of
//! quick-access cards and horizontal shelves of artwork, over the glass aurora backdrop.

use std::{
    collections::{HashMap, HashSet},
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use eframe::egui::{
    self, pos2, scroll_area::ScrollBarVisibility, vec2, Align, Align2, FontId, Id, Layout, Margin, Rect,
    Rounding, ScrollArea, Sense, Ui,
};
use twe_music::{
    glass::{self, Shelf, Shortcut},
    icons::Icon,
    library::{self, SavedTrack},
    m3::{self, IconButtonKind},
    models::{Screen, Track},
    state::PlaybackStatus,
    theme::{self, shape, ColorScheme, Role},
    ytmusic,
};

use crate::TweMusicApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Mood {
    Energize,
    Relax,
    FeelGood,
    Workout,
}

impl Mood {
    const ALL: [Mood; 4] = [Mood::Energize, Mood::Relax, Mood::FeelGood, Mood::Workout];

    fn label(self) -> &'static str {
        match self {
            Mood::Energize => "Energize",
            Mood::Relax => "Relax",
            Mood::FeelGood => "Feel Good",
            Mood::Workout => "Workout",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Mood::Energize => Icon::Bolt,
            Mood::Relax => Icon::Moon,
            Mood::FeelGood => Icon::Smile,
            Mood::Workout => Icon::Dumbbell,
        }
    }

    /// What is searched on YouTube Music for this mood.
    fn query(self) -> &'static str {
        match self {
            Mood::Energize => "energizing upbeat songs",
            Mood::Relax => "relaxing chill songs",
            Mood::FeelGood => "feel good hits",
            Mood::Workout => "workout gym music",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sheet {
    History,
    Activity,
}

/// What a tap on Home should play.
#[derive(Clone, Copy)]
enum Play {
    Playlist(usize),
    /// A song from the quick-access pool (mood picks, or recent and liked songs).
    Pool(usize),
    History(usize),
    Liked(usize),
}

/// Everything the Home screen remembers between frames.
#[derive(Default)]
pub(crate) struct HomeState {
    mood: Option<Mood>,
    moods: HashMap<Mood, Vec<Track>>,
    loading: Option<(Mood, Receiver<Result<Vec<Track>, String>>)>,
    sheet: Option<Sheet>,
    /// The local hour of day and when it was read, for the greeting.
    clock: Option<(u32, Instant)>,
}

/// The local hour (0-23). Rust's standard library has no time zones, so ask `date`;
/// if that fails, fall back to UTC.
fn local_hour() -> u32 {
    let from_date = Command::new("date")
        .arg("+%H")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u32>().ok())
        .filter(|hour| *hour < 24);
    from_date.unwrap_or_else(|| {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|since| ((since.as_secs() / 3600) % 24) as u32).unwrap_or(12)
    })
}

fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

fn unix_day() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| (since.as_secs() / 86_400) as u32).unwrap_or(0)
}

fn unix_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_nanos() as u64).unwrap_or(1)
}

impl TweMusicApp {
    /// Receives a finished mood search.
    pub(crate) fn poll_home(&mut self) {
        let Some((mood, receiver)) = &self.home.loading else { return };
        let mood = *mood;
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The search stopped unexpectedly.".to_owned()),
        };
        self.home.loading = None;
        match result {
            Ok(tracks) => {
                self.home.moods.insert(mood, tracks);
            }
            Err(error) => {
                if self.home.mood == Some(mood) {
                    self.home.mood = None;
                }
                self.notify(error);
            }
        }
    }

    /// Selects (or deselects) a mood and looks its songs up once.
    fn select_mood(&mut self, mood: Mood) {
        if self.home.mood == Some(mood) {
            self.home.mood = None;
            return;
        }
        self.home.mood = Some(mood);
        let busy = self.home.loading.as_ref().is_some_and(|(loading, _)| *loading == mood);
        if self.home.moods.contains_key(&mood) || busy {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let wake = self.ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(ytmusic::search_tracks(mood.query()));
            wake.request_repaint();
        });
        // Replacing the receiver drops an older lookup; its answer is simply ignored.
        self.home.loading = Some((mood, rx));
    }

    /// The songs on the Speed dial: the chosen mood's, else what was played lately and liked.
    fn dial_tracks(&self) -> Vec<Track> {
        if let Some(mood) = self.home.mood {
            return self.home.moods.get(&mood).cloned().unwrap_or_default();
        }
        let liked: &[SavedTrack] = self.library.playlists.first().map(|playlist| playlist.tracks.as_slice()).unwrap_or(&[]);
        let mut seen = HashSet::new();
        let mut tracks = Vec::new();
        for saved in self.library.history.iter().chain(liked.iter()) {
            if tracks.len() >= 3 * 6 {
                break;
            }
            if seen.insert(saved.video_id.clone()) {
                tracks.push(saved.to_track());
            }
        }
        tracks
    }

    /// Queues `tracks` starting at `index` (the earlier ones follow at the end) and plays it.
    fn play_from(&mut self, mut tracks: Vec<Track>, index: usize) {
        if index >= tracks.len() {
            return;
        }
        tracks.rotate_left(index);
        let first = tracks[0].clone();
        self.state.set_queue(tracks);
        self.play_remote(&first);
    }

    fn shuffle_all(&mut self) {
        let mut seen = HashSet::new();
        let mut tracks = Vec::new();
        for playlist in &self.library.playlists {
            for saved in &playlist.tracks {
                if seen.insert(saved.video_id.clone()) {
                    tracks.push(saved.to_track());
                }
            }
        }
        if tracks.is_empty() {
            tracks = self.dial_tracks();
        }
        if tracks.is_empty() {
            self.notify("Like a song or pick a mood first, then shuffle.");
            return;
        }
        let tracks = library::shuffled(tracks, unix_seed());
        self.play_from(tracks, 0);
    }

    /// Adds listening time (called while a song is really playing).
    pub(crate) fn record_listening(&mut self, elapsed: Duration) {
        self.listen_carry += elapsed.as_secs_f32().min(2.0);
        if self.listen_carry >= 30.0 {
            let seconds = self.listen_carry as u32;
            self.listen_carry -= seconds as f32;
            self.library.add_listening(unix_day(), seconds);
            self.save_library();
        }
    }

    fn liked_tracks(&self) -> Vec<Track> {
        self.library.playlists.first().map(|playlist| playlist.tracks.iter().map(SavedTrack::to_track).collect()).unwrap_or_default()
    }

    fn history_tracks(&self) -> Vec<Track> {
        self.library.history.iter().map(SavedTrack::to_track).collect()
    }

    pub(crate) fn render_home(&mut self, ui: &mut Ui) {
        let c = theme::colors(ui);
        let mut picked_mood = None;
        let mut all = false;
        let mut play = None;
        let mut shuffle = false;
        let mut open_sheet = None;
        let mut open_settings = false;

        // The greeting only needs the hour, so look at the clock once a minute.
        let stale = self.home.clock.map_or(true, |(_, read)| read.elapsed() > Duration::from_secs(60));
        if stale {
            self.home.clock = Some((local_hour(), Instant::now()));
        }
        let hello = greeting(self.home.clock.map_or(12, |(hour, _)| hour));

        let playing_id = if self.state.playback == PlaybackStatus::Playing { self.state.current_track } else { None };
        let pool = self.dial_tracks();
        let history = self.history_tracks();
        let liked = self.liked_tracks();
        let playlists: Vec<(usize, String, usize, Option<String>)> = self
            .library
            .playlists
            .iter()
            .enumerate()
            .filter(|(_, playlist)| !playlist.tracks.is_empty())
            .map(|(index, playlist)| {
                let art = playlist.tracks.iter().find_map(|saved| saved.thumbnail_url.clone());
                (index, playlist.name.clone(), playlist.tracks.len(), art)
            })
            .collect();

        ScrollArea::vertical().auto_shrink([false, false]).id_salt("home-scroll").show(ui, |ui| {
            let width = ui.available_width().min(900.0);
            ui.set_max_width(width);
            ui.add_space(4.0);

            // Greeting and the round buttons.
            ui.allocate_ui_with_layout(vec2(width, 52.0), Layout::left_to_right(Align::Center), |ui| {
                ui.label(theme::text(Role::HeadlineSmall, hello).color(c.on_surface));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::User, false, "Profile and settings").clicked() {
                        open_settings = true;
                    }
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::Activity, false, "Listening activity").clicked() {
                        open_sheet = Some(Sheet::Activity);
                    }
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::History, false, "Recently played").clicked() {
                        open_sheet = Some(Sheet::History);
                    }
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::Shuffle, false, "Shuffle all").clicked() {
                        shuffle = true;
                    }
                });
            });
            ui.add_space(6.0);

            // Filter chips: All, then the moods.
            ScrollArea::horizontal()
                .id_salt("home-moods")
                .auto_shrink([false, true])
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if glass::filter_chip(ui, Id::new("home-chip-all"), "All", self.home.mood.is_none(), &c).clicked() {
                            all = true;
                        }
                        for mood in Mood::ALL {
                            let id = Id::new(("home-chip", mood.label()));
                            if glass::filter_chip(ui, id, mood.label(), self.home.mood == Some(mood), &c).clicked() {
                                picked_mood = Some(mood);
                            }
                        }
                        if self.home.loading.is_some() {
                            m3::loading_indicator(ui, 24.0);
                        }
                    });
                });
            ui.add_space(16.0);

            // Quick access: two columns of short cards.
            let mut quick: Vec<(String, Option<String>, u64, bool, Play)> = Vec::new();
            if self.home.mood.is_none() {
                for (index, name, _, art) in &playlists {
                    quick.push((name.clone(), art.clone(), *index as u64 * 37 + 11, *index == 0, Play::Playlist(*index)));
                }
            }
            for (index, track) in pool.iter().enumerate() {
                quick.push((track.title.clone(), track.thumbnail_url.clone(), track.id.0, false, Play::Pool(index)));
            }
            quick.truncate(8);
            if quick.is_empty() {
                let message = if self.home.loading.is_some() {
                    "Finding songs…"
                } else {
                    "Play something and it shows up here, or pick a mood above."
                };
                glass::glass_frame(ui.visuals().dark_mode, shape::EXTRA_LARGE).inner_margin(Margin::same(20.0)).show(ui, |ui| {
                    ui.set_width(width - 40.0);
                    ui.label(theme::text(Role::BodyLarge, message).color(c.on_surface_variant));
                });
            } else {
                let gap = 8.0;
                let columns = if width < 300.0 { 1 } else { 2 };
                let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
                let rows = quick.len().div_ceil(columns);
                let (area, _) = ui.allocate_exact_size(vec2(width, rows as f32 * 56.0 + (rows as f32 - 1.0) * gap), Sense::hover());
                for (slot, (title, art, seed, heart, action)) in quick.iter().enumerate() {
                    let (row, column) = ((slot / columns) as f32, (slot % columns) as f32);
                    let rect = Rect::from_min_size(
                        area.min + vec2(column * (card_width + gap), row * (56.0 + gap)),
                        vec2(card_width, 56.0),
                    );
                    let is_playing = match action {
                        Play::Pool(index) => pool.get(*index).is_some_and(|track| Some(track.id) == playing_id),
                        _ => false,
                    };
                    let item = Shortcut { title, art: art.as_deref(), seed: *seed, heart: *heart, playing: is_playing };
                    if glass::shortcut_card(ui, rect, Id::new(("home-quick", slot, title.as_str())), &item, &c).clicked() {
                        play = Some(*action);
                    }
                }
            }
            ui.add_space(26.0);

            let card = if width < 420.0 { 132.0 } else { 156.0 };
            // Shelves. With a mood chosen, its songs are the shelf; otherwise recents, playlists and likes.
            if let Some(mood) = self.home.mood {
                if pool.len() > 8 {
                    let heading = format!("More {}", mood.label().to_lowercase());
                    if let Some(index) = shelf(ui, &heading, card, "home-shelf-mood", pool.iter().skip(8), 8, playing_id, &c) {
                        play = Some(Play::Pool(index));
                    }
                }
            } else {
                if let Some(index) = shelf(ui, "Recently played", card, "home-shelf-history", history.iter(), 0, playing_id, &c) {
                    play = Some(Play::History(index));
                }
                if let Some(index) = playlist_shelf(ui, "Your playlists", card, &playlists, &c) {
                    play = Some(Play::Playlist(index));
                }
                if let Some(index) = shelf(ui, "Jump back in", card, "home-shelf-liked", liked.iter(), 0, playing_id, &c) {
                    play = Some(Play::Liked(index));
                }
            }
            ui.add_space(24.0);
        });

        if all {
            self.home.mood = None;
        }
        if let Some(mood) = picked_mood {
            // A chip that is already selected stays selected; "All" is what clears it.
            if self.home.mood != Some(mood) {
                self.select_mood(mood);
            }
        }
        match play {
            Some(Play::Playlist(index)) => self.play_playlist(index),
            Some(Play::Pool(index)) => self.play_from(pool, index),
            Some(Play::History(index)) => self.play_from(history, index),
            Some(Play::Liked(index)) => self.play_from(liked, index),
            None => {}
        }
        if shuffle {
            self.shuffle_all();
        }
        if open_settings {
            self.state.screen = Screen::Settings;
        }
        if open_sheet.is_some() {
            self.home.sheet = open_sheet;
        }
        self.render_home_sheet(&ui.ctx().clone());
    }

    /// The History and Activity sheets.
    fn render_home_sheet(&mut self, ctx: &egui::Context) {
        let Some(sheet) = self.home.sheet else { return };
        let dark = ctx.style().visuals.dark_mode;
        let c = theme::colors_ctx(ctx);
        let width = (ctx.screen_rect().width() - 32.0).clamp(240.0, 380.0);
        let mut close = false;
        let mut play = None;
        let history: Vec<SavedTrack> = self.library.history.clone();
        let week = self.library.listening_week(unix_day(), 7);
        let today = unix_day();

        egui::Window::new("home-sheet")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::none()
                    .fill(c.surface_container_high)
                    .stroke(glass::glass_edge(dark))
                    .shadow(glass::glass_shadow(dark))
                    .rounding(Rounding::same(shape::EXTRA_LARGE))
                    .inner_margin(Margin::same(20.0)),
            )
            .show(ctx, |ui| {
                ui.set_width(width - 40.0);
                ui.horizontal(|ui| {
                    let title = if sheet == Sheet::History { "Recently played" } else { "Listening activity" };
                    ui.label(theme::text(Role::TitleLarge, title).color(c.on_surface));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if m3::icon_button(ui, IconButtonKind::Standard, Icon::Close, false, "Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                match sheet {
                    Sheet::History => {
                        if history.is_empty() {
                            ui.label(theme::text(Role::BodyMedium, "Songs you play will be listed here.").color(c.on_surface_variant));
                        }
                        ScrollArea::vertical().max_height(360.0).auto_shrink([true, true]).show(ui, |ui| {
                            for (index, saved) in history.iter().enumerate() {
                                if history_row(ui, saved, index, &c) {
                                    play = Some(index);
                                }
                            }
                        });
                    }
                    Sheet::Activity => activity_chart(ui, &week, today, &c),
                }
            });

        if close || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.home.sheet = None;
        }
        if let Some(index) = play {
            let tracks: Vec<Track> = history.iter().map(SavedTrack::to_track).collect();
            self.home.sheet = None;
            self.play_from(tracks, index);
        }
    }
}

/// A heading and a row of artwork cards that scrolls sideways. Returns the tapped card's
/// position in `tracks` plus `offset`. Draws nothing when there are no songs.
fn shelf<'a>(
    ui: &mut Ui,
    heading: &str,
    card: f32,
    id: &str,
    tracks: impl Iterator<Item = &'a Track>,
    offset: usize,
    playing: Option<twe_music::models::TrackId>,
    c: &ColorScheme,
) -> Option<usize> {
    let tracks: Vec<&Track> = tracks.take(15).collect();
    if tracks.is_empty() {
        return None;
    }
    let mut tapped = None;
    ui.label(theme::text(Role::TitleLarge, heading).color(c.on_surface));
    ui.add_space(10.0);
    ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for (index, track) in tracks.iter().enumerate() {
                    let item = Shelf {
                        title: &track.title,
                        subtitle: &track.artist,
                        art: track.thumbnail_url.as_deref(),
                        seed: track.id.0,
                        playing: Some(track.id) == playing,
                    };
                    if glass::shelf_card(ui, card, Id::new((id, track.id.0)), &item, c).clicked() {
                        tapped = Some(offset + index);
                    }
                }
            });
        });
    ui.add_space(22.0);
    tapped
}

/// The shelf of the user's playlists. Returns the tapped playlist's index in the library.
fn playlist_shelf(ui: &mut Ui, heading: &str, card: f32, playlists: &[(usize, String, usize, Option<String>)], c: &ColorScheme) -> Option<usize> {
    if playlists.is_empty() {
        return None;
    }
    let mut tapped = None;
    ui.label(theme::text(Role::TitleLarge, heading).color(c.on_surface));
    ui.add_space(10.0);
    ScrollArea::horizontal()
        .id_salt("home-shelf-playlists")
        .auto_shrink([false, true])
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for (index, name, count, art) in playlists {
                    let subtitle = format!("Playlist · {count} songs");
                    let item = Shelf { title: name, subtitle: &subtitle, art: art.as_deref(), seed: *index as u64 * 37 + 11, playing: false };
                    if glass::shelf_card(ui, card, Id::new(("home-playlist", *index)), &item, c).clicked() {
                        tapped = Some(*index);
                    }
                }
            });
        });
    ui.add_space(22.0);
    tapped
}

fn history_row(ui: &mut Ui, saved: &SavedTrack, index: usize, c: &ColorScheme) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let response = m3::pointer_cursor(response, true);
    let painter = ui.painter().clone();
    if response.hovered() {
        painter.rect_filled(rect, Rounding::same(shape::MEDIUM), theme::with_alpha(c.on_surface, theme::state::HOVER));
    }
    let art = Rect::from_min_size(rect.min + vec2(4.0, 6.0), vec2(44.0, 44.0));
    glass::placeholder_art(&painter, art, shape::MEDIUM, index as u64 * 17 + 3, c);
    if let Some(url) = saved.thumbnail_url.as_deref() {
        glass::paint_album_art(ui, url, art, shape::MEDIUM);
    }
    let text_left = art.right() + 12.0;
    let clip = Rect::from_min_max(pos2(text_left, rect.top()), pos2(rect.right() - 4.0, rect.bottom()));
    let text_painter = painter.with_clip_rect(clip.intersect(ui.clip_rect()));
    text_painter.text(pos2(text_left, rect.top() + 10.0), Align2::LEFT_TOP, &saved.title, FontId::proportional(15.0), c.on_surface);
    text_painter.text(pos2(text_left, rect.top() + 31.0), Align2::LEFT_TOP, &saved.artist, FontId::proportional(13.0), c.on_surface_variant);
    response.clicked()
}

/// Seven bars of listening time, oldest to newest, with minutes and a weekly total.
fn activity_chart(ui: &mut Ui, week: &[u32], today: u32, c: &ColorScheme) {
    let total: u32 = week.iter().sum();
    let minutes = |seconds: u32| (seconds + 30) / 60;
    ui.label(theme::text(Role::HeadlineSmall, format!("{} min", minutes(total))).color(c.on_surface));
    ui.label(theme::text(Role::BodyMedium, "listened in the last 7 days").color(c.on_surface_variant));
    ui.add_space(12.0);

    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
    let painter = ui.painter().clone();
    let count = week.len().max(1) as f32;
    let slot = rect.width() / count;
    let bar = (slot * 0.56).min(34.0);
    let most = week.iter().copied().max().unwrap_or(0).max(1) as f32;
    let chart_height = rect.height() - 40.0;
    for (index, seconds) in week.iter().enumerate() {
        let centre = rect.left() + slot * (index as f32 + 0.5);
        let height = (chart_height * *seconds as f32 / most).max(6.0);
        let is_today = index + 1 == week.len();
        let top = rect.top() + 16.0 + chart_height - height;
        let bar_rect = Rect::from_min_max(pos2(centre - bar / 2.0, top), pos2(centre + bar / 2.0, rect.top() + 16.0 + chart_height));
        let colour = if is_today { c.primary } else { glass::mix(c.surface_container_highest, c.primary, 0.45) };
        painter.rect_filled(bar_rect, Rounding::same(bar / 2.0), colour);
        if *seconds > 0 {
            painter.text(pos2(centre, top - 3.0), Align2::CENTER_BOTTOM, minutes(*seconds), FontId::proportional(11.0), c.on_surface_variant);
        }
        let day = today.saturating_sub((week.len() - 1 - index) as u32);
        let weekday = ["M", "T", "W", "T", "F", "S", "S"][((day + 3) % 7) as usize];
        painter.text(
            pos2(centre, rect.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            weekday,
            FontId::proportional(13.0),
            if is_today { c.on_surface } else { c.on_surface_variant },
        );
    }
}
