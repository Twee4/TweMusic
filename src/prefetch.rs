//! When to look up the next track's stream (so it can follow without a gap) and when the
//! current track's stream is old enough that YouTube's URL may stop working.

use std::time::Duration;

/// Start resolving the next track this long before the current one ends.
pub const PREFETCH_LEAD: f64 = 15.0;
/// Never prefetch for tracks shorter than this: the whole track is the lead time.
const MIN_DURATION: f64 = 20.0;
/// YouTube stream URLs last about six hours; refresh with an hour to spare.
pub const STREAM_MAX_AGE: Duration = Duration::from_secs(5 * 60 * 60);

/// True once the track is within `PREFETCH_LEAD` seconds of its end.
pub fn should_prefetch(position: Option<f64>, duration: Option<f64>) -> bool {
    let (Some(position), Some(duration)) = (position, duration) else { return false };
    duration >= MIN_DURATION && position > 0.0 && duration - position <= PREFETCH_LEAD
}

/// True when a stream issued `age` ago should be looked up again.
pub fn stream_is_stale(age: Duration) -> bool {
    age >= STREAM_MAX_AGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetches_only_near_the_end_of_a_known_track() {
        assert!(!should_prefetch(None, Some(200.0)));
        assert!(!should_prefetch(Some(10.0), None));
        assert!(!should_prefetch(Some(10.0), Some(200.0)));
        assert!(should_prefetch(Some(186.0), Some(200.0)));
        assert!(should_prefetch(Some(199.5), Some(200.0)));
        // A very short clip is not worth a prefetch.
        assert!(!should_prefetch(Some(5.0), Some(12.0)));
        // A position of 0 means the track has not really started.
        assert!(!should_prefetch(Some(0.0), Some(20.0)));
    }

    #[test]
    fn streams_go_stale_after_five_hours() {
        assert!(!stream_is_stale(Duration::from_secs(4 * 60 * 60)));
        assert!(stream_is_stale(Duration::from_secs(5 * 60 * 60)));
        assert!(stream_is_stale(Duration::from_secs(7 * 60 * 60)));
    }
}
