//! Song lyrics: parsing time-synced (LRC) text and finding the line that is sung right now.

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Seconds from the start of the song; 0 for lyrics without timing.
    pub time: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// True when every line carries a time, so the current line can follow the song.
    pub synced: bool,
}

/// Parses LRC text such as `[01:23.45] some words`. A line may carry several time tags; lines
/// without any are skipped (metadata such as `[ar:Artist]` has no digits before the colon).
pub fn parse_synced(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            match parse_time(&stripped[..end]) {
                Some(seconds) => times.push(seconds),
                None => break,
            }
            rest = stripped[end + 1..].trim_start();
        }
        for time in times {
            lines.push(Line { time, text: rest.trim().to_owned() });
        }
    }
    lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    lines
}

fn parse_time(tag: &str) -> Option<f64> {
    let (minutes, seconds) = tag.split_once(':')?;
    let minutes: f64 = minutes.trim().parse().ok()?;
    let seconds: f64 = seconds.trim().parse().ok()?;
    (minutes >= 0.0 && seconds >= 0.0).then_some(minutes * 60.0 + seconds)
}

/// Lyrics without timing: one line per line of text.
pub fn parse_plain(text: &str) -> Vec<Line> {
    text.lines().map(|line| Line { time: 0.0, text: line.trim().to_owned() }).collect()
}

/// Builds lyrics from what a lyrics service returned, preferring the time-synced text.
pub fn from_texts(synced: Option<&str>, plain: Option<&str>) -> Option<Lyrics> {
    if let Some(text) = synced {
        let lines = parse_synced(text);
        if !lines.is_empty() {
            return Some(Lyrics { lines, synced: true });
        }
    }
    let lines = parse_plain(plain?);
    lines.iter().any(|line| !line.text.is_empty()).then_some(Lyrics { lines, synced: false })
}

/// The index of the line being sung at `position` seconds (the last one that has started).
pub fn current_line(lines: &[Line], position: f64) -> Option<usize> {
    let started = lines.partition_point(|line| line.time <= position + 0.15);
    started.checked_sub(1)
}

/// A title without the extras that stop a lyrics search from matching: `(feat. X)`, `[Official]`
/// and anything after a dash.
pub fn clean_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in title.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let first = out.split(" - ").next().unwrap_or("").trim();
    first.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_time_tags_and_skips_metadata() {
        let lines = parse_synced("[ar:Someone]\n[00:12.50] First\n[01:02.00][02:00.00] Chorus\n[00:30.00]\n");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], Line { time: 12.5, text: "First".into() });
        assert_eq!(lines[1].time, 30.0);
        assert_eq!(lines[1].text, "");
        assert_eq!(lines[2], Line { time: 62.0, text: "Chorus".into() });
        assert_eq!(lines[3].time, 120.0);
    }

    #[test]
    fn finds_the_line_being_sung() {
        let lines = parse_synced("[00:05.00] a\n[00:10.00] b\n[00:20.00] c");
        assert_eq!(current_line(&lines, 1.0), None);
        assert_eq!(current_line(&lines, 5.0), Some(0));
        assert_eq!(current_line(&lines, 12.0), Some(1));
        assert_eq!(current_line(&lines, 99.0), Some(2));
    }

    #[test]
    fn prefers_synced_text_and_falls_back_to_plain() {
        let both = from_texts(Some("[00:01.00] hi"), Some("hi")).unwrap();
        assert!(both.synced);
        let plain = from_texts(None, Some("one\ntwo")).unwrap();
        assert!(!plain.synced);
        assert_eq!(plain.lines.len(), 2);
        assert!(from_texts(Some("no tags here"), Some("  \n ")).is_none());
        assert!(from_texts(None, None).is_none());
    }

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Headlights (feat. KIDDO)"), "Headlights");
        assert_eq!(clean_title("Opurimananku - Sharin Amud Shapri - [With Malay translation]"), "Opurimananku");
    }
}
