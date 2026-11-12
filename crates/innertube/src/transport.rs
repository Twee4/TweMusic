//! The HTTP side of InnerTube: one place that builds the client and the headers.

use std::time::Duration;

use serde_json::Value;

use crate::{clients::ClientProfile, InnerTubeError};

pub const VISITOR_DATA_URL: &str = "https://www.youtube.com/sw.js_data";

pub fn http_client() -> Result<reqwest::blocking::Client, InnerTubeError> {
    Ok(reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build()?)
}

/// Fetches the raw page that contains the visitor ID. Failures just mean "no visitor ID".
pub fn fetch_visitor_page(http: &reqwest::blocking::Client) -> Option<String> {
    http.get(VISITOR_DATA_URL).send().ok()?.text().ok()
}

/// Posts a `player` request as `client` and returns the decoded JSON.
pub fn post_player(
    http: &reqwest::blocking::Client,
    client: &ClientProfile,
    body: &Value,
    visitor_data: Option<&str>,
) -> Result<Value, InnerTubeError> {
    post(http, client, "player", body, visitor_data)
}

/// Posts to an InnerTube endpoint (`player`, `search` or `browse`) as `client`.
pub fn post(
    http: &reqwest::blocking::Client,
    client: &ClientProfile,
    endpoint: &str,
    body: &Value,
    visitor_data: Option<&str>,
) -> Result<Value, InnerTubeError> {
    debug_assert!(["player", "search", "browse"].contains(&endpoint));
    // The host was validated against YouTube's own hosts when the profile was loaded.
    let origin = format!("https://{}", client.host);
    let mut request = http
        .post(format!("{origin}/youtubei/v1/{endpoint}?prettyPrint=false"))
        .header("User-Agent", client.user_agent.as_str())
        .header("X-Goog-Api-Format-Version", "1")
        .header("X-YouTube-Client-Name", client.id())
        .header("X-YouTube-Client-Version", client.client_version.as_str())
        .header("Origin", origin.as_str())
        .header("X-Origin", origin.as_str())
        .header("Referer", format!("{origin}/"))
        .json(body);
    if let Some(visitor_data) = visitor_data {
        request = request.header("X-Goog-Visitor-Id", visitor_data);
    }
    let response = request.send()?;
    if !response.status().is_success() {
        return Err(InnerTubeError::Api(format!("HTTP {}", response.status())));
    }
    Ok(response.json::<Value>()?)
}
