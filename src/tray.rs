//! System tray icon (StatusNotifierItem) with playback controls.
//!
//! The tray runs on its own thread. Menu clicks are sent to the app as [`TrayCommand`]s and
//! the window is woken up so it handles them right away.

use std::sync::mpsc::Sender;

use ksni::{blocking::{Handle, TrayMethods}, menu::StandardItem, MenuItem, Tray};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    PlayPause,
    Next,
    Previous,
    Quit,
}

pub struct TweTray {
    sender: Sender<TrayCommand>,
    wake: Box<dyn Fn() + Send + Sync>,
    playing: bool,
    now_playing: String,
}

impl TweTray {
    fn send(&self, command: TrayCommand) {
        if self.sender.send(command).is_ok() {
            (self.wake)();
        }
    }
}

fn item(label: &str, icon: &str, command: TrayCommand) -> MenuItem<TweTray> {
    StandardItem {
        label: label.into(),
        icon_name: icon.into(),
        activate: Box::new(move |tray: &mut TweTray| tray.send(command)),
        ..Default::default()
    }
    .into()
}

impl Tray for TweTray {
    fn id(&self) -> String {
        "twe_music".into()
    }

    fn title(&self) -> String {
        if self.now_playing.is_empty() { "TweMusic".into() } else { self.now_playing.clone() }
    }

    // The icon is drawn in code rather than named, because a themed icon name can be
    // missing from the user's icon theme, which leaves an invisible tray entry.
    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![pixmap(32), pixmap(64)]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let (label, icon) = if self.playing {
            ("Pause", "media-playback-pause")
        } else {
            ("Play", "media-playback-start")
        };
        vec![
            item("Show TweMusic", "multimedia-player", TrayCommand::Show),
            MenuItem::Separator,
            item(label, icon, TrayCommand::PlayPause),
            item("Next", "media-skip-forward", TrayCommand::Next),
            item("Previous", "media-skip-backward", TrayCommand::Previous),
            MenuItem::Separator,
            item("Quit", "application-exit", TrayCommand::Quit),
        ]
    }
}

pub struct TrayHandle(Handle<TweTray>);

impl TrayHandle {
    /// Starts the tray. Fails with a readable message when the desktop has no tray host
    /// (for example GNOME without the AppIndicator extension).
    pub fn spawn(sender: Sender<TrayCommand>, wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let tray = TweTray { sender, wake: Box::new(wake), playing: false, now_playing: String::new() };
        match tray.spawn() {
            Ok(handle) => Ok(Self(handle)),
            Err(ksni::Error::WontShow) => Err(
                "Your desktop has no system tray. On GNOME, install the AppIndicator extension.".to_owned(),
            ),
            Err(error) => Err(format!("Could not start the tray icon: {error}")),
        }
    }

    pub fn update(&self, playing: bool, now_playing: &str) {
        let now_playing = now_playing.to_owned();
        self.0.update(move |tray| {
            tray.playing = playing;
            tray.now_playing = now_playing;
        });
    }

    pub fn shutdown(self) {
        self.0.shutdown().wait();
    }
}

/// A purple disc with a white music note, as ARGB32 (network byte order).
fn pixmap(size: usize) -> ksni::Icon {
    let scale = size as f32 / 32.0;
    // Soft edge: coverage from the signed distance (positive inside).
    let coverage = |distance: f32| (distance * scale + 0.5).clamp(0.0, 1.0);
    let mut data = Vec::with_capacity(size * size * 4);
    for row in 0..size {
        for column in 0..size {
            let x = (column as f32 + 0.5) / scale;
            let y = (row as f32 + 0.5) / scale;
            let disc = coverage(15.5 - ((x - 16.0).powi(2) + (y - 16.0).powi(2)).sqrt());
            let head = coverage(4.0 - ((x - 12.5).powi(2) + (y - 21.5).powi(2)).sqrt());
            let stem = coverage((x - 15.5).min(18.0 - x).min(y - 8.0).min(21.5 - y));
            let flag = if (18.0..=23.0).contains(&x) && (8.0..=13.0).contains(&y) && (y - 8.0) <= (x - 18.0) + 3.0 {
                1.0
            } else {
                0.0
            };
            let note = head.max(stem).max(flag);
            let mix = |background: f32, foreground: f32| (background + (foreground - background) * note) as u8;
            data.extend_from_slice(&[
                (disc * 255.0) as u8,
                mix(103.0, 255.0),
                mix(80.0, 255.0),
                mix(164.0, 255.0),
            ]);
        }
    }
    ksni::Icon { width: size as i32, height: size as i32, data }
}
