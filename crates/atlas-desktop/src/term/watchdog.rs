//! Per-terminal stall detection, pure logic (no clock, no AppKit): the render thread feeds it what
//! happened and what time it is, and it says what to do next.
//!
//! A terminal is *owed a frame* from the moment something asks for one (pty bytes, input, a resize,
//! a wake from sleep) until `Renderer::render` presents one. `render` returning `Ok(false)` (the
//! swapchain had no texture: lost or outdated after sleep, a display change) or `Err` used to leave
//! the tab blank until the next byte, because nothing asked again: that is #104. And a renderer
//! that never returns, or never succeeds, left the tab dead with no sign of it (#108).
//!
//! Escalation, cheapest first, all driven by the time a frame has been owed:
//! 1. [`Action::Redraw`] every [`REDRAW_EVERY`]: render again with no new bytes (the renderer
//!    reconfigures a lost surface on the failed call, so the retry usually draws).
//! 2. [`Action::Rebuild`] once [`REBUILD_AFTER`] has passed: a new view and `Renderer` (the `Vt`,
//!    and so the screen and scrollback, is untouched), at most once per [`REBUILD_AFTER`] so a
//!    machine with no working GPU does not loop.

use std::time::{Duration, Instant};

/// A frame owed this long: draw again.
pub(super) const REDRAW_EVERY: Duration = Duration::from_millis(250);
/// A frame owed this long (the issue's "~2 s"): the surface is not coming back, rebuild it.
pub(super) const REBUILD_AFTER: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    /// Nothing is owed, or not yet time to act.
    None,
    Redraw,
    Rebuild,
}

/// Why the render thread should rebuild the surface; the incident detail in `diag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Why {
    /// The watchdog: a frame was owed for [`REBUILD_AFTER`].
    Stalled,
    /// The user asked ("Reload terminal").
    Manual,
    /// The Mac woke from sleep.
    Wake,
    /// A display was added, removed or reconfigured.
    Screens,
}

impl Why {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Why::Stalled => "stalled",
            Why::Manual => "reload",
            Why::Wake => "wake",
            Why::Screens => "screens",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Watchdog {
    /// Since when a frame is owed; `None` = up to date.
    owed_since: Option<Instant>,
    last_redraw: Option<Instant>,
    last_rebuild: Option<Instant>,
    /// The last frame that reached the layer.
    last_presented: Option<Instant>,
}

impl Watchdog {
    /// Something wants a frame (bytes, input, resize, wake). Keeps the earliest time: more bytes
    /// arriving does not postpone the deadline of a frame that is already late.
    pub(super) fn owe(&mut self, now: Instant) {
        self.owed_since.get_or_insert(now);
    }

    /// Nothing is owed any more because nothing is being shown (a parked view is not drawn):
    /// not a stall, and `owe` starts a fresh budget when it is shown again.
    pub(super) fn settle(&mut self) {
        self.owed_since = None;
        self.last_redraw = None;
    }

    /// `render` presented a frame.
    pub(super) fn presented(&mut self, now: Instant) {
        self.owed_since = None;
        self.last_redraw = None;
        self.last_presented = Some(now);
    }

    /// A new surface replaced the old one: its first frame is owed from now, with a fresh budget.
    pub(super) fn rebuilt(&mut self, now: Instant) {
        self.last_rebuild = Some(now);
        self.owed_since = Some(now);
        self.last_redraw = Some(now);
    }

    /// What to do at `now`.
    pub(super) fn poll(&mut self, now: Instant) -> Action {
        let Some(since) = self.owed_since else {
            return Action::None;
        };
        let owed = now.saturating_duration_since(since);
        let rebuilt_recently = self
            .last_rebuild
            .is_some_and(|r| now.saturating_duration_since(r) < REBUILD_AFTER);
        if owed >= REBUILD_AFTER && !rebuilt_recently {
            return Action::Rebuild;
        }
        let due = self
            .last_redraw
            .is_none_or(|r| now.saturating_duration_since(r) >= REDRAW_EVERY);
        if due {
            self.last_redraw = Some(now);
            return Action::Redraw;
        }
        Action::None
    }

