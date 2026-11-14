//! The Mac changed under the terminals: it woke from sleep, or a display was added, removed or
//! reconfigured (lid, external monitor). A `CAMetalLayer` drawable can be lost or bound to the old
//! display across either, and nothing redraws it until bytes arrive: #104 (a blank tab whose pty
//! is alive). Every native terminal rebuilds its surface, keeping its screen and scrollback.
//!
//! Split in three: [`Coalescer`] is the burst logic (pure, the clock is a parameter),
//! [`on_system_event`] is what a settled event does (unit-tested with the registry as a
//! parameter), [`install`] only wires AppKit's notifications to them.
//!
//! Why a coalescer (#166): reconnecting a monitor after a wake posts `ScreensChanged` dozens of
//! times in a second (52 in the report), and every one rebuilt every terminal: dozens of
//! back-to-back `Surface::configure` calls on one shared GPU device, until one failed. A burst
//! now yields one rebuild, on its trailing edge, so the surface is made for the final layout.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::shared::Shared;
use super::watchdog::Why;

/// Quiet time after the last event of a burst before the terminals rebuild. Long enough to span
/// the notifications of one hotplug (they arrive within tens of ms of each other), short enough
/// that a wake is not visibly blank.
pub(super) const SETTLE: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SystemEvent {
    /// `NSWorkspaceDidWakeNotification`.
    Wake,
    /// `NSApplicationDidChangeScreenParametersNotification`.
    ScreensChanged,
}

impl SystemEvent {
    fn why(self) -> Why {
        match self {
            SystemEvent::Wake => Why::Wake,
            SystemEvent::ScreensChanged => Why::Screens,
        }
    }
}

/// Events seen but not yet acted on. Trailing-edge debounce: every event pushes the deadline out
/// to `SETTLE` after itself, so the action runs once the burst has gone quiet.
#[derive(Debug, Default)]
pub(super) struct Coalescer {
    pending: Option<(Instant, SystemEvent)>,
}

impl Coalescer {
    /// An event arrived at `now`. The deadline restarts; the event kept is the stronger one
    /// (`Wake` over `ScreensChanged`: it names the cause, and both rebuild the same way).
    pub(super) fn push(&mut self, now: Instant, ev: SystemEvent) {
        let ev = match self.pending {
            Some((_, SystemEvent::Wake)) => SystemEvent::Wake,
            _ => ev,
        };
        self.pending = Some((now + SETTLE, ev));
    }

    /// When the burst settles, if one is pending.
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending.map(|(d, _)| d)
    }

    /// The event to act on once the burst has been quiet for `SETTLE` as of `now`. Takes it.
    pub(super) fn take_due(&mut self, now: Instant) -> Option<SystemEvent> {
        match self.pending {
            Some((d, ev)) if d <= now => {
                self.pending = None;
                Some(ev)
            }
            _ => None,
        }
    }
}

/// Ask every terminal's render thread to rebuild its surface. Returns how many were asked. Only
/// sets a flag and wakes each thread: the rebuild itself runs there, so a terminal whose renderer
/// is stuck delays only its own recovery.
pub(super) fn on_system_event(ev: SystemEvent, terms: &[Arc<Shared>]) -> usize {
    for sh in terms {
        sh.rebuild(ev.why());
    }
    terms.len()
}

