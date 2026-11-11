//! Song search through InnerTube's `search` endpoint (public metadata only).

use serde_json::json;

use crate::{clients, models::TrackInfo, parse, transport, InnerTubeError};

/// Restricts results to songs.
const SONGS_FILTER: &str = "EgWKAQIIAWoKEAkQChADEAQQBQ%3D%3D";

/// Searches YouTube Music. Blocks on the network, so call it from a worker thread.
pub fn search(query: &str) -> Result<Vec<TrackInfo>, InnerTubeError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let profiles = clients::load();
    let usable = clients::eligible(&profiles, "search");
    if usable.is_empty() {
        return Err(InnerTubeError::Api("No client profile in clients.json is meant for search.".to_owned()));
    }
    let http = transport::http_client()?;
    let mut last_error = None;
    for client in usable {
        let body = json!({
            "context": clients::build_context(client, None),
            "query": query,
            "params": SONGS_FILTER,
        });
        match transport::post(&http, client, "search", &body, None) {
            Ok(response) => return Ok(parse::collect_search_tracks(&response)),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or(InnerTubeError::NoFormat))
}
