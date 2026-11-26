//! Small MPV JSON-IPC controller for local audio playback on Linux.
//! It deliberately accepts filesystem paths only; remote service results are metadata-only.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

#[derive(Debug)]
pub enum MpvError {
    NotInstalled(std::io::Error),
    IpcUnavailable(String),
    Io(std::io::Error),
    InvalidResponse(String),
}

impl std::fmt::Display for MpvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled(error) => write!(f, "Could not start mpv (install mpv first): {error}"),
            Self::IpcUnavailable(message) => write!(f, "mpv IPC unavailable: {message}"),
            Self::Io(error) => write!(f, "mpv IPC error: {error}"),
            Self::InvalidResponse(message) => write!(f, "mpv returned an invalid response: {message}"),
        }
    }
}

impl std::error::Error for MpvError {}

impl From<std::io::Error> for MpvError {
    fn from(error: std::io::Error) -> Self { Self::Io(error) }
}


/// How long a property read waits for mpv. These run on the UI thread twice a second.
const POLL_TIMEOUT: Duration = Duration::from_millis(250);
/// After a read times out, skip property reads for this long.
const BUSY_BACKOFF: Duration = Duration::from_millis(1500);

pub struct MpvPlayer {
    child: Child,
    socket_path: PathBuf,
    /// mpv's own log of warnings and errors, so a failure can be explained.
    log_path: PathBuf,
    request_id: u64,
    /// The user agent last given to mpv, so it is only set again when it changes.
    user_agent: Option<String>,
    /// While mpv is not answering (for example it is busy opening a stream), property reads
    /// fail at once until this time, so the window never freezes waiting for it.
    busy_until: Option<Instant>,
}

