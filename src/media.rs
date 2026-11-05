//! Desktop media controls: shows the current track to the desktop and receives media
//! key / media widget commands. On Linux this is MPRIS over D-Bus (via souvlaki's
//! pure-Rust zbus backend).

use std::{sync::mpsc::Sender, time::Duration};

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};

use crate::{models::Track, state::PlaybackStatus};

pub struct Media {
    controls: MediaControls,
}

impl Media {
    /// Registers with the desktop. Returns `None` when media controls aren't available
    /// (for example there is no D-Bus session), and the app simply runs without them.
    /// Events are sent through `sender`; `wake` is called after each one so the UI can
    /// repaint and handle it promptly.
    pub fn new(sender: Sender<MediaControlEvent>, wake: impl Fn() + Send + 'static) -> Option<Self> {
        let mut controls = MediaControls::new(PlatformConfig {
            dbus_name: "twe_music",
            display_name: "TweMusic",
            hwnd: None,
        })
        .ok()?;
        controls
            .attach(move |event| {
                let _ = sender.send(event);
                wake();
            })
            .ok()?;
        Some(Self { controls })
    }

    /// Publishes the current track (if any), playback state and position.
    pub fn update(&mut self, track: Option<&Track>, status: PlaybackStatus, position_seconds: Option<f64>) {
        if let Some(track) = track {
            let _ = self.controls.set_metadata(MediaMetadata {
                title: Some(&track.title),
                artist: Some(&track.artist),
                album: track.album.as_deref(),
                cover_url: None,
                duration: (track.duration > Duration::ZERO).then_some(track.duration),
            });
        }
        let progress = position_seconds
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            .map(|seconds| MediaPosition(Duration::from_secs_f64(seconds)));
        let playback = match status {
            PlaybackStatus::Playing => MediaPlayback::Playing { progress },
            PlaybackStatus::Paused => MediaPlayback::Paused { progress },
            PlaybackStatus::Stopped => MediaPlayback::Stopped,
        };
        let _ = self.controls.set_playback(playback);
    }
}
