use eframe::egui::Color32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    Library,
    Settings,
}

#[derive(Clone)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub duration: String,
    pub cover: Color32,
}

impl Song {
    pub fn new(title: &str, artist: &str, duration: &str, cover: Color32) -> Self {
        Self {
            title: title.to_owned(),
            artist: artist.to_owned(),
            duration: duration.to_owned(),
            cover,
        }
    }
}