impl MpvPlayer {
    pub fn start() -> Result<Self, MpvError> {
        let socket_path = std::env::temp_dir().join(format!("twe-music-mpv-{}.sock", std::process::id()));
        let _ = fs::remove_file(&socket_path);
        let log_path = std::env::temp_dir().join(format!("twe-music-mpv-{}.log", std::process::id()));
        let _ = fs::remove_file(&log_path);
        let child = Command::new("mpv")
            .args([
                "--no-video",
                "--idle=yes",
                "--no-terminal",
                "--force-window=no",
                "--ytdl=no",
                "--msg-level=all=warn",
            ])
            .arg(format!("--input-ipc-server={}", socket_path.display()))
            .arg(format!("--log-file={}", log_path.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(MpvError::NotInstalled)?;
        let mut player = Self { child, socket_path, log_path, request_id: 0, user_agent: None, busy_until: None };
        player.wait_for_socket()?;
        Ok(player)
    }

    pub fn load_file(&mut self, path: &Path) -> Result<(), MpvError> {
        if !path.is_file() {
            return Err(MpvError::IpcUnavailable(format!("not a regular file: {}", path.display())));
        }
        self.command(json!(["loadfile", path.to_string_lossy(), "replace"]))?;
        Ok(())
    }

    /// Plays a stream resolved by [`twe_innertube::resolve`], replacing whatever is playing (and
    /// any track queued behind it). Only Google media hosts over HTTPS are accepted, and MPV is
    /// told to use the same user agent the URL was issued to.
    pub fn load_stream(&mut self, stream: &twe_innertube::Stream) -> Result<(), MpvError> {
        self.send_stream(stream, "replace")
    }

    /// Queues a stream to start the moment the current track ends, with no gap. mpv has one
    /// user agent at a time, so a stream issued to a different client than the one playing
    /// shares it for the last seconds of the current track; the clients in use rarely differ.
    pub fn queue_stream(&mut self, stream: &twe_innertube::Stream) -> Result<(), MpvError> {
        self.send_stream(stream, "append")
    }

    fn send_stream(&mut self, stream: &twe_innertube::Stream, mode: &str) -> Result<(), MpvError> {
        if !twe_innertube::is_allowed_stream_url(&stream.url) {
            return Err(MpvError::IpcUnavailable("refusing to load an unsupported URL".to_owned()));
        }
        if self.user_agent.as_deref() != Some(stream.user_agent.as_str()) {
            self.command(json!(["set_property", "user-agent", stream.user_agent]))?;
            self.user_agent = Some(stream.user_agent.clone());
        }
        self.command(json!(["loadfile", stream.url, mode]))?;
        Ok(())
    }

    /// The last error mpv logged, explained in plain words, if it logged one.
    pub fn last_problem(&self) -> Option<String> {
        let log = fs::read(&self.log_path).ok()?;
        let tail = &log[log.len().saturating_sub(16 * 1024)..];
        explain_log(&String::from_utf8_lossy(tail))
    }

    /// The last few lines mpv logged, for diagnosing a stream that would not open.
    pub fn recent_log(&self, lines: usize) -> Vec<String> {
        let Ok(log) = fs::read(&self.log_path) else { return Vec::new() };
        let tail = &log[log.len().saturating_sub(16 * 1024)..];
        let text = String::from_utf8_lossy(tail);
        let all: Vec<&str> = text.lines().collect();
        all[all.len().saturating_sub(lines)..]
            .iter()
            .map(|line| line.chars().take(220).collect::<String>())
            .collect()
    }

    /// Index of the playing entry in mpv's playlist. It becomes 1 when a queued stream has
    /// taken over from the track before it.
    pub fn playlist_pos(&mut self) -> Option<i64> {
        self.get_property("playlist-pos").ok()?.as_i64()
    }

    /// Jumps to the queued stream now (the Next button), instead of waiting for the track to end.
    pub fn advance_to_queued(&mut self) -> Result<(), MpvError> {
        self.command(json!(["playlist-next", "force"])).map(|_| ())
    }

    /// Removes the finished first entry once a queued stream has taken over.
    pub fn drop_finished_entry(&mut self) {
        let _ = self.command(json!(["playlist-remove", 0]));
    }

    /// Drops everything queued behind the playing track.
    pub fn clear_queued(&mut self) {
        let _ = self.command(json!(["playlist-clear"]));
    }

    /// Reads an MPV property. An unavailable property (nothing loaded yet) is `Value::Null`.
    pub fn get_property(&mut self, name: &str) -> Result<Value, MpvError> {
        if self.busy_until.is_some_and(|until| Instant::now() < until) {
            return Err(MpvError::IpcUnavailable("mpv is busy".to_owned()));
        }
        match self.request_within(json!(["get_property", name]), POLL_TIMEOUT) {
            Ok(response) => Ok(response.get("data").cloned().unwrap_or(Value::Null)),
            Err(MpvError::Io(error)) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                self.busy_until = Some(Instant::now() + BUSY_BACKOFF);
                Err(MpvError::IpcUnavailable("mpv did not answer in time".to_owned()))
            }
            Err(MpvError::InvalidResponse(message)) if message.contains("property unavailable") => Ok(Value::Null),
            Err(error) => Err(error),
        }
    }

    /// Seeks to an absolute position in the current track, in seconds.
    pub fn seek_to(&mut self, seconds: f64) -> Result<(), MpvError> {
        self.command(json!(["seek", seconds.max(0.0), "absolute"]))?;
        Ok(())
    }

    pub fn position(&mut self) -> Option<f64> {
        self.get_property("time-pos").ok()?.as_f64()
    }

    pub fn duration(&mut self) -> Option<f64> {
        self.get_property("duration").ok()?.as_f64()
    }

    /// True when MPV has nothing loaded (finished, failed, or never started a file).
    pub fn is_idle(&mut self) -> bool {
        self.get_property("idle-active").ok().and_then(|value| value.as_bool()).unwrap_or(false)
    }

    /// Toggles pause and returns the new pause state.
    pub fn toggle_pause(&mut self) -> Result<bool, MpvError> {
        self.command(json!(["cycle", "pause"]))?;
        // Query the property after toggling so the UI reflects actual MPV state.
        let response = self.request(json!(["get_property", "pause"]))?;
        response.get("data").and_then(Value::as_bool)
            .ok_or_else(|| MpvError::InvalidResponse("missing pause property".to_owned()))
    }

    fn wait_for_socket(&mut self) -> Result<(), MpvError> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.child.try_wait()?.is_some() {
                return Err(MpvError::IpcUnavailable("mpv exited before opening its IPC socket".to_owned()));
            }
            if self.socket_path.exists() { return Ok(()); }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return Err(MpvError::IpcUnavailable("timed out waiting for mpv socket".to_owned()));
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    fn command(&mut self, command: Value) -> Result<Value, MpvError> {
        self.request(command)
    }

    fn request(&mut self, command: Value) -> Result<Value, MpvError> {
        self.request_within(command, Duration::from_secs(2))
    }

    fn request_within(&mut self, command: Value, timeout: Duration) -> Result<Value, MpvError> {
        self.request_id = self.request_id.wrapping_add(1);
        let id = self.request_id;
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.write_all(json!({ "command": command, "request_id": id }).to_string().as_bytes())?;
        stream.write_all(b"\n")?;
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Err(MpvError::InvalidResponse("connection closed before response".to_owned()));
            }
            let value: Value = serde_json::from_str(&line)
                .map_err(|error| MpvError::InvalidResponse(error.to_string()))?;
            if value.get("request_id").and_then(Value::as_u64) == Some(id) {
                if value.get("error").and_then(Value::as_str).is_some_and(|error| error != "success") {
                    return Err(MpvError::InvalidResponse(value.to_string()));
                }
                return Ok(value);
            }
        }
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        let _ = self.command(json!(["quit"]));
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.log_path);
    }
}

