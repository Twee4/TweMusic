//! Desktop notifications for the track that just started.

use std::thread;

use crate::models::Track;

/// Shows "title / artist" as a desktop notification. Runs on its own thread because the
/// D-Bus call can block, and ignores failures: a missing notification daemon is not an error
/// worth interrupting playback for.
pub fn show_track(track: &Track) {
    let title = track.title.clone();
    let artist = track.artist.clone();
    thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("TweMusic")
            .summary(&title)
            .body(&artist)
            .icon("multimedia-player")
            .timeout(notify_rust::Timeout::Milliseconds(4000))
            .show();
    });
}
