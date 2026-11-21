//! When to look up the next track's stream (so it can follow without a gap or a wait) and when the
//! current track's stream is old enough that YouTube's URL may stop working.

use std::time::Duration;

/// YouTube stream URLs last about six hours; refresh with an hour to spare.
pub const STREAM_MAX_AGE: Duration = Duration::from_secs(5 * 60 * 60);

/// True once the track is playing and its length is known. The next track is looked up right
/// away (not just before the end) so the Next button can switch to it at once; the address stays
/// valid for hours.
pub fn should_prefetch(position: Option<f64>, duration: Option<f64>) -> bool {
    position.is_some_and(|position| position > 0.0) && duration.is_some_and(|duration| duration > 0.0)
}

/// True when a stream issued `age` ago should be looked up again.
pub fn stream_is_stale(age: Duration) -> bool {
    age >= STREAM_MAX_AGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetches_as_soon_as_the_track_is_playing() {
        assert!(!should_prefetch(None, Some(200.0)));
        assert!(!should_prefetch(Some(10.0), None));
        // A position of 0 means the track has not really started.
        assert!(!should_prefetch(Some(0.0), Some(200.0)));
        assert!(should_prefetch(Some(1.0), Some(200.0)));
        assert!(should_prefetch(Some(199.5), Some(200.0)));
    }

    #[test]
    fn streams_go_stale_after_five_hours() {
        assert!(!stream_is_stale(Duration::from_secs(4 * 60 * 60)));
        assert!(stream_is_stale(Duration::from_secs(5 * 60 * 60)));
        assert!(stream_is_stale(Duration::from_secs(7 * 60 * 60)));
    }
}
