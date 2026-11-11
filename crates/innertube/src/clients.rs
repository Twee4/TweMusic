//! Client identities sent to InnerTube. They live in `clients.json` (embedded in the
//! build) so a broken profile can be fixed by editing a file: put a replacement at
//! `~/.config/twe_music/clients.json` (or `$XDG_CONFIG_HOME/twe_music/clients.json`).

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::Deserialize;

const BUILTIN: &str = include_str!("../clients.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProfile {
    /// A label for this profile (shown in error messages).
    pub name: String,
    /// The `clientName` sent to InnerTube (for example `ANDROID_VR`).
    pub client_name: String,
    pub client_version: String,
    /// The numeric client ID for the `X-YouTube-Client-Name` header. Optional for the
    /// well-known client names.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Which InnerTube host to call.
    #[serde(default = "default_host")]
    pub host: String,
    pub user_agent: String,
    /// Which endpoints this profile is meant for (`player`, `search`, `browse`).
    #[serde(default = "default_use_for")]
    pub use_for: Vec<String>,
    /// True when the profile's streams need signature / throttling deciphering, which this
    /// crate can't do yet. Such profiles are kept in the file but skipped.
    #[serde(default)]
    pub needs_cipher: bool,
    /// Extra `context.client` fields (OS and device details).
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

fn default_host() -> String {
    "music.youtube.com".to_owned()
}

fn default_use_for() -> Vec<String> {
    vec!["player".to_owned()]
}

const HOSTS: [&str; 2] = ["music.youtube.com", "www.youtube.com"];

/// Numeric IDs of the well-known clients.
fn known_id(client_name: &str) -> Option<&'static str> {
    Some(match client_name {
        "WEB" => "1",
        "ANDROID" => "3",
        "IOS" => "5",
        "TVHTML5" => "7",
        "ANDROID_VR" => "28",
        "WEB_REMIX" => "67",
        "TVHTML5_SIMPLY" => "75",
        "VISIONOS" => "101",
        _ => return None,
    })
}

impl ClientProfile {
    /// The numeric client ID, from the file or from the well-known names.
    pub fn id(&self) -> &str {
        self.client_id.as_deref().or_else(|| known_id(&self.client_name)).unwrap_or("")
    }

    /// True if this profile can be used to look up a playable stream right now.
    pub fn can_play(&self) -> bool {
        !self.needs_cipher && self.use_for.iter().any(|endpoint| endpoint == "player")
    }
}

#[derive(Deserialize)]
struct File {
    clients: Vec<ClientProfile>,
}

fn plain(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.chars().all(|ch| (' '..='~').contains(&ch))
}

/// Parses and validates a client list. Values end up in HTTP headers and URLs, so they must
/// be printable ASCII, a sensible length, and the host must be one of YouTube's own.
pub fn parse(text: &str) -> Result<Vec<ClientProfile>, String> {
    let file: File = serde_json::from_str(text).map_err(|error| format!("invalid clients file: {error}"))?;
    if file.clients.is_empty() {
        return Err("the clients file lists no clients".to_owned());
    }
    for client in &file.clients {
        let id = client.id();
        let extra_ok = client.extra.iter().all(|(key, value)| {
            !key.is_empty() && key.chars().all(|ch| ch.is_ascii_alphanumeric()) && plain(value, 128)
        });
        let fields_ok = plain(&client.name, 64)
            && plain(&client.client_name, 64)
            && plain(&client.client_version, 32)
            && !id.is_empty()
            && id.len() <= 6
            && id.chars().all(|ch| ch.is_ascii_digit())
            && plain(&client.user_agent, 300)
            && HOSTS.contains(&client.host.as_str())
            && client.use_for.iter().all(|endpoint| plain(endpoint, 16));
        if !(fields_ok && extra_ok) {
            return Err(format!("invalid client entry: {} {}", client.name, client.client_version));
        }
    }
    Ok(file.clients)
}

pub fn builtin() -> Vec<ClientProfile> {
    parse(BUILTIN).expect("the embedded clients.json is valid")
}

