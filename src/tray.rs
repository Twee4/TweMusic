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
    /// True when the panel the icon sits on is dark, so the icon is drawn light.
    dark_panel: bool,
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
        [24, 32, 48, 64].into_iter().map(|size| pixmap(size, self.dark_panel)).collect()
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
        let tray = TweTray { sender, wake: Box::new(wake), playing: false, now_playing: String::new(), dark_panel: true };
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

    /// Redraws the icon in light or dark ink to stay visible on the panel.
    pub fn set_dark_panel(&self, dark: bool) {
        self.0.update(move |tray| tray.dark_panel = dark);
    }

    pub fn shutdown(self) {
        self.0.shutdown().wait();
    }
}

/// The tray icon: the app's two notes as a single-colour shape on a transparent background,
/// like the symbolic icons of other tray apps, as ARGB32 pixels (network byte order). `ink` is
/// the colour of the shape: light for a dark panel, dark for a light one.
fn render_icon(size: usize, ink: [u8; 3]) -> Vec<u8> {
    // The notes of `packaging/twemusic.svg`, in its 128 x 128 space, scaled up to fill the icon.
    const SCALE: f32 = 1.2;
    let inside_note = |x: f32, y: f32| {
        let head = |cx: f32, cy: f32| (x - cx).hypot(y - cy) <= 17.0;
        let stem_right = (76.0..=84.0).contains(&x) && (24.0..=78.5).contains(&y);
        let stem_left = (46.0..=54.0).contains(&x) && (34.0..=88.5).contains(&y);
        let beam = (46.0..=84.0).contains(&x) && {
            let top = 34.0 - (x - 46.0) * 10.0 / 38.0;
            let bottom = if x <= 54.0 { 45.0 } else if x >= 76.0 { 40.0 } else { 45.0 - (x - 54.0) * 5.0 / 22.0 };
            y >= top && y <= bottom
        };
        head(67.0, 78.5) || head(37.0, 88.5) || stem_right || stem_left || beam
    };
    let samples = 3;
    let mut data = Vec::with_capacity(size * size * 4);
    for row in 0..size {
        for column in 0..size {
            let mut covered = 0.0_f32;
            for sy in 0..samples {
                for sx in 0..samples {
                    // Back from the icon's pixel to the notes' space, centred on their middle.
                    let px = (column as f32 + (sx as f32 + 0.5) / samples as f32) / size as f32 * 128.0;
                    let py = (row as f32 + (sy as f32 + 0.5) / samples as f32) / size as f32 * 128.0;
                    let x = (px - 64.0) / SCALE + 52.0;
                    let y = (py - 64.0) / SCALE + 64.75;
                    if inside_note(x, y) {
                        covered += 1.0;
                    }
                }
            }
            let alpha = covered / (samples * samples) as f32;
            data.extend_from_slice(&[(alpha * 255.0).round() as u8, ink[0], ink[1], ink[2]]);
        }
    }
    data
}

fn pixmap(size: usize, dark_panel: bool) -> ksni::Icon {
    let ink = if dark_panel { [0xF2, 0xF2, 0xF2] } else { [0x2B, 0x27, 0x33] };
    ksni::Icon { width: size as i32, height: size as i32, data: render_icon(size, ink) }
}
