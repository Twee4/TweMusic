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
        vec![pixmap(24), pixmap(32), pixmap(48), pixmap(64)]
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

/// The app icon (a rounded purple square with two eighth notes) as ARGB32 pixels, network byte
/// order: the same artwork as `packaging/twemusic.svg`, drawn from its shapes so it stays sharp at
/// any size and needs no image files.
fn render_icon(size: usize) -> Vec<u8> {
    // Everything below is in the SVG's own 128 x 128 space.
    let inside_square = |x: f32, y: f32| {
        // Rounded square, corner radius 30.
        let (cx, cy) = ((x - 64.0).abs() - 34.0, (y - 64.0).abs() - 34.0);
        cx.max(0.0).hypot(cy.max(0.0)) + cx.max(cy).min(0.0) <= 30.0
    };
    let inside_note = |x: f32, y: f32| {
        let head = |cx: f32, cy: f32| (x - cx).hypot(y - cy) <= 17.0;
        let stem_right = (76.0..=84.0).contains(&x) && (24.0..=78.5).contains(&y);
        let stem_left = (46.0..=54.0).contains(&x) && (34.0..=88.5).contains(&y);
        // The beam between the two stems: from the slanted top edge down to the lower edge.
        let beam = (46.0..=84.0).contains(&x) && {
            let top = 34.0 - (x - 46.0) * 10.0 / 38.0;
            let bottom = if x <= 54.0 { 45.0 } else if x >= 76.0 { 40.0 } else { 45.0 - (x - 54.0) * 5.0 / 22.0 };
            y >= top && y <= bottom
        };
        head(67.0, 78.5) || head(37.0, 88.5) || stem_right || stem_left || beam
    };
    let lerp = |from: f32, to: f32, t: f32| from + (to - from) * t;
    let mut data = Vec::with_capacity(size * size * 4);
    let samples = 3;
    for row in 0..size {
        for column in 0..size {
            let (mut square, mut note) = (0.0_f32, 0.0_f32);
            for sy in 0..samples {
                for sx in 0..samples {
                    let x = (column as f32 + (sx as f32 + 0.5) / samples as f32) * 128.0 / size as f32;
                    let y = (row as f32 + (sy as f32 + 0.5) / samples as f32) * 128.0 / size as f32;
                    if inside_square(x, y) {
                        square += 1.0;
                        if inside_note(x, y) {
                            note += 1.0;
                        }
                    }
                }
            }
            let total = (samples * samples) as f32;
            let (square, note) = (square / total, if square > 0.0 { note / square } else { 0.0 });
            // The diagonal gradient #6750a4 -> #2b1f4a, with the note #eaddff on top.
            let t = ((column + row) as f32 + 1.0) / (2.0 * size as f32);
            let background = [lerp(103.0, 43.0, t), lerp(80.0, 31.0, t), lerp(164.0, 74.0, t)];
            let ink = [234.0, 221.0, 255.0];
            data.push((square * 255.0).round() as u8);
            for channel in 0..3 {
                data.push(lerp(background[channel], ink[channel], note).round() as u8);
            }
        }
    }
    data
}

fn pixmap(size: usize) -> ksni::Icon {
    ksni::Icon { width: size as i32, height: size as i32, data: render_icon(size) }
}
