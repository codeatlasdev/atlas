//! The recording clock: what the HUD shows. Pure (times are parameters), so pause, resume, the limit and the
//! countdown are tested without a screen.
//!
//! Elapsed time counts only the time actually being recorded: a pause freezes it. A recording ends by itself at the
//! limit. The 3-2-1 countdown before it is its own phase with its own elapsed value (the HUD draws the digit from it).

use std::time::Duration;

/// The countdown before a recording starts.
pub const COUNTDOWN: Duration = Duration::from_secs(3);

/// A recording stops by itself after this long (the HUD shows the last 30 s).
pub const MAX_MS: u64 = 10 * 60 * 1000;

/// Elapsed recording time: the segments that already ended plus the one running now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    /// Milliseconds of the segments that ended (a pause ends one).
    done_ms: u64,
    /// When the running segment started, as milliseconds on the caller's clock; `None` while paused or stopped.
    running_since: Option<u64>,
}

impl Clock {
    /// Start counting at `now_ms` (any monotonic millisecond clock).
    pub fn start(now_ms: u64) -> Self {
        Self {
            done_ms: 0,
            running_since: Some(now_ms),
        }
    }

    /// Recorded milliseconds at `now_ms`, never above [`MAX_MS`].
    pub fn elapsed(&self, now_ms: u64) -> u64 {
        let live = self.running_since.map_or(0, |s| now_ms.saturating_sub(s));
        (self.done_ms + live).min(MAX_MS)
    }

    /// Freeze the elapsed time. A second pause changes nothing.
    pub fn pause(&mut self, now_ms: u64) {
        if let Some(s) = self.running_since.take() {
            self.done_ms += now_ms.saturating_sub(s);
        }
    }

    /// Count again. Resuming a running clock changes nothing.
    pub fn resume(&mut self, now_ms: u64) {
        if self.running_since.is_none() {
            self.running_since = Some(now_ms);
        }
    }

    pub fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    /// The limit has been reached: the recording must stop now.
    pub fn at_limit(&self, now_ms: u64) -> bool {
        self.elapsed(now_ms) >= MAX_MS
    }
}

/// How far into the countdown `since` is, in milliseconds, capped at its length (the HUD holds the last digit).
pub fn countdown_ms(since: Duration) -> u64 {
    since.min(COUNTDOWN).as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_counts_from_the_start() {
        let c = Clock::start(1_000);
        assert_eq!(c.elapsed(1_000), 0);
        assert_eq!(c.elapsed(4_500), 3_500);
    }

    /// A pause freezes the HUD timer: the time spent paused never counts, however long it lasts.
    #[test]
    fn a_pause_freezes_the_time_and_a_resume_continues_from_it() {
        let mut c = Clock::start(0);
        c.pause(2_000);
        assert_eq!(c.elapsed(2_000), 2_000);
        assert_eq!(
            c.elapsed(60_000),
            2_000,
            "a minute paused is not recorded time"
        );
        assert!(!c.is_running());
        c.resume(60_000);
        assert_eq!(c.elapsed(61_500), 3_500);
        assert!(c.is_running());
    }

    #[test]
    fn pausing_twice_or_resuming_twice_changes_nothing() {
        let mut c = Clock::start(0);
        c.pause(1_000);
        c.pause(5_000);
        assert_eq!(
            c.elapsed(9_000),
            1_000,
            "the second pause must not add the time in between"
        );
        c.resume(10_000);
        c.resume(12_000);
        assert_eq!(
            c.elapsed(13_000),
            4_000,
            "the second resume must not restart the segment"
        );
    }

    /// The limit is on recorded time, not wall time: a long pause does not use it up, and the clock never shows
    /// more than the limit (a late tick must not draw 10:01).
    #[test]
    fn the_limit_is_on_recorded_time_and_never_exceeded() {
        let mut c = Clock::start(0);
        c.pause(MAX_MS - 1_000);
        assert!(!c.at_limit(10 * MAX_MS));
        c.resume(10 * MAX_MS);
        assert!(!c.at_limit(10 * MAX_MS + 999));
        assert!(c.at_limit(10 * MAX_MS + 1_000));
        assert_eq!(c.elapsed(10 * MAX_MS + 50_000), MAX_MS);
    }

    /// A clock reading from before the start (the caller's clock stepped back) is zero, not a wrapped huge number.
    #[test]
    fn a_clock_that_steps_back_reads_zero_not_a_wrapped_value() {
        let c = Clock::start(5_000);
        assert_eq!(c.elapsed(1_000), 0);
        let mut p = Clock::start(5_000);
        p.pause(1_000);
        assert_eq!(p.elapsed(9_000), 0);
    }

    #[test]
    fn the_countdown_is_capped_at_three_seconds() {
        assert_eq!(countdown_ms(Duration::from_millis(0)), 0);
        assert_eq!(countdown_ms(Duration::from_millis(1_200)), 1_200);
        assert_eq!(countdown_ms(Duration::from_secs(9)), 3_000);
    }
}
