//! Optional fallback for when YouTube's bot check blocks TweMusic's own stream lookup.
//!
//! If the user picks a browser in Settings, `yt-dlp` is run with `--cookies-from-browser`, so
//! the lookup happens as the user's signed-in session. TweMusic never reads, stores or sends
//! the cookies itself: yt-dlp reads them from the browser each time, and only the resulting
//! stream address comes back. Nothing here runs unless the user turned it on and the direct
//! lookup was refused by the bot check.

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;
use twe_innertube::{ids, is_allowed_stream_url, InnerTubeError, Stream};

/// Browsers yt-dlp can read cookies from. Only these exact names are ever passed on, so a
/// hand-edited settings file cannot add yt-dlp options.
pub const BROWSERS: [&str; 5] = ["firefox", "chromium", "chrome", "brave", "vivaldi"];

const TIMEOUT: Duration = Duration::from_secs(60);
/// After a bot check, go straight to yt-dlp for this long instead of asking YouTube again
/// for every track (repeated refused requests are what keeps a connection flagged).
const SKIP_DIRECT_FOR: Duration = Duration::from_secs(10 * 60);
const FALLBACK_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

static BLOCKED_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

pub fn is_supported_browser(name: &str) -> bool {
    BROWSERS.contains(&name)
}

/// Looks the track up directly; if YouTube's bot check refuses and `browser` is set, asks
/// yt-dlp (signed in through that browser) instead. Blocks, so call it from a worker thread.
pub fn resolve(video_id: &str, browser: Option<&str>) -> Result<Stream, String> {
    let browser = browser.filter(|name| is_supported_browser(name));
    if let Some(browser) = browser {
        let blocked = BLOCKED_UNTIL.lock().ok().and_then(|slot| *slot).is_some_and(|until| Instant::now() < until);
        if blocked {
            return via_ytdlp(video_id, browser);
        }
    }
    match twe_innertube::resolve(video_id) {
        Ok(stream) => Ok(stream),
        Err(InnerTubeError::BotCheck) if browser.is_some() => {
            if let Ok(mut slot) = BLOCKED_UNTIL.lock() {
                *slot = Some(Instant::now() + SKIP_DIRECT_FOR);
            }
            via_ytdlp(video_id, browser.unwrap_or_default())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn via_ytdlp(video_id: &str, browser: &str) -> Result<Stream, String> {
    let url = ids::youtube_music_url(video_id).ok_or("YouTube returned an invalid video ID.")?;
    let mut child = Command::new("yt-dlp")
        // A user config could add options such as --exec; ignore it.
        .args(["--ignore-config", "--no-playlist", "--no-warnings", "-j", "-f", "bestaudio/best"])
        .args(["--cookies-from-browser", browser, "--"])
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "YouTube is blocking this connection and yt-dlp is not installed. Install yt-dlp (and deno or node) to sign in through your browser.".to_owned()
            } else {
                format!("Could not start yt-dlp: {error}")
            }
        })?;
    let mut stdout = child.stdout.take().ok_or("Could not read yt-dlp output.")?;
    let mut stderr = child.stderr.take().ok_or("Could not read yt-dlp output.")?;
    let out_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.by_ref().take(4 * 1024 * 1024).read_to_string(&mut text);
        text
    });
    let err_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.by_ref().take(64 * 1024).read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("yt-dlp took too long to answer.".to_owned());
            }
            Err(error) => return Err(format!("yt-dlp failed: {error}")),
        }
    };
    let output = out_reader.join().unwrap_or_default();
    let errors = err_reader.join().unwrap_or_default();
    if !status.success() {
        return Err(describe_failure(&errors, browser));
    }
    parse_output(&output)
}

/// A short, readable reason from yt-dlp's last error line.
fn describe_failure(stderr: &str, browser: &str) -> String {
    let last = stderr.lines().rev().find(|line| line.contains("ERROR")).unwrap_or("").trim();
    let lower = last.to_ascii_lowercase();
    if lower.contains("cookie") || lower.contains("keyring") || lower.contains("database") {
        format!("yt-dlp could not read {browser}'s cookies. Close {browser} and check you are signed in to YouTube there.")
    } else if lower.contains("not a bot") || lower.contains("sign in") {
        format!("YouTube still asked for sign-in. Open music.youtube.com in {browser}, sign in, then try again.")
    } else if last.is_empty() {
        "yt-dlp failed without an error message.".to_owned()
    } else {
        let short: String = last.chars().take(160).collect();
        format!("yt-dlp failed: {short}")
    }
}

/// Reads yt-dlp's `-j` output (one JSON object) into a stream TweMusic is willing to play.
pub fn parse_output(output: &str) -> Result<Stream, String> {
    let line = output.lines().find(|line| line.trim_start().starts_with('{')).ok_or("yt-dlp gave no stream.")?;
    let value: Value = serde_json::from_str(line).map_err(|_| "yt-dlp gave an unreadable answer.".to_owned())?;
    let url = value.get("url").and_then(Value::as_str).ok_or("yt-dlp gave no stream address.")?;
    if !is_allowed_stream_url(url) {
        return Err("yt-dlp returned an address TweMusic will not play.".to_owned());
    }
    let user_agent = value
        .pointer("/http_headers/User-Agent")
        .and_then(Value::as_str)
        .filter(|agent| !agent.is_empty() && agent.is_ascii() && !agent.contains(['\r', '\n']))
        .unwrap_or(FALLBACK_USER_AGENT)
        .to_owned();
    let mime_type = value.get("ext").and_then(Value::as_str).map_or_else(String::new, |ext| format!("audio/{ext}"));
    let bitrate = value.get("abr").and_then(Value::as_f64).map_or(0, |abr| (abr * 1000.0) as u64);
    Ok(Stream { url: url.to_owned(), user_agent, mime_type, bitrate })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_browsers_are_accepted() {
        assert!(is_supported_browser("firefox"));
        assert!(!is_supported_browser("firefox:default; rm"));
        assert!(!is_supported_browser("--exec"));
        assert!(!is_supported_browser(""));
    }

    #[test]
    fn reads_the_stream_from_ytdlp_json() {
        let json = r#"{"id":"x","url":"https://rr1---sn-a.googlevideo.com/videoplayback?expire=1","ext":"webm","abr":129.5,
            "http_headers":{"User-Agent":"Mozilla/5.0 Test"}}"#.replace('\n', " ");
        let stream = parse_output(&json).unwrap();
        assert_eq!(stream.user_agent, "Mozilla/5.0 Test");
        assert_eq!(stream.mime_type, "audio/webm");
        assert_eq!(stream.bitrate, 129_500);
    }

    #[test]
    fn refuses_foreign_hosts_and_garbage() {
        assert!(parse_output(r#"{"url":"https://evil.example/a"}"#).is_err());
        assert!(parse_output(r#"{"url":"http://rr1---sn-a.googlevideo.com/a"}"#).is_err());
        assert!(parse_output("not json").is_err());
        assert!(parse_output(r#"{"id":"x"}"#).is_err());
        // Missing headers fall back to a plain browser user agent.
        let stream = parse_output(r#"{"url":"https://rr1---sn-a.googlevideo.com/a"}"#).unwrap();
        assert!(stream.user_agent.starts_with("Mozilla/5.0"));
    }

    #[test]
    fn failure_text_is_readable() {
        assert!(describe_failure("ERROR: could not find firefox cookies database", "firefox").contains("Close firefox"));
        assert!(describe_failure("ERROR: Sign in to confirm you're not a bot", "brave").contains("music.youtube.com"));
        assert!(describe_failure("", "firefox").contains("without an error message"));
    }
}
