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
pub struct ClientProfile {
    pub name: String,
    pub version: String,
    pub id: String,
    pub user_agent: String,
    /// Extra `context.client` fields (OS and device details).
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct File {
    clients: Vec<ClientProfile>,
}

fn plain(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.chars().all(|ch| (' '..='~').contains(&ch))
}

/// Parses and validates a client list. Values end up in HTTP headers, so they must be
/// printable ASCII and a sensible length.
pub fn parse(text: &str) -> Result<Vec<ClientProfile>, String> {
    let file: File = serde_json::from_str(text).map_err(|error| format!("invalid clients file: {error}"))?;
    if file.clients.is_empty() {
        return Err("the clients file lists no clients".to_owned());
    }
    for client in &file.clients {
        let extra_ok = client.extra.iter().all(|(key, value)| {
            !key.is_empty() && key.chars().all(|ch| ch.is_ascii_alphanumeric()) && plain(value, 128)
        });
        let fields_ok = plain(&client.name, 64)
            && plain(&client.version, 32)
            && client.id.chars().all(|ch| ch.is_ascii_digit())
            && !client.id.is_empty()
            && client.id.len() <= 6
            && plain(&client.user_agent, 300);
        if !(fields_ok && extra_ok) {
            return Err(format!("invalid client entry: {} {}", client.name, client.version));
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
        assert!(clients.len() >= 2);
        assert_eq!(clients[0].name, "ANDROID_VR");
        assert_eq!(clients[0].extra.get("osName").map(String::as_str), Some("Android"));
    }

    #[test]
    fn rejects_bad_overrides() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"clients":[]}"#).is_err());
        let bad_agent = r#"{"clients":[{"name":"X","version":"1","id":"1","user_agent":"a\r\nHost: evil"}]}"#;
        assert!(parse(bad_agent).is_err());
        let bad_id = r#"{"clients":[{"name":"X","version":"1","id":"x1","user_agent":"ok"}]}"#;
        assert!(parse(bad_id).is_err());
        let bad_key = r#"{"clients":[{"name":"X","version":"1","id":"1","user_agent":"ok","extra":{"a b":"c"}}]}"#;
        assert!(parse(bad_key).is_err());
        let good = r#"{"clients":[{"name":"X","version":"1","id":"1","user_agent":"ok"}]}"#;
        assert_eq!(parse(good).unwrap().len(), 1);
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