/// Observe wake and display changes for the life of the process (macOS; main thread at setup).
pub fn install() {
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSNotificationCenter, NSString};
    use parking_lot::{Condvar, Mutex};
    use std::ptr::NonNull;
    use std::sync::LazyLock;

    static BURST: Mutex<Coalescer> = Mutex::new(Coalescer { pending: None });
    static WAKE: Condvar = Condvar::new();
    /// The settle thread, started by the first event.
    static WORKER: LazyLock<()> = LazyLock::new(|| {
        let _ = std::thread::Builder::new()
            .name("atlas-term-system".into())
            .spawn(settle_loop);
    });

    /// Waits for a burst to settle, then rebuilds once. Never holds `BURST` while acting.
    fn settle_loop() {
        let mut burst = BURST.lock();
        loop {
            match burst.deadline() {
                None => WAKE.wait(&mut burst),
                Some(d) => {
                    if !WAKE.wait_until(&mut burst, d).timed_out() {
                        continue;
                    }
                    if let Some(ev) = burst.take_due(Instant::now()) {
                        drop(burst);
                        // The registry lock is held only to clone the `Arc`s, never across the asks.
                        let terms = super::all_shared();
                        let n = on_system_event(ev, &terms);
                        crate::diag::incident(
                            "terminal-system-event",
                            &format!("{ev:?}: rebuilding {n} terminal(s)"),
                        );
                        burst = BURST.lock();
                    }
                }
            }
        }
    }

    fn fire(ev: SystemEvent) {
        LazyLock::force(&WORKER);
        BURST.lock().push(Instant::now(), ev);
        WAKE.notify_one();
    }

    let wake = RcBlock::new(|_: NonNull<objc2_foundation::NSNotification>| fire(SystemEvent::Wake));
    let screens = RcBlock::new(|_: NonNull<objc2_foundation::NSNotification>| {
        fire(SystemEvent::ScreensChanged)
    });
    // SAFETY: the blocks are `Fn` and touch only process-wide statics (`Send + Sync`); `queue:
    // None` runs them on the posting thread, and they only record the event and notify a condvar.
    // The returned observer tokens are leaked on purpose: the observation lasts as long as the app.
    unsafe {
        let ws = NSWorkspace::sharedWorkspace().notificationCenter();
        let t = ws.addObserverForName_object_queue_usingBlock(
            Some(&NSString::from_str("NSWorkspaceDidWakeNotification")),
            None,
            None,
            &wake,
        );
        std::mem::forget(t);
        let t = NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&NSString::from_str(
                "NSApplicationDidChangeScreenParametersNotification",
            )),
            None,
            None,
            &screens,
        );
        std::mem::forget(t);
    }
    std::mem::forget(wake);
    std::mem::forget(screens);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the coalescer like the settle thread does: events at `at` (ms), polled at every ms.
    /// Returns the instants (ms) at which an action fired, with the event it fired for.
    fn run(events: &[(u64, SystemEvent)], until_ms: u64) -> Vec<(u64, SystemEvent)> {
        let t0 = Instant::now();
        let mut c = Coalescer::default();
        let mut fired = Vec::new();
        for ms in 0..=until_ms {
            let now = t0 + Duration::from_millis(ms);
            for (_, ev) in events.iter().filter(|(at, _)| *at == ms) {
                c.push(now, *ev);
            }
            if let Some(ev) = c.take_due(now) {
                fired.push((ms, ev));
            }
        }
        fired
    }

    // #166: 52 ScreensChanged in one second rebuilt every terminal 52 times; one rebuild now, on
    // the trailing edge (SETTLE after the last event), so the final display layout is the one used.
    #[test]
    fn a_burst_of_screen_changes_fires_once_on_the_trailing_edge() {
        let s = SETTLE.as_millis() as u64;
        let events: Vec<_> = (0..52)
            .map(|i| (i * 19, SystemEvent::ScreensChanged))
            .collect();
        let last = 51 * 19;
        let fired = run(&events, last + s + 200);
        assert_eq!(fired, vec![(last + s, SystemEvent::ScreensChanged)]);
    }

    // Events further apart than the window are separate incidents (a display plugged, later
    // unplugged): each gets its own rebuild.
    #[test]
    fn events_spaced_beyond_the_window_fire_separately() {
        let s = SETTLE.as_millis() as u64;
        let gap = s + 50;
        let events = [
            (0, SystemEvent::ScreensChanged),
            (gap, SystemEvent::ScreensChanged),
            (2 * gap, SystemEvent::ScreensChanged),
        ];
        let fired = run(&events, 2 * gap + s + 50);
        assert_eq!(
            fired.iter().map(|(ms, _)| *ms).collect::<Vec<_>>(),
            vec![s, gap + s, 2 * gap + s]
        );
    }

    // An event just inside the window pushes the deadline out instead of firing early: the rebuild
    // must not run while the layout is still changing.
    #[test]
    fn an_event_inside_the_window_postpones_the_deadline() {
        let s = SETTLE.as_millis() as u64;
        let fired = run(
            &[
                (0, SystemEvent::ScreensChanged),
                (s - 1, SystemEvent::ScreensChanged),
            ],
            2 * s + 10,
        );
        assert_eq!(fired, vec![(2 * s - 1, SystemEvent::ScreensChanged)]);
    }

    // A wake and the display re-enumeration it causes are one recovery, reported as the wake,
    // whichever order they arrive in.
    #[test]
    fn a_wake_and_the_screen_changes_around_it_fire_once_as_wake() {
        for order in [
            [SystemEvent::Wake, SystemEvent::ScreensChanged],
            [SystemEvent::ScreensChanged, SystemEvent::Wake],
        ] {
            let fired = run(&[(0, order[0]), (30, order[1])], 1000);
            assert_eq!(fired.len(), 1, "{order:?}");
            assert_eq!(fired[0].1, SystemEvent::Wake, "{order:?}");
        }
    }

    #[test]
    fn nothing_pending_means_no_deadline_and_nothing_due() {
        let mut c = Coalescer::default();
        assert_eq!(c.deadline(), None);
        assert_eq!(c.take_due(Instant::now() + Duration::from_secs(60)), None);
        let now = Instant::now();
        c.push(now, SystemEvent::ScreensChanged);
        assert_eq!(c.deadline(), Some(now + SETTLE));
        assert_eq!(c.take_due(now), None, "not before the window has passed");
        assert!(c.take_due(now + SETTLE).is_some());
        assert_eq!(c.deadline(), None, "taken once");
    }

    // The settled event asks every terminal exactly once, however many were registered.
    #[test]
    fn a_settled_event_asks_each_terminal_once() {
        let terms = [Shared::for_test(10, 2), Shared::for_test(10, 2)];
        assert_eq!(on_system_event(SystemEvent::ScreensChanged, &terms), 2);
        for t in &terms {
            assert_eq!(t.state.lock().rebuild, Some(Why::Screens));
        }
    }
}
