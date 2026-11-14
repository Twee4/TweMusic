//! The Home screen: a glass-and-aurora take on YouTube Music's home, with mood chips, a
//! paged Speed dial, circular Mixes and a big Shuffle button.

use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use eframe::egui::{
    self, pos2, scroll_area::ScrollBarVisibility, vec2, Align, Align2, FontId, Id, Layout, Margin, Rect,
    Rounding, ScrollArea, Sense, Ui,
};
use twe_music::{
    glass::{self, Tile},
    icons::Icon,
    library::{self, SavedTrack},
    m3::{self, IconButtonKind},
    models::{Screen, Track},
    state::PlaybackStatus,
    theme::{self, motion, shape, ColorScheme, Role},
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

enum MixSource {
    Playlist(usize),
    Recent,
}

struct Mix {
    title: String,
    subtitle: String,
    art: Option<String>,
    seed: u64,
    source: MixSource,
}

/// Everything the Home screen remembers between frames.
#[derive(Default)]
pub(crate) struct HomeState {
    mood: Option<Mood>,
    moods: HashMap<Mood, Vec<Track>>,
    loading: Option<(Mood, Receiver<Result<Vec<Track>, String>>)>,
    page: usize,
    /// Horizontal pixels the Speed dial has been dragged since the finger went down.
    drag: f32,
    sheet: Option<Sheet>,
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
            self.home.page = 0;
            return;
        }
        self.home.mood = Some(mood);
        self.home.page = 0;
        self.home.drag = 0.0;
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

    fn mixes(&self) -> Vec<Mix> {
        let mut mixes = Vec::new();
        for (index, playlist) in self.library.playlists.iter().enumerate() {
            if playlist.tracks.is_empty() {
                continue;
            }
            mixes.push(Mix {
                title: playlist.name.clone(),
                subtitle: format!("{} songs", playlist.tracks.len()),
                art: playlist.tracks.iter().find_map(|saved| saved.thumbnail_url.clone()),
                seed: index as u64 * 37 + 11,
                source: MixSource::Playlist(index),
            });
        }
        if !self.library.history.is_empty() {
            mixes.push(Mix {
                title: "Recently played".to_owned(),
                subtitle: format!("{} songs", self.library.history.len()),
                art: self.library.history.iter().find_map(|saved| saved.thumbnail_url.clone()),
                seed: 73,
                source: MixSource::Recent,
            });
        }
        mixes
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

    pub(crate) fn render_home(&mut self, ui: &mut Ui) {
        let c = theme::colors(ui);
        let mut picked_mood = None;
        let mut play_dial = None;
        let mut play_mix = None;
        let mut shuffle = false;
        let mut open_sheet = None;
        let mut open_settings = false;

        ScrollArea::vertical().auto_shrink([false, false]).id_salt("home-scroll").show(ui, |ui| {
            let width = ui.available_width().min(820.0);
            ui.set_max_width(width);
            ui.add_space(4.0);

            // Header: the title, and what lives behind the little round buttons.
            ui.allocate_ui_with_layout(vec2(width, 52.0), Layout::left_to_right(Align::Center), |ui| {
                ui.label(theme::text(Role::HeadlineLarge, "Home").color(c.on_surface));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::User, false, "Profile and settings").clicked() {
                        open_settings = true;
                    }
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::Activity, false, "Listening activity").clicked() {
                        open_sheet = Some(Sheet::Activity);
                    }
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::History, false, "Recently played").clicked() {
                        open_sheet = Some(Sheet::History);
                    }
                });
            });
            ui.add_space(8.0);

            // Mood chips.
            ScrollArea::horizontal()
                .id_salt("home-moods")
                .auto_shrink([false, true])
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for mood in Mood::ALL {
                            let selected = self.home.mood == Some(mood);
                            let id = Id::new(("home-mood", mood.label()));
                            if glass::mood_chip(ui, id, mood.icon(), mood.label(), selected, &c).clicked() {
                                picked_mood = Some(mood);
                            }
                        }
                    });
                });
            ui.add_space(22.0);

            // Speed dial.
            ui.horizontal(|ui| {
                ui.label(theme::text(Role::TitleLarge, "Speed dial").color(c.on_surface));
                if let Some(mood) = self.home.mood {
                    ui.label(theme::text(Role::BodyMedium, format!("· {}", mood.label())).color(c.on_surface_variant));
                }
                if self.home.loading.is_some() {
                    m3::loading_indicator(ui, 22.0);
                }
            });
            ui.add_space(8.0);
            let tracks = self.dial_tracks();
            if tracks.is_empty() {
                self.empty_dial(ui, width, &c);
            } else {
                play_dial = self.speed_dial(ui, width, &tracks, &c);
            }
            ui.add_space(24.0);

            // Mixes.
            ui.label(theme::text(Role::TitleLarge, "Mixes").color(c.on_surface));
            ui.add_space(8.0);
            let mixes = self.mixes();
            if mixes.is_empty() {
                ui.label(theme::text(Role::BodyMedium, "Like a song or make a playlist and your mixes appear here.").color(c.on_surface_variant));
            } else {
                let diameter = if width < 420.0 { 104.0 } else { 128.0 };
                ScrollArea::horizontal()
                    .id_salt("home-mixes")
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 16.0;
                            for (index, mix) in mixes.iter().enumerate() {
                                let id = Id::new(("home-mix", index));
                                let response = glass::mix_disc(ui, diameter, id, &mix.title, &mix.subtitle, mix.art.as_deref(), mix.seed, &c);
                                if response.clicked() {
                                    play_mix = Some(index);
                                }
                            }
                        });
                    });
            }
            ui.add_space(24.0);

            if glass::shuffle_button(ui, Id::new("home-shuffle"), "Shuffle", &c).clicked() {
                shuffle = true;
            }
            ui.add_space(32.0);
        });

        if let Some(mood) = picked_mood {
            self.select_mood(mood);
        }
        if let Some(index) = play_dial {
            let tracks = self.dial_tracks();
            self.play_from(tracks, index);
        }
        if let Some(index) = play_mix {
            let mixes = self.mixes();
            match mixes.get(index).map(|mix| &mix.source) {
                Some(MixSource::Playlist(playlist)) => self.play_playlist(*playlist),
                Some(MixSource::Recent) => {
                    let tracks: Vec<Track> = self.library.history.iter().map(SavedTrack::to_track).collect();
                    self.play_from(tracks, 0);
                }
                None => {}
            }
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

    fn empty_dial(&self, ui: &mut Ui, width: f32, c: &ColorScheme) {
        let dark = ui.visuals().dark_mode;
        let message = if self.home.loading.is_some() {
            "Finding songs…"
        } else {
            "Play something and it lands here, or pick a mood above."
        };
        glass::glass_frame(dark, shape::EXTRA_LARGE)
            .inner_margin(Margin::same(20.0))
            .show(ui, |ui| {
                ui.set_width(width - 40.0);
                ui.label(theme::text(Role::BodyLarge, message).color(c.on_surface_variant));
            });
    }

    /// The paged 3×2 grid. Returns the song that was tapped.
    fn speed_dial(&mut self, ui: &mut Ui, width: f32, tracks: &[Track], c: &ColorScheme) -> Option<usize> {
        const PER_PAGE: usize = 6;
        let gap = 12.0;
        let tile = glass::tile_size(width, 3, gap);
        let pages = tracks.len().div_ceil(PER_PAGE).max(1);
        self.home.page = self.home.page.min(pages - 1);

        let (pager, drag) = ui.allocate_exact_size(vec2(width, tile.y * 2.0 + gap), Sense::drag());
        if drag.dragged() {
            self.home.drag += drag.drag_delta().x;
        }
        if drag.drag_stopped() {
            let threshold = width * 0.18;
            if self.home.drag < -threshold && self.home.page + 1 < pages {
                self.home.page += 1;
            } else if self.home.drag > threshold && self.home.page > 0 {
                self.home.page -= 1;
            }
            self.home.drag = 0.0;
        }
        let target = (self.home.page as f32 - self.home.drag / width).clamp(-0.25, pages as f32 - 0.75);
        let spring = if drag.dragged() { motion::SPATIAL_FAST } else { motion::SPATIAL_DEFAULT };
        let position = m3::spring(ui, Id::new("home-dial-position"), target, spring);

        let playing_id = if self.state.playback == PlaybackStatus::Playing { self.state.current_track } else { None };
        let old_clip = ui.clip_rect();
        ui.set_clip_rect(pager.intersect(old_clip));
        let mut tapped = None;
        for page in 0..pages {
            let offset = (page as f32 - position) * (width + gap);
            if offset.abs() >= width + gap {
                continue;
            }
            for slot in 0..PER_PAGE {
                let index = page * PER_PAGE + slot;
                let Some(track) = tracks.get(index) else { break };
                let (row, column) = ((slot / 3) as f32, (slot % 3) as f32);
                let min = pager.min + vec2(offset + column * (tile.x + gap), row * (tile.y + gap));
                let rect = Rect::from_min_size(min, tile);
                let data = Tile {
                    title: &track.title,
                    art: track.thumbnail_url.as_deref(),
                    seed: track.id.0,
                    playing: playing_id == Some(track.id),
                };
                if glass::speed_tile(ui, rect, Id::new(("home-tile", track.id.0)), &data, c).clicked() {
                    tapped = Some(index);
                }
            }
        }
        ui.set_clip_rect(old_clip);

        ui.add_space(6.0);
        if let Some(page) = glass::dots(ui, Id::new("home-dots"), pages, position, c) {
            self.home.page = page;
            self.home.drag = 0.0;
        }
        tapped
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
