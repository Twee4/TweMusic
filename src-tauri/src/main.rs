#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use tauri::{State, WebviewUrl, WebviewWindowBuilder};
use twe_player::mpv::MpvPlayer;

/// The shared MPV player, started the first time something is played.
struct Player(Mutex<Option<MpvPlayer>>);

/// Looks up the audio stream for a YouTube video ID with InnerTube and plays it in MPV,
/// the same path the native TweMusic app uses.
#[tauri::command]
async fn play_video(video_id: String, player: State<'_, Player>) -> Result<(), String> {
    let stream = tauri::async_runtime::spawn_blocking(move || twe_innertube::resolve(&video_id))
        .await
        .map_err(|error| error.to_string())??;
    let mut guard = player.0.lock().map_err(|_| "the player lock is poisoned".to_owned())?;
    if guard.is_none() {
        *guard = Some(MpvPlayer::start().map_err(|error| error.to_string())?);
    }
    let mpv = guard.as_mut().ok_or_else(|| "the player did not start".to_owned())?;
    mpv.load_stream(&stream).map_err(|error| error.to_string())
}

/// Toggles pause and returns whether playback is now paused.
#[tauri::command]
fn toggle_pause(player: State<'_, Player>) -> Result<bool, String> {
    let mut guard = player.0.lock().map_err(|_| "the player lock is poisoned".to_owned())?;
    let mpv = guard.as_mut().ok_or_else(|| "nothing is playing".to_owned())?;
    mpv.toggle_pause().map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(Player(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![play_video, toggle_pause])
        .setup(|app| {
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("https://music.youtube.com".parse().unwrap()),
            )
            .title("TweMusic")
            .inner_size(1280.0, 820.0)
            .resizable(true)
            .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri app");
}
