use eframe::egui::{self, Align, Button, CentralPanel, Color32, Context, Layout, Margin, RichText, ScrollArea, SidePanel, TopBottomPanel};
use twe_music::{models::{Screen, Track}, state::{AppState, PlaybackStatus}, theme};

struct TweMusicApp {
    state: AppState,
}

impl TweMusicApp {
    fn new() -> Self {
        Self { state: AppState::new() }
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, title: &str, screen: Screen) {
        let active = self.state.screen == screen;
        let response = ui.add_sized(
            [160.0, 42.0],
            Button::new(RichText::new(title).size(15.0))
                .fill(if active { theme::PRIMARY } else { Color32::TRANSPARENT }),
        );
        if response.clicked() {
            self.state.screen = screen;
        }
    }

    fn track_row(&mut self, ui: &mut egui::Ui, track: &Track, index: usize) {
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
                if ui.button("Play").clicked() {
                    self.state.play_track(track.id);
                }
                ui.label(track.duration_label());
            });
        });
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
            for (index, track) in tracks.iter().enumerate() {
                self.track_row(ui, track, index);
            }
        });
    }

    fn render_search(&mut self, ui: &mut egui::Ui) {
        ui.heading("Search");
        ui.add_space(12.0);
        ui.add_sized(
            [ui.available_width(), 42.0],
            egui::TextEdit::singleline(&mut self.state.search_query)
                .hint_text("Search songs, artists, albums"),
        );
        ui.add_space(18.0);
        let matches = self.state.search_results();
        if self.state.search_query.trim().is_empty() {
            ui.label(RichText::new("Try a song or artist name").color(theme::ON_SURFACE_VARIANT));
        } else if matches.is_empty() {
            ui.label(RichText::new("No matching tracks in the demo catalog.").color(theme::ON_SURFACE_VARIANT));
        } else {
            ui.label(format!("{} results", matches.len()));
            ui.add_space(8.0);
            let tracks: Vec<Track> = matches.into_iter().cloned().collect();
            ScrollArea::vertical().show(ui, |ui| {
                for (index, track) in tracks.iter().enumerate() {
                    self.track_row(ui, track, index);
                }
            });
        }
    }

    fn render_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your library");
        ui.add_space(8.0);
        ui.label(RichText::new("Saved music will appear here when an account is connected.").color(theme::ON_SURFACE_VARIANT));
        ui.add_space(16.0);
        ui.heading("Demo tracks");
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() {
                self.track_row(ui, track, index);
            }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(16.0);
        ui.group(|ui| {
            ui.set_min_width(360.0);
            ui.label(RichText::new("Appearance").strong());
            ui.add_space(8.0);
            ui.label("Material 3-inspired dark theme");
        });
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.set_min_width(360.0);
            ui.label(RichText::new("Account").strong());
            ui.add_space(8.0);
            ui.label("YouTube Music connection is not implemented yet.");
            ui.label("No credentials are stored by this prototype.");
        });
    }

    fn render_player(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("player-panel").show(ctx, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if let Some(track) = self.state.current_track().cloned() {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(track.title).strong());
                        ui.label(RichText::new(track.artist).color(theme::ON_SURFACE_VARIANT));
                    });
                } else {
                    ui.label(RichText::new("Nothing playing").color(theme::ON_SURFACE_VARIANT));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Next").clicked() {
                        self.state.next_track();
                    }
                    let play_label = if self.state.playback == PlaybackStatus::Playing { "Pause" } else { "Play" };
                    if ui.button(play_label).clicked() {
                        self.state.toggle_playback();
                    }
                    if ui.button("Previous").clicked() {
                        self.state.previous_track();
                    }
                });
            });
            ui.add_space(5.0);
        });
    }
}

impl eframe::App for TweMusicApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(theme::dark_visuals());
        TopBottomPanel::top("top-bar")
            .frame(egui::Frame::none().inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("TweMusic");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Rust desktop prototype").color(theme::ON_SURFACE_VARIANT));
                    });
                });
            });
        SidePanel::left("navigation").resizable(false).default_width(184.0)
            .frame(egui::Frame::none().inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                self.nav_button(ui, "Home", Screen::Home);
                self.nav_button(ui, "Search", Screen::Search);
                self.nav_button(ui, "Library", Screen::Library);
                self.nav_button(ui, "Settings", Screen::Settings);
            });
        CentralPanel::default().show(ctx, |ui| match self.state.screen {
            Screen::Home => self.render_home(ui),
            Screen::Search => self.render_search(ui),
            Screen::Library => self.render_library(ui),
            Screen::Settings => self.render_settings(ui),
        });
        self.render_player(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 520.0])
            .with_title("TweMusic"),
        ..Default::default()
    };
    eframe::run_native("TweMusic", options, Box::new(|_cc| Ok(Box::new(TweMusicApp::new()))))
}
