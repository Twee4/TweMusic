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

/// Returns true when `yt-dlp` can be run; MPV needs it to play YouTube URLs.
pub fn ytdlp_available() -> bool {
    Command::new("yt-dlp")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub struct MpvPlayer {
    child: Child,
    socket_path: PathBuf,
    request_id: u64,
}

impl MpvPlayer {
    pub fn start() -> Result<Self, MpvError> {
        let socket_path = std::env::temp_dir().join(format!("twe-music-mpv-{}.sock", std::process::id()));
        let _ = fs::remove_file(&socket_path);
        let child = Command::new("mpv")
            .args([
                "--no-video",
                "--idle=yes",
                "--no-terminal",
                "--force-window=no",
                "--ytdl=yes",
                "--ytdl-format=bestaudio/best",
            ])
            .arg(format!("--input-ipc-server={}", socket_path.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(MpvError::NotInstalled)?;
        let mut player = Self { child, socket_path, request_id: 0 };
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

    /// Loads a YouTube Music watch URL; MPV resolves the audio stream through yt-dlp.
    /// Only `https://music.youtube.com/watch?v=` URLs are accepted.
    pub fn load_url(&mut self, url: &str) -> Result<(), MpvError> {
        if !url.starts_with("https://music.youtube.com/watch?v=") {
            return Err(MpvError::IpcUnavailable("refusing to load an unsupported URL".to_owned()));
        }
        self.command(json!(["loadfile", url, "replace"]))?;
        Ok(())
    }

    /// Reads an MPV property. An unavailable property (nothing loaded yet) is `Value::Null`.
    pub fn get_property(&mut self, name: &str) -> Result<Value, MpvError> {
        match self.request(json!(["get_property", name])) {
            Ok(response) => Ok(response.get("data").cloned().unwrap_or(Value::Null)),
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
        self.request_id = self.request_id.wrapping_add(1);
        let id = self.request_id;
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
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
    }
}
