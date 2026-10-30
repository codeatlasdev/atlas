//! macOS side of the keystroke pill: the Input Monitoring permission, the global key monitor and the click-through
//! window at the bottom of the recorded region that shows the pill ([`super::keys`] decides what it says).
//!
//! The window is a capture surface ([`Surface::Keys`]) that is *not* excluded from the recording: the pill is part
//! of the picture. It never takes the mouse (`set_ignore_cursor_events`) or the keyboard (shown through
//! `surface::show`, like every capture window, and never built in an agent instance).
//!
//! Privacy: a key press is turned into its label inside the monitor and goes straight to the pill. Nothing is logged,
//! written to disk or kept after the recording; keys typed into a password field never reach a global monitor (the
//! system's secure input hides them).

use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use super::geom::Rect;
use super::keys::{self, Mods, Pill};
use super::surface::{self, Surface};

/// The pill window's size in points.
const SIZE: (f64, f64) = (420.0, 64.0);
/// Gap between the pill and the bottom edge of the recorded region.
const MARGIN: f64 = 28.0;

/// The Input Monitoring grant of this process.
pub fn granted() -> bool {
    objc2_core_graphics::CGPreflightListenEventAccess()
}

/// Shows the system prompt (once; macOS remembers the answer) and returns the current grant. It is asked only when
/// a recording starts with keystrokes on.
pub fn request() -> bool {
    objc2_core_graphics::CGRequestListenEventAccess()
}

/// The monitor object, kept to remove it. `Retained<AnyObject>` is not `Send`; it is only created, and removed,
/// under this mutex.
struct Monitor(Retained<AnyObject>);
// SAFETY: the token is only handed back to `NSEvent::removeMonitor`, which is thread-safe, and never dereferenced.
unsafe impl Send for Monitor {}

/// A running keystroke pill: stops with [`Keys::stop`].
pub struct Keys {
    monitor: Option<Monitor>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

fn mods_of(f: NSEventModifierFlags) -> Mods {
    Mods {
        ctrl: f.contains(NSEventModifierFlags::Control),
        alt: f.contains(NSEventModifierFlags::Option),
        shift: f.contains(NSEventModifierFlags::Shift),
        cmd: f.contains(NSEventModifierFlags::Command),
    }
}

/// Start the pill for a recording of `region` (global points) on `display` (global frame). Returns `Err` with a
/// sentence when it cannot: the caller shows it on the HUD and records without the pill.
pub fn start(app: &AppHandle, region: Rect, display: Rect) -> Result<Keys, String> {
    if !granted() {
        return Err(super::recopts::KEYS_SKIPPED.into());
    }
    let win = surface::build(app, Surface::Keys, 0, "")?;
    let (x, y) = keys::pill_origin(
        (region.x, region.y, region.w, region.h),
        (display.x, display.y, display.w, display.h),
        SIZE,
        MARGIN,
    );
    let _ = win.set_size(tauri::LogicalSize::new(SIZE.0, SIZE.1));
    let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    // The pill must never intercept a click or a key meant for what is being recorded.
    let _ = win.set_ignore_cursor_events(true);
    let _ = app.emit_to(Surface::Keys.label(0), "shots://keys", Vec::<String>::new());
    // Shown with the first key, not before: an empty pill would be a dark smear in every recording.

    let pill = Arc::new(Mutex::new(Pill::default()));
    let t0 = Instant::now();
    let handler_pill = pill.clone();
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| {
        // SAFETY: AppKit hands the handler a live event for the duration of the call.
        let ev = unsafe { ev.as_ref() };
        let flags = ev.modifierFlags();
        let code = ev.keyCode();
        let chars = ev
            .charactersIgnoringModifiers()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if let Some(label) = keys::format_key(mods_of(flags), code, &chars) {
            handler_pill
                .lock()
                .push(label, t0.elapsed().as_millis() as u64);
        }
    });
    let monitor =
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
            .map(Monitor)
            .ok_or("macOS refused the keyboard monitor.")?;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (a, s, p) = (app.clone(), stop.clone(), pill);
    std::thread::spawn(move || {
        let mut last: Vec<String> = Vec::new();
        let mut shown = false;
        while !s.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(80));
            let now = p.lock().visible(t0.elapsed().as_millis() as u64);
            if now == last {
                continue;
            }
            if !now.is_empty()
                && !shown
                && let Some(w) = a.get_webview_window(&Surface::Keys.label(0))
            {
                surface::show(&a, &w);
                shown = true;
            }
            let _ = a.emit_to(Surface::Keys.label(0), "shots://keys", &now);
            last = now;
        }
    });
    Ok(Keys {
        monitor: Some(monitor),
        stop,
    })
}

impl Keys {
    /// Remove the monitor, stop the thread and hide the pill. Safe to call once; the model dies with the thread.
    pub fn stop(mut self, app: &AppHandle) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(m) = self.monitor.take() {
            // SAFETY: the token came from `addGlobalMonitorForEventsMatchingMask`.
            unsafe { NSEvent::removeMonitor(&m.0) };
        }
        if let Some(w) = app.get_webview_window(&Surface::Keys.label(0)) {
            let _ = w.hide();
        }
    }
}