    /// The next instant `poll` can return something other than `None`, for the render thread's
    /// timed wait; `None` when no frame is owed (sleep until woken).
    pub(super) fn next_check(&self, now: Instant) -> Option<Instant> {
        let since = self.owed_since?;
        let redraw = self.last_redraw.map_or(now, |r| r + REDRAW_EVERY);
        let rebuild = [
            Some(since + REBUILD_AFTER),
            self.last_rebuild.map(|r| r + REBUILD_AFTER),
        ]
        .into_iter()
        .flatten()
        .max()
        .expect("since is always present");
        Some(redraw.min(rebuild).max(now))
    }

    /// Age of the last presented frame, for the incident record.
    pub(super) fn frame_age(&self, now: Instant) -> Option<Duration> {
        self.last_presented
            .map(|p| now.saturating_duration_since(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn up_to_date_terminal_is_left_alone_however_long_it_idles() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        w.presented(ms(t0, 5));
        assert_eq!(w.poll(ms(t0, 60_000)), Action::None);
        assert_eq!(w.next_check(ms(t0, 60_000)), None);
    }

    // Regression #104: `render` said Ok(false) after wake, nothing asked again, tab stayed blank.
    #[test]
    fn a_frame_that_failed_to_present_is_retried_without_new_bytes() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        assert_eq!(w.poll(t0), Action::Redraw, "first attempt is immediate");
        assert_eq!(w.poll(ms(t0, 100)), Action::None, "no busy loop");
        assert_eq!(
            w.poll(ms(t0, 250)),
            Action::Redraw,
            "then every REDRAW_EVERY"
        );
        assert_eq!(w.poll(ms(t0, 500)), Action::Redraw);
    }

    #[test]
    fn a_retry_that_presents_ends_the_escalation() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        assert_eq!(w.poll(t0), Action::Redraw);
        w.presented(ms(t0, 260));
        assert_eq!(
            w.poll(ms(t0, 3_000)),
            Action::None,
            "never rebuilds a healthy terminal"
        );
        // A later stall starts a fresh budget.
        w.owe(ms(t0, 10_000));
        assert_eq!(w.poll(ms(t0, 10_000)), Action::Redraw);
    }

    #[test]
    fn a_frame_owed_for_two_seconds_rebuilds_the_surface() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        w.poll(t0);
        assert_eq!(w.poll(ms(t0, 1_999)), Action::Redraw);
        assert_eq!(w.poll(ms(t0, 2_000)), Action::Rebuild);
    }

    // More output piling up behind a dead renderer must not keep pushing the deadline out.
    #[test]
    fn new_bytes_do_not_postpone_the_rebuild() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        for i in 1..20 {
            w.owe(ms(t0, i * 100));
        }
        assert_eq!(w.poll(ms(t0, 2_000)), Action::Rebuild);
    }

    #[test]
    fn a_broken_gpu_rebuilds_at_most_once_per_window_not_in_a_loop() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        assert_eq!(w.poll(ms(t0, 2_000)), Action::Rebuild);
        w.rebuilt(ms(t0, 2_000));
        // The new surface fails too: redraws continue, but no second rebuild inside the window.
        assert_eq!(w.poll(ms(t0, 3_999)), Action::Redraw);
        assert_ne!(w.poll(ms(t0, 3_999)), Action::Rebuild);
        assert_eq!(w.poll(ms(t0, 4_000)), Action::Rebuild);
    }

    #[test]
    fn a_rebuilt_surface_that_presents_is_healthy_again() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        w.owe(t0);
        w.rebuilt(ms(t0, 2_000));
        w.presented(ms(t0, 2_010));
        assert_eq!(w.poll(ms(t0, 9_000)), Action::None);
        assert_eq!(w.frame_age(ms(t0, 9_010)), Some(Duration::from_secs(7)));
    }

    #[test]
    fn next_check_is_when_the_next_action_becomes_due_never_in_the_past() {
        let t0 = Instant::now();
        let mut w = Watchdog::default();
        assert_eq!(w.next_check(t0), None);
        w.owe(t0);
        assert_eq!(w.next_check(t0), Some(t0), "first redraw is due at once");
        w.poll(t0);
        assert_eq!(w.next_check(ms(t0, 10)), Some(ms(t0, 250)));
        // Long after both deadlines: due now, not in the past.
        assert_eq!(w.next_check(ms(t0, 5_000)), Some(ms(t0, 5_000)));
    }

    #[test]
    fn frame_age_is_unknown_before_the_first_frame() {
        assert_eq!(Watchdog::default().frame_age(Instant::now()), None);
    }
}
