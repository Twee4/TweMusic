use eframe::{App, Frame, NativeOptions};
use eframe::egui::{
    self, Align, Button, CentralPanel, Color32, Context, Layout, Margin, RichText, ScrollArea,
    SidePanel, TopBottomPanel,
};

mod models;
mod theme;

use models::{Screen, Song};
use theme::{dark_visuals, COLORS};

struct TweMusicApp {
    screen: Screen,
    search_query: String,
    songs: Vec<Song>,
}

impl TweMusicApp {
    fn new() -> Self {
        Self {
            screen: Screen::Home,
            search_query: String::new(),
            songs: vec![
                Song::new("Midnight Echo", "Nova Lane", "3:42", COLORS.primary),
                Song::new("Electric Bloom", "Iris City", "4:15", COLORS.secondary),
                Song::new("Silver Skyline", "Red Harbor", "2:58", COLORS.tertiary),
                Song::new("Night Shift", "Luna Kestrel", "5:07", COLORS.accent),
                Song::new("Golden Thread", "Northline", "3:26", COLORS.success),
                Song::new("Moss and Neon", "Velvet Echo", "4:42", COLORS.warn),
            ],
        }
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, label: &str, target: Screen) {
        let active = self.screen == target;

        let response = ui.add_sized(
            [160.0, 38.0],
            Button::new(RichText::new(label).size(15.0)).fill(if active { COLORS.primary } else { Color32::TRANSPARENT }),
        );

        if response.clicked() {
            self.screen = target;
        }
    }

    fn render_topbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("TweMusic");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_sized(
                        [110.0, 38.0],
                        Button::new(RichText::new("Sign in").size(14.0).strong()).fill(COLORS.primary),
                    )
                    .clicked()
                {
                    self.screen = Screen::Settings;
                }
            });
        });
    }

    fn render_nav(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(12.0);
            self.nav_button(ui, "Home", Screen::Home);
            self.nav_button(ui, "Search", Screen::Search);
            self.nav_button(ui, "Library", Screen::Library);
            self.nav_button(ui, "Settings", Screen::Settings);
        });
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Good evening");
        ui.add_space(12.0);

        egui::Grid::new("home-grid")
            .spacing((16.0, 16.0))
            .min_col_width(220.0)
            .show(ui, |ui| {
                for song in &self.songs {
                    let response = ui.allocate_ui(vec2(220.0, 120.0), |ui| {
                        ui.vertical(|ui| {
                            ui.allocate_ui(vec2(220.0, 80.0), |ui| {
                                ui.painter().rect_filled(ui.max_rect().shrink(4.0), 16.0, song.cover);
                            });
                            ui.add_space(8.0);
                            ui.label(RichText::new(song.title.clone()).size(18.0).strong());
                            ui.label(RichText::new(song.artist.clone()).size(12.0).color(Color32::from_rgb(186, 190, 200)));
                        });
                    });
                    if response.response.clicked() {
                        // placeholder for future playback selection
                    }
                    ui.end_row();
                }
            });
    }

    fn render_search(&mut self, ui: &mut egui::Ui) {
        ui.heading("Search");
        ui.add_space(12.0);
        ui.add_sized(
            [ui.available_width(), 42.0],
            egui::TextEdit::singleline(&mut self.search_query).hint_text("Search songs, artists, albums"),
        );

        ui.add_space(18.0);

        if self.search_query.trim().is_empty() {
            ui.label(RichText::new("Trending now").size(14.0).color(Color32::from_rgb(188, 193, 206)));
        } else {
            ui.label(RichText::new(format!("Results for: \"{}\"", self.search_query)).size(14.0).color(Color32::from_rgb(188, 193, 206)));
        }
    }

    fn render_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your library");
        ui.add_space(14.0);

        ScrollArea::vertical().show(ui, |ui| {
            for song in &self.songs {
                ui.horizontal(|ui| {
                    ui.allocate_ui(vec2(48.0, 48.0), |ui| {
                        ui.painter().rect_filled(ui.max_rect().shrink(4.0), 12.0, song.cover);
                    });
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(song.title.clone()).strong());
                        ui.label(RichText::new(song.artist.clone()).color(Color32::from_rgb(182, 188, 200)));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(song.duration.clone());
                    });
                });
                ui.add_space(8.0);
            }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(14.0);

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Appearance").strong());
                ui.add_space(8.0);
                ui.checkbox(&mut true, "Dark mode");
                ui.checkbox(&mut true, "Material 3 accent colors");
            });
        });

        ui.add_space(14.0);

        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Account").strong());
                ui.add_space(8.0);
                ui.label("YouTube Music cookie auth will be added in the next stage.");
                ui.label("Storage: ~/.config/TweMusic");
            });
        });
    }
}

impl App for TweMusicApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        ctx.set_visuals(dark_visuals());

        TopBottomPanel::top("topbar-panel")
            .frame(egui::Frame::none().inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| self.render_topbar(ui));

        SidePanel::left("navigation-panel")
            .default_width(180.0)
            .resizable(false)
            .frame(egui::Frame::none().inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| self.render_nav(ui));

        CentralPanel::default().show(ctx, |ui| match self.screen {
            Screen::Home => self.render_home(ui),
            Screen::Search => self.render_search(ui),
            Screen::Library => self.render_library(ui),
            Screen::Settings => self.render_settings(ui),
        });

        TopBottomPanel::bottom("player-panel")
            .exact_height(86.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(vec2(52.0, 52.0), |ui| {
                            ui.painter().rect_filled(ui.max_rect().shrink(4.0), 15.0, COLORS.primary);
                        });
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Midnight Echo").size(16.0).strong());
                            ui.label(RichText::new("Nova Lane").size(12.0).color(Color32::from_rgb(180, 186, 203)));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized([36.0, 36.0], Button::new("⏮").min_size(vec2(36.0, 36.0)).rounding(18.0));
                                ui.add_sized([42.0, 42.0], Button::new("⏸").min_size(vec2(42.0, 42.0)).fill(COLORS.primary).rounding(18.0));
                                ui.add_sized([36.0, 36.0], Button::new("⏭").min_size(vec2(36.0, 36.0)).rounding(18.0));
                            });
                        });
                    });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_title("TweMusic"),
        ..Default::default()
    };

    eframe::run_native("TweMusic", native_options, Box::new(|_cc| Ok(Box::new(TweMusicApp::new()))))
}
