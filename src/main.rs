use std::{fs, path::PathBuf, sync::mpsc::{self, Receiver, TryRecvError}, thread, time::{Duration, Instant}};

use eframe::egui::{self, Align, Button, CentralPanel, Color32, Context, Layout, Margin, RichText, ScrollArea, SidePanel, TopBottomPanel};
use twe_music::{models::{Screen, Track}, mpv::{self, MpvError, MpvPlayer}, state::{AppState, PlaybackStatus}, theme, ytmusic};

fn format_seconds(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

struct TweMusicApp {
    state: AppState,
    search_rx: Option<Receiver<Result<Vec<Track>, String>>>,
    search_in_flight: bool,
    mpv: Option<MpvPlayer>,
    local_file_path: String,
    player_message: Option<String>,
    ytdlp_available: Option<bool>,
    last_poll: Instant,
    position: Option<f64>,
    duration: Option<f64>,
    played_something: bool,
    loading_since: Option<Instant>,
}

impl TweMusicApp {
    fn new() -> Self {
        Self {
            state: AppState::new(), search_rx: None, search_in_flight: false,
            mpv: None, local_file_path: String::new(), player_message: None,
            ytdlp_available: None, last_poll: Instant::now(), position: None, duration: None,
            played_something: false, loading_since: None,
        }
    }

    fn ensure_player(&mut self) -> Result<&mut MpvPlayer, MpvError> {
        if self.mpv.is_none() {
            self.mpv = Some(MpvPlayer::start()?);
        }
        Ok(self.mpv.as_mut().expect("player was just started"))
    }

    /// Streams a YouTube Music track's audio through MPV (which resolves it with yt-dlp).
    fn play_remote(&mut self, track: &Track) {
        let Some(url) = track.video_id.as_deref().and_then(ytmusic::youtube_music_url) else {
            self.player_message = Some("YouTube returned an invalid video ID.".to_owned());
            return;
        };
        if !*self.ytdlp_available.get_or_insert_with(mpv::ytdlp_available) {
            self.ytdlp_available = None; // check again next time, in case it was just installed
            self.player_message = Some("yt-dlp is required to play YouTube tracks. Install it (for example `pipx install yt-dlp`) and try again.".to_owned());
            return;
        }
        let result = self.ensure_player().and_then(|player| player.load_url(&url));
        match result {
            Ok(()) => {
                self.state.play_track(track.id);
                self.position = None;
                self.duration = None;
                self.played_something = false;
                self.loading_since = Some(Instant::now());
                self.player_message = Some(format!("Loading {}…", track.title));
            }
            Err(error) => self.player_message = Some(error.to_string()),
        }
    }

    fn skip(&mut self, forward: bool) {
        let Some(id) = self.state.adjacent_remote(forward) else { return; };
        if let Some(track) = self.state.tracks.iter().find(|track| track.id == id).cloned() {
            self.play_remote(&track);
        }
    }

    /// Polls MPV for progress, detects the end of a track, and auto-advances the queue.
    fn tick_player(&mut self, ctx: &Context) {
        if self.state.playback != PlaybackStatus::Playing { return; }
        let interval = Duration::from_millis(500);
        ctx.request_repaint_after(interval);
        if self.last_poll.elapsed() < interval { return; }
        let Some(player) = self.mpv.as_mut() else { return; };
        self.last_poll = Instant::now();
        let position = player.position();
        let duration = player.duration();
        let idle = player.is_idle();

        self.position = position;
        self.duration = duration;
        if position.is_some_and(|value| value > 0.0) {
            if !self.played_something { self.player_message = None; }
            self.played_something = true;
            self.loading_since = None;
        }
        if !idle { return; }
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
            self.player_message = Some("Could not play this track. Try updating yt-dlp (`yt-dlp -U`).".to_owned());
        }
    }

    fn start_search(&mut self, ctx: &Context) {
        let query = self.state.search_query.trim().to_owned();
        if query.is_empty() || self.search_in_flight { return; }
        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);
        self.search_in_flight = true;
        self.state.search_message = Some("Searching YouTube Music…".to_owned());
        let repaint = ctx.clone();
        thread::spawn(move || {
            let result = ytmusic::search_tracks(&query).map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }

    fn poll_search(&mut self) {
        let Some(receiver) = &self.search_rx else { return; };
        match receiver.try_recv() {
            Ok(Ok(tracks)) => {
                let count = tracks.len();
                self.state.add_search_results(tracks);
                self.state.search_message = Some(if count == 0 { "No tracks found." } else { "Search complete." }.to_owned());
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Ok(Err(error)) => {
                self.state.remote_results.clear();
                self.state.search_message = Some(error);
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Err(TryRecvError::Disconnected) => {
                self.state.search_message = Some("Search worker stopped unexpectedly.".to_owned());
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn start_local_playback(&mut self) {
        let input = self.local_file_path.trim();
        if input.is_empty() {
            self.player_message = Some("Enter a local audio file path first.".to_owned());
            return;
        }
        let path = PathBuf::from(input);
        let canonical = match fs::canonicalize(&path) {
            Ok(path) if path.is_file() => path,
            Ok(_) => { self.player_message = Some("The selected path is not a file.".to_owned()); return; }
            Err(error) => { self.player_message = Some(format!("Cannot open file: {error}")); return; }
        };
        if let Some(player) = &mut self.mpv {
            match player.load_file(&canonical) {
                Ok(()) => {
                    self.state.playback = PlaybackStatus::Playing;
                    self.player_message = Some(format!("Playing {}", canonical.display()));
                }
                Err(error) => self.player_message = Some(error.to_string()),
            }
        } else {
            match MpvPlayer::start().and_then(|mut player| { player.load_file(&canonical)?; Ok(player) }) {
                Ok(player) => {
                    self.mpv = Some(player);
                    self.state.playback = PlaybackStatus::Playing;
                    self.player_message = Some(format!("Playing {}", canonical.display()));
                }
                Err(error) => self.player_message = Some(error.to_string()),
            }
        }
    }

    fn toggle_local_playback(&mut self) {
        let Some(player) = &mut self.mpv else {
            self.player_message = Some("Choose a local audio file in Settings to start playback.".to_owned());
            return;
        };
        match player.toggle_pause() {
            Ok(paused) => {
                self.state.playback = if paused { PlaybackStatus::Paused } else { PlaybackStatus::Playing };
                self.player_message = Some(if paused { "Paused" } else { "Playing" }.to_owned());
            }
            Err(error) => self.player_message = Some(error.to_string()),
        }
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, title: &str, screen: Screen) {
        let active = self.state.screen == screen;
        let text_color = if active { theme::ON_PRIMARY_CONTAINER } else { theme::ON_SURFACE_VARIANT };
        let response = ui.add_sized(
            [ui.available_width(), 44.0],
            Button::new(RichText::new(title).size(15.0).color(text_color))
                .fill(if active { theme::PRIMARY_CONTAINER } else { Color32::TRANSPARENT }),
        );
        if response.clicked() { self.state.screen = screen; }
    }

    fn track_row(&mut self, ui: &mut egui::Ui, track: &Track, index: usize) {
        let video_id = track.video_id.clone();
        let mut open_error = None;
        let mut play_requested = false;
        egui::Frame::group(ui.style())
            .fill(theme::SURFACE_CONTAINER)
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let color = theme::COVER_COLORS[index % theme::COVER_COLORS.len()];
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 10.0, color);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "♪", egui::FontId::proportional(24.0), Color32::WHITE);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&track.title).strong().size(15.0));
                        ui.label(RichText::new(&track.artist).color(theme::ON_SURFACE_VARIANT));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(video_id) = &video_id {
                            if ui.button("Play").clicked() { play_requested = true; }
                            if ui.button("Open on web").clicked() {
                                match ytmusic::youtube_music_url(video_id) {
                                    Some(url) => if let Err(error) = webbrowser::open(&url) {
                                        open_error = Some(format!("Could not open YouTube Music: {error}"));
                                    },
                                    None => open_error = Some("YouTube returned an invalid video ID.".to_owned()),
                                }
                            }
                        } else {
                            ui.add_enabled(false, Button::new("Demo track"));
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new(track.duration_label()).color(theme::ON_SURFACE_VARIANT));
                    });
                });
            });
        if let Some(error) = open_error { self.player_message = Some(error); }
        if play_requested { self.play_remote(track); }
        ui.add_space(8.0);
    }

    fn page_intro(ui: &mut egui::Ui, eyebrow: &str, title: &str, description: &str) {
        ui.label(RichText::new(eyebrow.to_uppercase()).size(11.0).strong().color(theme::PRIMARY));
        ui.add_space(4.0);
        ui.heading(RichText::new(title).size(30.0));
        ui.add_space(4.0);
        ui.label(RichText::new(description).color(theme::ON_SURFACE_VARIANT));
        ui.add_space(22.0);
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        Self::page_intro(ui, "Your soundtrack", "Good evening", "Find something lovely to listen to.");
        ui.horizontal(|ui| {
            ui.heading("Quick picks");
            ui.add_space(8.0);
            ui.label(RichText::new("A few tracks to get you started").color(theme::ON_SURFACE_VARIANT));
        });
        ui.add_space(12.0);
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_search(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        Self::page_intro(ui, "Explore", "Search music", "Discover songs and artists on YouTube Music.");
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 112.0).max(180.0);
            let response = ui.add_sized([width, 44.0], egui::TextEdit::singleline(&mut self.state.search_query)
                .hint_text("Try a song, artist, or album"));
            let submitted = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let clicked = ui.add_sized([100.0, 44.0], Button::new("Search").fill(theme::PRIMARY_CONTAINER)).clicked();
            if submitted || clicked { self.start_search(ctx); }
        });
        ui.add_space(12.0);
        if let Some(message) = &self.state.search_message {
            ui.label(RichText::new(message).color(theme::ON_SURFACE_VARIANT));
            ui.add_space(8.0);
        }
        let results = self.state.remote_results.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in results.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_library(&mut self, ui: &mut egui::Ui) {
        Self::page_intro(ui, "Collection", "Your library", "Your saved music, all in one place.");
        egui::Frame::group(ui.style()).fill(theme::SURFACE_CONTAINER).inner_margin(Margin::same(18.0)).show(ui, |ui| {
            ui.label(RichText::new("Your library is waiting").strong().size(17.0));
            ui.add_space(5.0);
            ui.label(RichText::new("Saved music will appear here when an account is connected.").color(theme::ON_SURFACE_VARIANT));
        });
        ui.add_space(22.0);
        ui.heading("Demo tracks");
        ui.add_space(10.0);
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        Self::page_intro(ui, "Preferences", "Settings", "Manage playback and learn how TweMusic works.");
        egui::Frame::group(ui.style()).fill(theme::SURFACE_CONTAINER).inner_margin(Margin::same(18.0)).show(ui, |ui| {
            ui.label(RichText::new("Local audio playback").strong().size(17.0));
            ui.add_space(4.0);
            ui.label(RichText::new("Play a file from your device using MPV.").color(theme::ON_SURFACE_VARIANT));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_sized([ui.available_width() - 110.0, 40.0], egui::TextEdit::singleline(&mut self.local_file_path).hint_text("/path/to/audio-file"));
                if ui.add_sized([100.0, 40.0], Button::new("Play file").fill(theme::PRIMARY_CONTAINER)).clicked() { self.start_local_playback(); }
            });
            if let Some(message) = &self.player_message {
                ui.add_space(8.0);
                ui.label(RichText::new(message).color(theme::ON_SURFACE_VARIANT));
            }
        });
        ui.add_space(12.0);
        egui::Frame::group(ui.style()).fill(theme::SURFACE_CONTAINER).inner_margin(Margin::same(18.0)).show(ui, |ui| {
            ui.label(RichText::new("YouTube Music").strong().size(17.0));
            ui.add_space(4.0);
            ui.label(RichText::new("Search returns public metadata. Play streams the audio only, through MPV and yt-dlp, so no video or ads load.").color(theme::ON_SURFACE_VARIANT));
            ui.add_space(4.0);
            ui.label(RichText::new("Requires mpv and yt-dlp. This may go against YouTube's terms of service, and yt-dlp needs regular updates.").color(theme::ON_SURFACE_VARIANT));
        });
    }

    fn render_player(&mut self, ctx: &Context) {
        let (title, subtitle) = match self.state.current_track() {
            Some(track) if self.mpv.is_some() => (track.title.clone(), track.artist.clone()),
            _ if self.mpv.is_some() => ("Local audio player".to_owned(), "MPV".to_owned()),
            _ => ("Nothing playing".to_owned(), "Ready for your next song".to_owned()),
        };
        let status = self.player_message.clone().unwrap_or(subtitle);
        let progress = match (self.position, self.duration) {
            (Some(position), Some(duration)) if duration > 0.0 => Some((position, duration)),
            _ => None,
        };
        let can_prev = self.state.adjacent_remote(false).is_some();
        let can_next = self.state.adjacent_remote(true).is_some();
        let playing = self.state.playback == PlaybackStatus::Playing;
        TopBottomPanel::bottom("player-panel")
            .frame(egui::Frame::none().fill(theme::SURFACE_CONTAINER).inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 9.0, theme::PRIMARY_CONTAINER);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "♪", egui::FontId::proportional(21.0), theme::ON_PRIMARY_CONTAINER);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).strong());
                        ui.label(RichText::new(status).size(12.0).color(theme::ON_SURFACE_VARIANT));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_enabled(can_next, Button::new("Next")).clicked() { self.skip(true); }
                        if ui.add_sized([100.0, 38.0], Button::new(if playing { "Pause" } else { "Play" }).fill(theme::PRIMARY_CONTAINER)).clicked() {
                            self.toggle_local_playback();
                        }
                        if ui.add_enabled(can_prev, Button::new("Prev")).clicked() { self.skip(false); }
                        if let Some((position, duration)) = progress {
                            ui.add_space(12.0);
                            ui.label(format!("{} / {}", format_seconds(position), format_seconds(duration)));
                            ui.add(egui::ProgressBar::new((position / duration).clamp(0.0, 1.0) as f32).desired_width(160.0));
                        }
                    });
                });
            });
    }
}

