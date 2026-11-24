//! When a hidden terminal gives its GPU back (#254), pure logic (no clock, no GPU).
//!
//! A terminal whose view the page parked (a background tab or worktree, a dialog over it) draws nothing, but its
//! `Renderer` kept a swapchain, the glyph and colour atlases and the vertex buffers: about 12-15 MB of VRAM each on
//! Linux, 20 terminals of a crew = 250-300 MB for views nobody sees. The render thread drops that renderer once the
//! view has stayed hidden for [`RELEASE_AFTER`], and builds a new one on the same view when it is shown again. The
//! screen and scrollback live in the `Vt`, so nothing is lost; the view is off screen until the new renderer's first
//! frame (`Placement`), so the rebuild is never seen.
//!
//! The delay keeps a quick back-and-forth (worktree A, B, A) from paying a rebuild each time, and a hidden terminal
//! that is about to be shown again (a dialog opening) from releasing at all.

use std::time::{Duration, Instant};

/// How long a view must stay hidden before its renderer is dropped.
pub(super) const RELEASE_AFTER: Duration = Duration::from_secs(30);

/// Linux only for now: its rebuild on show is measured (docs/perf/terminal-linux-gpu.md). macOS keeps its renderers
/// until the same numbers exist there.
pub(super) const ENABLED: bool = cfg!(target_os = "linux");

#[derive(Debug, Clone, Copy)]
pub(super) struct Release {
    after: Option<Duration>,
    /// Since when the view has been continuously hidden; `None` while it is shown.
    hidden_since: Option<Instant>,
}

impl Release {
    /// `after`: `None` never releases (the platforms that have not opted in).
    pub(super) fn new(after: Option<Duration>) -> Self {
        Self {
            after,
            hidden_since: None,
        }
    }

    /// What the loop sees at `now`: is the view hidden? Keeps the earliest instant of an unbroken hidden spell.
    pub(super) fn observe(&mut self, hidden: bool, now: Instant) {
        if hidden {
            self.hidden_since.get_or_insert(now);
        } else {
            self.hidden_since = None;
        }
    }

    /// When the renderer is due to be dropped, for the loop's timed wait; `None` when shown or never releasing.
    pub(super) fn deadline(&self) -> Option<Instant> {
        Some(self.hidden_since? + self.after?)
    }

    /// Drop the renderer now?
    pub(super) fn due(&self, now: Instant) -> bool {
        self.deadline().is_some_and(|d| d <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(30);

    #[test]
    fn a_hidden_view_is_released_only_after_the_whole_delay() {
        let t0 = Instant::now();
        let mut r = Release::new(Some(T));
        r.observe(true, t0);
        assert_eq!(r.deadline(), Some(t0 + T));
        assert!(!r.due(t0 + T - Duration::from_millis(1)));
        assert!(r.due(t0 + T));
    }

    // A worktree switch A, B, A shows the terminal again before the delay: that must restart the delay, not resume it
    // (or a terminal hidden for 29 s twice in a minute would be rebuilt on the second show).
    #[test]
    fn showing_the_view_restarts_the_delay() {
        let t0 = Instant::now();
        let mut r = Release::new(Some(T));
        r.observe(true, t0);
        r.observe(false, t0 + Duration::from_secs(29));
        assert_eq!(r.deadline(), None, "shown: nothing is due");
        r.observe(true, t0 + Duration::from_secs(40));
        assert!(!r.due(t0 + Duration::from_secs(40) + T - Duration::from_millis(1)));
        assert!(r.due(t0 + Duration::from_secs(40) + T));
    }

    // Observing again while still hidden (every wake of the loop does) must not push the deadline out: a terminal that
    // receives output all the time while hidden would never release.
    #[test]
    fn observing_a_hidden_view_again_keeps_the_first_instant() {
        let t0 = Instant::now();
        let mut r = Release::new(Some(T));
        r.observe(true, t0);
        for s in 1..30 {
            r.observe(true, t0 + Duration::from_secs(s));
        }
        assert!(r.due(t0 + T));
    }

    #[test]
    fn a_platform_that_did_not_opt_in_never_releases() {
        let t0 = Instant::now();
        let mut r = Release::new(None);
        r.observe(true, t0);
        assert_eq!(r.deadline(), None);
        assert!(!r.due(t0 + Duration::from_secs(86_400)));
    }

    #[test]
    fn a_shown_view_is_never_due() {
        let mut r = Release::new(Some(T));
        r.observe(false, Instant::now());
        assert!(!r.due(Instant::now() + Duration::from_secs(86_400)));
    }
}
