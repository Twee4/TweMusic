//! Window hiding for desktops where an app cannot minimise or hide its own window.
//!
//! On Wayland compositors such as Hyprland a client has no say in minimising, so "close to the
//! tray" would do nothing. Hyprland can move a window to a hidden "special" workspace, which
//! makes it disappear until it is moved back; that is what a scratchpad does.

use std::process::{Command, Stdio};

const HIDDEN_WORKSPACE: &str = "special:twemusic";

/// True when running under Hyprland.
pub fn is_hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Moves this app's window to a hidden workspace. Returns false if the desktop is not
/// Hyprland (or `hyprctl` is missing), so the caller can fall back to minimising.
pub fn hide_window() -> bool {
    if !is_hyprland() {
        return false;
    }
    let selector = format!("{HIDDEN_WORKSPACE},pid:{}", std::process::id());
    run_hyprctl(&["dispatch", "movetoworkspacesilent", &selector])
}

/// Brings the window back to the workspace that is showing and focuses it.
pub fn show_window() -> bool {
    if !is_hyprland() {
        return false;
    }
    let pid = std::process::id();
    let workspace = Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|json| parse_workspace_id(&json))
        // A special workspace has a negative id; if one is showing, fall back to the first.
        .filter(|id| *id > 0)
        .unwrap_or(1);
    let moved = run_hyprctl(&["dispatch", "movetoworkspace", &format!("{workspace},pid:{pid}")]);
    let focused = run_hyprctl(&["dispatch", "focuswindow", &format!("pid:{pid}")]);
    moved && focused
}

fn run_hyprctl(args: &[&str]) -> bool {
    Command::new("hyprctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Reads the top-level `"id"` from `hyprctl activeworkspace -j` output.
pub fn parse_workspace_id(json: &str) -> Option<i64> {
    let after = &json[json.find("\"id\"")? + 4..];
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    let end = after.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(after.len());
    after[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_workspace_id() {
        assert_eq!(parse_workspace_id("{\n    \"id\": 3,\n    \"name\": \"3\"\n}"), Some(3));
        assert_eq!(parse_workspace_id("{\"id\":-98,\"name\":\"special:x\"}"), Some(-98));
        assert_eq!(parse_workspace_id("{}"), None);
        assert_eq!(parse_workspace_id("nonsense"), None);
    }
}
