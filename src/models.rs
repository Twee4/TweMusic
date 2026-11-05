use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    Library,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: TrackId,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration: Duration,
}

impl Track {
    pub fn new(id: u64, title: &str, artist: &str, album: Option<&str>, seconds: u64) -> Self {
        Self {
            id: TrackId(id),
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: album.map(str::to_owned),
            duration: Duration::from_secs(seconds),
        }
    }

    pub fn duration_label(&self) -> String {
        let total_seconds = self.duration.as_secs();
        format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
    }
}
