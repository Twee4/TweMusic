//! The HTTP side of InnerTube: one place that builds the client and the headers.

use std::time::Duration;

use serde_json::Value;

use crate::clients::ClientProfile;

pub const ORIGIN: &str = "https://music.youtube.com";
pub const PLAYER_URL: &str = "https://music.youtube.com/youtubei/v1/player?prettyPrint=false";
pub const VISITOR_DATA_URL: &str = "https://www.youtube.com/sw.js_data";

pub fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| format!("Could not start the network client: {error}"))
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
) -> Result<Value, String> {
    let mut request = http
        .post(PLAYER_URL)
        .header("User-Agent", client.user_agent.as_str())
        .header("X-Goog-Api-Format-Version", "1")
        .header("X-YouTube-Client-Name", client.id.as_str())
        .header("X-YouTube-Client-Version", client.version.as_str())
        .header("Origin", ORIGIN)
        .header("X-Origin", ORIGIN)
        .header("Referer", format!("{ORIGIN}/"))
        .json(body);
    if let Some(visitor_data) = visitor_data {
        request = request.header("X-Goog-Visitor-Id", visitor_data);
    }
    let response = request.send().map_err(|error| format!("network error: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.json::<Value>().map_err(|error| format!("unreadable response: {error}"))
}