/// Picks the last error line from mpv's log and turns it into advice.
pub fn explain_log(log: &str) -> Option<String> {
    let line = log
        .lines()
        .rev()
        .find(|line| line.contains("][e]") || line.to_ascii_lowercase().contains("failed"))?
        .trim();
    let lower = line.to_ascii_lowercase();
    let advice = if lower.contains("[ao]") || lower.contains("audio output") || lower.contains("audio driver") {
        "mpv could not open an audio output. Check that sound works, and on Arch install pipewire-pulse (or pipewire-alsa)."
    } else if lower.contains("403") || lower.contains("forbidden") {
        "The server refused the stream (HTTP 403). Update yt-dlp and try again."
    } else if lower.contains("tls") || lower.contains("certificate") || lower.contains("ssl") {
        "mpv could not make a secure connection. Check ca-certificates and your system clock."
    } else if lower.contains("unrecognized file format") || lower.contains("no codec") || lower.contains("ffmpeg") {
        "mpv could not decode this stream. Update mpv and ffmpeg (sudo pacman -Syu)."
    } else {
        let short: String = line.chars().take(140).collect();
        return Some(format!("mpv said: {short}"));
    };
    Some(advice.to_owned())
}

#[cfg(test)]
mod diagnosis_tests {
    use super::explain_log;

    #[test]
    fn explains_common_mpv_failures() {
        assert!(explain_log("[  0.10][e][ao] Failed to initialize audio driver 'pipewire'").unwrap().contains("audio output"));
        assert!(explain_log("[  1.00][e][http] HTTP error 403 Forbidden").unwrap().contains("403"));
        assert!(explain_log("[  1.00][e][tls] certificate verify failed").unwrap().contains("secure connection"));
        assert!(explain_log("[  1.00][e][demux] something odd").unwrap().starts_with("mpv said:"));
        assert_eq!(explain_log("[  0.10][w][cplayer] just a warning"), None);
        assert_eq!(explain_log(""), None);
    }
}
