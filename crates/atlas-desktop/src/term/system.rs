//! The Mac changed under the terminals: it woke from sleep, or a display was added, removed or
//! reconfigured (lid, external monitor). A `CAMetalLayer` drawable can be lost or bound to the old
//! display across either, and nothing redraws it until bytes arrive: #104 (a blank tab whose pty
//! is alive). Every native terminal rebuilds its surface, keeping its screen and scrollback.
//!
//! Split in two: [`on_system_event`] is the logic (unit-tested with the registry as a parameter),
//! [`install`] only wires AppKit's notifications to it.

use std::sync::Arc;

use super::shared::Shared;
use super::watchdog::Why;

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
    use std::ptr::NonNull;

    fn fire(ev: SystemEvent) {
        // The registry lock is held only to clone the `Arc`s, never across the asks.
        let terms = super::all_shared();
        let n = on_system_event(ev, &terms);
        crate::diag::incident(
            "terminal-system-event",
            &format!("{ev:?}: rebuilding {n} terminal(s)"),
        );
    }

    let wake = RcBlock::new(|_: NonNull<objc2_foundation::NSNotification>| fire(SystemEvent::Wake));
    let screens = RcBlock::new(|_: NonNull<objc2_foundation::NSNotification>| {
        fire(SystemEvent::ScreensChanged)
    });
    // SAFETY: the blocks are `Fn` and touch only process-wide statics (`Send + Sync`); `queue:
    // None` runs them on the posting thread, and they only flip flags and notify condvars. The
    // returned observer tokens are leaked on purpose: the observation lasts as long as the app.
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
