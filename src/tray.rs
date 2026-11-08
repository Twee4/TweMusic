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

    fn icon_name(&self) -> String {
        "multimedia-player".into()
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
