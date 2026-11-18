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
    wait_with(
        secs,
        Duration::from_secs(1),
        |n| {
            let _ = app.emit("shots://timer", n);
        },
        std::thread::sleep,
    );
}

/// [`wait`] with the sink and the sleeper injected.
fn wait_with(
    secs: u32,
    step: Duration,
    mut show: impl FnMut(u32),
    mut sleep: impl FnMut(Duration),
) {
    for n in ticks(secs) {
        show(n);
        sleep(step);
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

    /// No timer means no event and no sleep: an immediate capture must not pay for the feature. The sleeper counts
    /// the sleeps asked for, so the guard is "none were asked", not a stopwatch.
    #[test]
    fn a_zero_timer_does_nothing() {
        let mut seen = Vec::new();
        let mut slept = Vec::new();
        wait_with(
            0,
            Duration::from_secs(5),
            |n| seen.push(n),
            |d| slept.push(d),
        );
        assert!(seen.is_empty());
        assert!(slept.is_empty(), "a zero timer slept: {slept:?}");
    }

    /// The page hides its countdown on `0`; it is sent exactly once, after the last number. Each number stays up
    /// for one step: it is shown, then the worker sleeps a step, then the next.
    #[test]
    fn the_end_of_the_countdown_is_announced_once() {
        let step = Duration::from_millis(7);
        let log = std::cell::RefCell::new(Vec::new());
        wait_with(
            3,
            step,
            |n| log.borrow_mut().push(format!("show {n}")),
            |d| log.borrow_mut().push(format!("sleep {d:?}")),
        );
        let sleep = format!("sleep {step:?}");
        assert_eq!(
            log.into_inner(),
            [
                "show 3", &sleep, "show 2", &sleep, "show 1", &sleep, "show 0"
            ]
        );
    }
}