fn override_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("twe_music").join("clients.json"))
}

/// The user's `clients.json` if it exists and is valid, otherwise the built-in list.
pub fn load() -> Vec<ClientProfile> {
    override_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| parse(&text).ok())
        .unwrap_or_else(builtin)
}

/// The profiles meant for `operation` (`player`, `search` or `browse`), in file order.
/// Profiles that need deciphering are left out.
pub fn eligible<'a>(profiles: &'a [ClientProfile], operation: &str) -> Vec<&'a ClientProfile> {
    profiles
        .iter()
        .filter(|profile| !profile.needs_cipher && profile.use_for.iter().any(|name| name == operation))
        .collect()
}

/// Index of the client that worked last, so the next lookup tries it first.
static PREFERRED: AtomicUsize = AtomicUsize::new(0);

pub fn remember_success(index: usize) {
    PREFERRED.store(index, Ordering::Relaxed);
}

/// The order to try `count` clients in: the last one that worked first, then the rest.
pub fn rotation(count: usize) -> Vec<usize> {
    let first = PREFERRED.load(Ordering::Relaxed).min(count.saturating_sub(1));
    (0..count).map(|offset| (first + offset) % count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_list_is_valid() {
        let clients = builtin();
        assert_eq!(clients[0].client_name, "ANDROID_VR");
        assert_eq!(clients[0].id(), "28");
        assert_eq!(clients[0].extra.get("osName").map(String::as_str), Some("Android"));
        let playable: Vec<_> = clients.iter().filter(|client| client.can_play()).map(|client| client.name.as_str()).collect();
        assert_eq!(playable, ["ANDROID_VR", "ANDROID_VR_OLD", "VISIONOS"]);
        assert!(clients.iter().any(|client| client.name == "WEB_REMIX" && client.needs_cipher));
    }

    #[test]
    fn eligible_filters_by_operation_and_cipher() {
        let clients = builtin();
        let names: Vec<_> = eligible(&clients, "player").iter().map(|client| client.name.as_str()).collect();
        assert_eq!(names, ["ANDROID_VR", "ANDROID_VR_OLD", "VISIONOS"]);
        assert!(eligible(&clients, "search").is_empty()); // only WEB_REMIX, which needs deciphering
    }

    #[test]
    fn accepts_minimal_entries_and_infers_the_client_id() {
        let text = r#"{"clients":[{"name":"WEB_REMIX","clientName":"WEB_REMIX","clientVersion":"1.2","userAgent":"ua"}]}"#;
        let clients = parse(text).unwrap();
        assert_eq!(clients[0].id(), "67");
        assert_eq!(clients[0].host, "music.youtube.com");
        assert!(clients[0].can_play());
    }

    #[test]
    fn rejects_bad_overrides() {
        let entry = |extra: &str| format!(
            r#"{{"clients":[{{"name":"X","clientName":"X","clientVersion":"1","clientId":"1","userAgent":"ok"{extra}}}]}}"#
        );
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"clients":[]}"#).is_err());
        assert!(parse(&entry("")).is_ok());
        assert!(parse(&entry(r#","host":"evil.example""#)).is_err());
        assert!(parse(&entry(r#","extra":{"a b":"c"}"#)).is_err());
        assert!(parse(&entry(r#","clientId":"x1""#)).is_err());
        let bad_agent = r#"{"clients":[{"name":"X","clientName":"X","clientVersion":"1","clientId":"1","userAgent":"a\r\nHost: evil"}]}"#;
        assert!(parse(bad_agent).is_err());
        let unknown_client = r#"{"clients":[{"name":"X","clientName":"NOPE","clientVersion":"1","userAgent":"ok"}]}"#;
        assert!(parse(unknown_client).is_err());
    }

    #[test]
    fn rotation_starts_with_the_last_success() {
        remember_success(1);
        assert_eq!(rotation(3), vec![1, 2, 0]);
        remember_success(7);
        assert_eq!(rotation(3), vec![2, 0, 1]);
        remember_success(0);
        assert_eq!(rotation(2), vec![0, 1]);
    }
}