impl eframe::App for TweMusicApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_search();
        self.tick_player(ctx);
        ctx.set_visuals(theme::dark_visuals());
        TopBottomPanel::top("top-bar")
            .frame(egui::Frame::none().fill(theme::SURFACE).inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 10.0, theme::PRIMARY_CONTAINER);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "♪", egui::FontId::proportional(20.0), theme::ON_PRIMARY_CONTAINER);
                    ui.add_space(8.0);
                    ui.label(RichText::new("TweMusic").size(20.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("SEARCH · DISCOVER · LISTEN").size(11.0).strong().color(theme::ON_SURFACE_VARIANT));
                    });
                });
            });
        SidePanel::left("navigation").resizable(false).default_width(196.0)
            .frame(egui::Frame::none().fill(theme::SURFACE).inner_margin(Margin::same(14.0))).show(ctx, |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("MENU").size(11.0).strong().color(theme::ON_SURFACE_VARIANT));
                ui.add_space(10.0);
                self.nav_button(ui, "⌂   Home", Screen::Home);
                ui.add_space(4.0);
                self.nav_button(ui, "⌕   Search", Screen::Search);
                ui.add_space(4.0);
                self.nav_button(ui, "♫   Library", Screen::Library);
                ui.add_space(4.0);
                self.nav_button(ui, "⚙   Settings", Screen::Settings);
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(RichText::new("Made for listening").size(12.0).color(theme::ON_SURFACE_VARIANT));
                    ui.add_space(4.0);
                });
            });
        CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BACKGROUND).inner_margin(Margin::same(26.0)))
            .show(ctx, |ui| match self.state.screen {
                Screen::Home => self.render_home(ui),
                Screen::Search => self.render_search(ui, ctx),
                Screen::Library => self.render_library(ui),
                Screen::Settings => self.render_settings(ui),
            });
        self.render_player(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 520.0]).with_title("TweMusic"),
        ..Default::default()
    };
    eframe::run_native("TweMusic", options, Box::new(|_cc| Ok(Box::new(TweMusicApp::new()))))
}
