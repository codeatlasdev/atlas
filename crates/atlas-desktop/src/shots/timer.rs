//! The self-timer of a still (3, 5 or 10 seconds): the overlay is gone, the screen is yours to arrange, a small
//! count shows on the quick-card corner, then the shot is taken.
//!
//! The waiting is a plain sleep on the capture worker thread (never the main thread); `ticks` is the pure part:
//! what the countdown shows and when.

use std::time::Duration;

use tauri::{AppHandle, Emitter};

/// The numbers a `secs` countdown shows, one per second, last one `1`: `3 → [3, 2, 1]`.
pub fn ticks(secs: u32) -> Vec<u32> {
    (1..=secs).rev().collect()
}

/// Block the capture worker for `secs`, telling the page each second (`shots://timer`, the number left; `0` when
/// it is over). `secs == 0` returns at once and emits nothing.
pub fn wait(app: &AppHandle, secs: u32) {
    wait_with(secs, Duration::from_secs(1), |n| {
        let _ = app.emit("shots://timer", n);
    });
}

/// [`wait`] with the clock and the sink injected.
fn wait_with(secs: u32, step: Duration, mut show: impl FnMut(u32)) {
    for n in ticks(secs) {
        show(n);
        std::thread::sleep(step);
    }
    if secs > 0 {
        show(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_countdown_shows_each_second_down_to_one() {
        assert_eq!(ticks(3), [3, 2, 1]);
        assert_eq!(ticks(10).len(), 10);
        assert_eq!(ticks(10)[0], 10);
        assert!(ticks(0).is_empty());
    }

    /// No timer means no event and no sleep: an immediate capture must not pay for the feature.
    #[test]
    fn a_zero_timer_does_nothing() {
        let mut seen = Vec::new();
        let t = std::time::Instant::now();
        wait_with(0, Duration::from_secs(5), |n| seen.push(n));
        assert!(seen.is_empty());
        assert!(t.elapsed() < Duration::from_millis(100));
    }

    /// The page hides its countdown on `0`; it is sent exactly once, after the last number.
    #[test]
    fn the_end_of_the_countdown_is_announced_once() {
        let mut seen = Vec::new();
        wait_with(3, Duration::from_millis(1), |n| seen.push(n));
        assert_eq!(seen, [3, 2, 1, 0]);
    }
}
