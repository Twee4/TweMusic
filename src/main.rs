use std::{fs, path::PathBuf, sync::mpsc::{self, Receiver, TryRecvError}, thread};

use eframe::egui::{self, Align, Button, CentralPanel, Color32, Context, Layout, Margin, RichText, ScrollArea, SidePanel, TopBottomPanel};
use twe_music::{models::{Screen, Track}, mpv::MpvPlayer, state::{AppState, PlaybackStatus}, theme, ytmusic};

struct TweMusicApp {
    state: AppState,
    search_rx: Option<Receiver<Result<Vec<Track>, String>>>,
    search_in_flight: bool,
    mpv: Option<MpvPlayer>,
    local_file_path: String,
    player_message: Option<String>,
}

impl TweMusicApp {
    fn new() -> Self {
        Self {
            state: AppState::new(), search_rx: None, search_in_flight: false,
            mpv: None, local_file_path: String::new(), player_message: None,
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
        let response = ui.add_sized([160.0, 42.0], Button::new(RichText::new(title).size(15.0))
            .fill(if active { theme::PRIMARY } else { Color32::TRANSPARENT }));
        if response.clicked() { self.state.screen = screen; }
    }

    fn track_row(&mut self, ui: &mut egui::Ui, track: &Track, index: usize) {
        let video_id = track.video_id.clone();
        let mut open_error = None;
        ui.horizontal(|ui| {
            let color = theme::COVER_COLORS[index % theme::COVER_COLORS.len()];
            let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, color);
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(&track.title).strong());
                ui.label(RichText::new(&track.artist).color(theme::ON_SURFACE_VARIANT));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(video_id) = &video_id {
                    if ui.button("Open in YouTube Music").clicked() {
                        match ytmusic::youtube_music_url(video_id) {
                            Some(url) => if let Err(error) = webbrowser::open(&url) {
                                open_error = Some(format!("Could not open YouTube Music: {error}"));
                            },
                            None => open_error = Some("YouTube returned an invalid video ID.".to_owned()),
                        }
                    }
                } else if ui.button("Queue demo track").clicked() {
                    self.state.play_track(track.id);
                }
                ui.label(track.duration_label());
            });
        });
        if let Some(error) = open_error { self.player_message = Some(error); }
        ui.add_space(8.0);
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Good evening");
        ui.label(RichText::new("A little music for your day").color(theme::ON_SURFACE_VARIANT));
        ui.add_space(18.0);
        ui.heading("Quick picks");
        ui.add_space(8.0);
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_search(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.heading("Search YouTube Music");
        ui.add_space(12.0);
        let response = ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(&mut self.state.search_query)
            .hint_text("Search songs and artists"));
        let submitted = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let clicked = ui.add(Button::new("Search").fill(theme::PRIMARY)).clicked();
        if submitted || clicked { self.start_search(ctx); }
        ui.add_space(12.0);
        if let Some(message) = &self.state.search_message {
            ui.label(RichText::new(message).color(theme::ON_SURFACE_VARIANT));
        }
        let results = self.state.remote_results.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in results.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your library");
        ui.add_space(8.0);
        ui.label(RichText::new("Saved music will appear here when an account is connected.").color(theme::ON_SURFACE_VARIANT));
        ui.add_space(16.0);
        ui.heading("Demo tracks");
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() { self.track_row(ui, track, index); }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(16.0);
        ui.group(|ui| {
            ui.set_min_width(420.0);
            ui.label(RichText::new("Local audio playback").strong());
            ui.add_space(8.0);
            ui.label("MPV is used to play files on your device.");
            ui.horizontal(|ui| {
                ui.add_sized([320.0, 36.0], egui::TextEdit::singleline(&mut self.local_file_path).hint_text("/path/to/audio-file"));
                if ui.button("Play file").clicked() { self.start_local_playback(); }
            });
            if let Some(message) = &self.player_message {
                ui.label(RichText::new(message).color(theme::ON_SURFACE_VARIANT));
            }
        });
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.set_min_width(420.0);
            ui.label(RichText::new("YouTube Music").strong());
            ui.add_space(8.0);
            ui.label("Search returns public metadata. Open a result to listen on music.youtube.com.");
            ui.label("The app does not extract or download YouTube audio streams.");
        });
    }

    fn render_player(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("player-panel").show(ctx, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(if self.mpv.is_some() { "Local audio player" } else { "Nothing playing" }).strong());
                    ui.label(RichText::new("MPV · local files").color(theme::ON_SURFACE_VARIANT));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Play / Pause").clicked() { self.toggle_local_playback(); }
                });
            });
            ui.add_space(5.0);
        });
    }
}

impl eframe::App for TweMusicApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_search();
        ctx.set_visuals(theme::dark_visuals());
        TopBottomPanel::top("top-bar").frame(egui::Frame::none().inner_margin(Margin::same(12.0))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("TweMusic");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Search and browser playback").color(theme::ON_SURFACE_VARIANT));
                });
            });
        });
        SidePanel::left("navigation").resizable(false).default_width(184.0)
            .frame(egui::Frame::none().inner_margin(Margin::same(12.0))).show(ctx, |ui| {
                ui.add_space(8.0);
                self.nav_button(ui, "Home", Screen::Home);
                self.nav_button(ui, "Search", Screen::Search);
                self.nav_button(ui, "Library", Screen::Library);
                self.nav_button(ui, "Settings", Screen::Settings);
            });
        CentralPanel::default().show(ctx, |ui| match self.state.screen {
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
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 520.0]).with_title("TweMusic"),
        ..Default::default()
    };
    eframe::run_native("TweMusic", options, Box::new(|_cc| Ok(Box::new(TweMusicApp::new()))))
}
