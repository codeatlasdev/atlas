//! The cursor blink policy of one terminal, pure (no clock, no GPU): the render thread feeds it what happened and what
//! time it is, and it says when the loop must wake next and whether a tick changed what is drawn.
//!
//! A terminal at rest draws nothing. A blinking cursor is the one thing that used to wake the thread and draw a whole
//! frame every [`BLINK`] for as long as the window stayed focused (#774: the idle cost of the native terminal against
//! xterm.js). The blink now follows the VTE/GTK rule (`gtk-cursor-blink-timeout`): it runs for [`REST_AFTER`] after the
//! last output or input, then the cursor settles visible and the thread sleeps until woken. A hidden (parked) view never
//! wakes for it at all.

use std::time::{Duration, Instant};

/// Half a blink cycle: how long the cursor stays on, then off.
pub(super) const BLINK: Duration = Duration::from_millis(530);
/// The blink stops this long after the last output or input and leaves the cursor visible.
pub(super) const REST_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub(super) struct Blink {
    /// The phase to draw: `false` hides a blinking cursor.
    on: bool,
    /// When the phase flips next.
    next: Instant,
    /// The last output or input: the blink runs until `since + REST_AFTER`.
    since: Instant,
    /// The blink has stopped (cursor visible) until the next [`Blink::restart`].
    resting: bool,
}

impl Blink {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            on: true,
            next: now + BLINK,
            since: now,
            resting: false,
        }
    }

    /// Output or input: the cursor is visible and the cycle starts over.
    pub(super) fn restart(&mut self, now: Instant) {
        *self = Self::new(now);
    }

    /// The phase to draw.
    pub(super) fn on(&self) -> bool {
        self.on
    }

    /// When the thread must wake for the blink, or `None` (not blinking, hidden, or resting: sleep until woken).
    pub(super) fn deadline(&self, blinks: bool, hidden: bool) -> Option<Instant> {
        (blinks && !hidden && !self.resting).then_some(self.next)
    }

    /// The deadline passed at `now`. `true` when what is drawn changed (the phase flipped, or the cursor came back on
    /// as the blink rested), so a frame is owed; `false` when nothing changed and the loop goes back to sleep.
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        if self.resting || now < self.next {
            return false;
        }
        if now.saturating_duration_since(self.since) >= REST_AFTER {
            self.resting = true;
            return !std::mem::replace(&mut self.on, true);
        }
        self.on = !self.on;
        self.next = now + BLINK;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the policy the way the render thread does: wake at each deadline, tick, count the frames it owes, until
    /// `until` after `t0`. Returns `(frames drawn, wakeups)`.
    fn run(b: &mut Blink, t0: Instant, until: Duration, blinks: bool, hidden: bool) -> (u32, u32) {
        let (mut frames, mut wakes) = (0, 0);
        while let Some(at) = b.deadline(blinks, hidden) {
            if at > t0 + until {
                break;
            }
            wakes += 1;
            frames += u32::from(b.tick(at));
        }
        (frames, wakes)
    }

    #[test]
    fn a_blinking_cursor_flips_each_half_cycle() {
        let t0 = Instant::now();
        let mut b = Blink::new(t0);
        assert!(b.on());
        assert_eq!(b.deadline(true, false), Some(t0 + BLINK));
        assert!(b.tick(t0 + BLINK));
        assert!(!b.on());
        assert!(b.tick(t0 + BLINK * 2));
        assert!(b.on());
    }

    // Incident (#774): an idle focused terminal with a blinking cursor drew two whole frames a second forever; the native
    // terminal idled 1 point of a core above xterm.js. The blink must stop and the thread must stop waking.
    #[test]
    fn the_blink_rests_with_the_cursor_visible_and_the_thread_sleeps() {
        let t0 = Instant::now();
        let mut b = Blink::new(t0);
        let (frames, _) = run(&mut b, t0, REST_AFTER * 6, true, false);
        // About 19 flips in 10 s, then none in the next 50 s.
        assert!(
            (17..=21).contains(&frames),
            "{frames} frames while blinking"
        );
        assert!(b.on(), "a rested cursor is visible, never stuck off");
        assert_eq!(b.deadline(true, false), None, "no wakeup at rest");
    }

    // A cursor that rests in its off phase needs one last frame to come back on, then nothing.
    #[test]
    fn resting_in_the_off_phase_draws_the_cursor_back_once() {
        let t0 = Instant::now();
        // Off phase, deadline at the rest time (19 flips from `on` end on `off`).
        let mut b = Blink::new(t0);
        b.on = false;
        b.next = t0 + REST_AFTER;
        assert!(
            b.tick(t0 + REST_AFTER),
            "the cursor coming back is a change"
        );
        assert!(b.on());
        assert_eq!(b.deadline(true, false), None);
        // Already on at the rest time: nothing changed, no frame.
        let mut b = Blink::new(t0);
        b.next = t0 + REST_AFTER;
        assert!(
            !b.tick(t0 + REST_AFTER),
            "no frame for a phase that did not change"
        );
    }

    #[test]
    fn output_or_input_restarts_a_rested_blink() {
        let t0 = Instant::now();
        let mut b = Blink::new(t0);
        run(&mut b, t0, REST_AFTER * 2, true, false);
        assert_eq!(b.deadline(true, false), None);
        let t1 = t0 + REST_AFTER * 3;
        b.restart(t1);
        assert!(b.on());
        assert_eq!(b.deadline(true, false), Some(t1 + BLINK));
        let (frames, _) = run(&mut b, t1, Duration::from_secs(3), true, false);
        assert!(frames >= 4, "blinking again after the restart: {frames}");
    }

    #[test]
    fn a_parked_view_or_a_steady_cursor_never_wakes_the_thread() {
        let t0 = Instant::now();
        let b = Blink::new(t0);
        assert_eq!(b.deadline(true, true), None, "hidden");
        assert_eq!(b.deadline(false, false), None, "steady cursor");
    }

    // A deadline woken early (a spurious wakeup, output that did not restart) must not flip the phase.
    #[test]
    fn a_tick_before_the_deadline_changes_nothing() {
        let t0 = Instant::now();
        let mut b = Blink::new(t0);
        assert!(!b.tick(t0 + BLINK - Duration::from_millis(1)));
        assert!(b.on());
    }
}
