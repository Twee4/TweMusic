//! "Up next": songs related to a track, from InnerTube's `next` endpoint (the same
//! automix radio YouTube Music builds after a song).

use serde_json::json;

use crate::{clients, ids, models::TrackInfo, parse, transport, InnerTubeError};

/// Songs that follow `video_id` in its radio, without the song itself. Blocks on the
/// network, so call it from a worker thread.
pub fn up_next(video_id: &str) -> Result<Vec<TrackInfo>, InnerTubeError> {
    if ids::youtube_music_url(video_id).is_none() {
        return Err(InnerTubeError::Api("YouTube returned an invalid video ID.".to_owned()));
    }
    let profiles = clients::load();
    let usable = clients::eligible(&profiles, "browse");
    let http = transport::http_client()?;
    let mut last_error = None;
    for client in usable {
        let body = json!({
            "context": clients::build_context(client, None),
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        });
        match transport::post(&http, client, "next", &body, None) {
            Ok(response) => {
                let tracks: Vec<TrackInfo> = parse::collect_queue_tracks(&response)
                    .into_iter()
                    .filter(|track| track.video_id != video_id)
                    .collect();
                return Ok(tracks);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| InnerTubeError::Api("No client profile in clients.json is meant for browse.".to_owned())))
}
