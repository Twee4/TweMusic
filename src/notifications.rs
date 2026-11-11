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

/// Sends a test notification and reports the outcome through `done` (`None` on success,
/// otherwise a readable error), so Settings can tell the user whether their desktop has
/// a notification daemon running.
pub fn send_test(done: impl FnOnce(Option<String>) + Send + 'static) {
    thread::spawn(move || {
        let result = notify_rust::Notification::new()
            .appname("TweMusic")
            .summary("TweMusic notifications work")
            .body("You will see this when a new song starts in the background.")
            .icon("multimedia-player")
            .show();
        done(result.err().map(|error| {
            format!("No notification daemon answered ({error}). Install and start one, such as mako, dunst or swaync.")
        }));
    });
}
