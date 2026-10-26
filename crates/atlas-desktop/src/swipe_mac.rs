//! Native two-finger swipe, AppKit side. A thin adapter: an `NSEvent` local monitor for scroll-wheel events
//! turns each event into an engine [`Sample`], [`Engine`] (swipe_engine.rs) decides everything (claim, 1:1
//! tracking, rubber band, release decision, critically damped settle), and this module only
//!
//!  * resolves the zone under the pointer (zones pushed by the frontend),
//!  * swallows the events the engine claims (the page must not scroll under a swipe, momentum tail included),
//!  * runs the engine's clock: a display-rate timer while a gesture is in flight,
//!  * reports frames and decisions to the frontend, stamped with the gesture's `seq`,
//!  * records the raw event stream (`gesture-record`) and replays one through the same code path (`replay`).
//!
//! AppKit's own `trackSwipeEvent` is not used: its `gestureAmount` is a fixed `scrollingDeltaX / 1000` whatever
//! the view size (measured), so a half-page drag needs a 500 pt reach, and its settle is opaque (it raced the JS
//! pager). The engine owns the physics; the only thing taken from AppKit is the event stream.

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSEvent, NSEventMask, NSEventPhase, NSHapticFeedbackManager, NSHapticFeedbackPattern,
    NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer, NSWindow,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::ptr::NonNull;
use tauri::{AppHandle, Emitter, Manager};

use crate::swipe_engine::{Engine, Hit, Momentum, PAGE_TRAVEL, Phase, Sample, Step};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Zone {
    zone: String,
    id: Option<String>,
    rect: Rect,
    prev: bool,
    next: bool,
    /// `history` only: nothing under the pointer scrolls horizontally.
    #[serde(default)]
    pinned: bool,
}

/// Every frame of a gesture, to the frontend. `seq` grows once per claimed gesture.
#[derive(Serialize, Clone)]
struct FrameMsg<'a> {
    zone: &'a str,
    id: Option<&'a str>,
    seq: u64,
    amount: f64,
    stage: crate::swipe_engine::Stage,
}

/// The release decision, once per `seq`.
#[derive(Serialize, Clone)]
struct CommitMsg<'a> {
    zone: &'a str,
    id: Option<&'a str>,
    seq: u64,
    dir: i8,
}

/// Rects where something scrolls horizontally itself (terminal, editor, an overflowing list): the Back/Forward swipe
/// never claims inside them. Pushed by the frontend next to the zones.
static SCROLLERS: Mutex<Vec<Rect>> = Mutex::new(Vec::new());

/// Zones, front to back (later entries are nested deeper: the first hit from the end wins).
static ZONES: Mutex<Vec<Zone>> = Mutex::new(Vec::new());

/// The engine plus what the adapter needs around it. Main thread only (the monitor and the timer both run
/// there); the `Mutex` is for the `static`.
struct Adapter {
    engine: Engine,
    /// Every decision since the last `replay` started (for the report).
    commits: Vec<(u64, i8)>,
    /// The zone the current gesture claimed.
    zone: Option<Zone>,
    rec: Option<Recording>,
}

static ADAPTER: Mutex<Option<Adapter>> = Mutex::new(None);

/// A raw trace under recording: what the monitor saw, and what the engine did with it.
struct Recording {
    events: Vec<RecEvent>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RecEvent {
    #[serde(flatten)]
    pub sample: Sample,
    /// `Some` once a gesture claimed this event: the engine's amount after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed: Option<bool>,
}

/// Whether native tracking is available (the app is on macOS and the monitor is installed).
pub fn enabled() -> bool {
    NSEvent::isSwipeTrackingFromScrollEventsEnabled()
}

/// The zones the engine hit-tests right now (the gesture bench aims its traces at these).
#[tauri::command]
pub fn swipe_zones_now() -> serde_json::Value {
    serde_json::to_value(&*ZONES.lock()).unwrap_or_default()
}

pub fn set_scrollers(rects: Vec<Rect>) {
    *SCROLLERS.lock() = rects;
}

pub fn set_zones(zones: Vec<Zone>) {
    *ZONES.lock() = zones;
}

/// Innermost zone containing the top-left point `(x, y)` that can move in either direction.
fn hit(x: f64, y: f64) -> Option<Zone> {
    let zones = ZONES.lock();
    zones
        .iter()
        .rev()
        .find(|z| {
            let r = z.rect;
            x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
        })
        .filter(|z| (z.zone != "history" || z.pinned) && (z.prev || z.next))
        .filter(|z| z.zone != "history" || !in_scroller(x, y))
        .cloned()
}

fn in_scroller(x: f64, y: f64) -> bool {
    SCROLLERS
        .lock()
        .iter()
        .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
}

fn phase_of(p: NSEventPhase) -> Phase {
    if p.contains(NSEventPhase::Began) {
        Phase::Began
    } else if p.contains(NSEventPhase::Ended) {
        Phase::Ended
    } else if p.contains(NSEventPhase::Cancelled) {
        Phase::Cancelled
    } else if p.contains(NSEventPhase::MayBegin) {
        Phase::MayBegin
    } else if p.contains(NSEventPhase::Changed) || p.contains(NSEventPhase::Stationary) {
        Phase::Changed
    } else {
        Phase::None
    }
}

fn momentum_of(p: NSEventPhase) -> Momentum {
    if p.contains(NSEventPhase::Began) {
        Momentum::Began
    } else if p.contains(NSEventPhase::Ended) || p.contains(NSEventPhase::Cancelled) {
        Momentum::Ended
    } else if p.is_empty() {
        Momentum::None
    } else {
        Momentum::Changed
    }
}

fn tick_haptic() {
    let p = NSHapticFeedbackManager::defaultPerformer();
    p.performFeedbackPattern_performanceTime(
        NSHapticFeedbackPattern::Alignment,
        NSHapticFeedbackPerformanceTime::Now,
    );
}

/// Top-left point of the event in the main window's content view, if it belongs to the main window.
fn point_in_main(app: &AppHandle, ev: &NSEvent, mtm: MainThreadMarker) -> Option<(f64, f64)> {
    let main = app.get_window("main")?.ns_window().ok()?;
    let win: Retained<NSWindow> = ev.window(mtm)?;
    if !std::ptr::eq(
        Retained::as_ptr(&win) as *const AnyObject,
        main as *const AnyObject,
    ) {
        return None;
    }
    let content = win.contentView()?;
    let p = content.convertPoint_fromView(ev.locationInWindow(), None);
    let h = content.bounds().size.height;
    Some((p.x, if content.isFlipped() { p.y } else { h - p.y }))
}

/// The one place an `NSEvent` becomes a [`Sample`]. `scrollingDeltaX` is already direction-corrected by AppKit
/// (natural scrolling: fingers left = negative), which is what the engine's convention is. The replay cannot
/// cover this function (it starts from samples); everything after it is the same code in both paths.
fn sample_of(ev: &NSEvent, p: (f64, f64)) -> Sample {
    Sample {
        // `NSEvent.timestamp` is the event's own hardware time (s since boot): jitter-free pacing.
        t: ev.timestamp() * 1000.0,
        phase: phase_of(ev.phase()),
        momentum: momentum_of(ev.momentumPhase()),
        dx: ev.scrollingDeltaX(),
        dy: ev.scrollingDeltaY(),
        precise: ev.hasPreciseScrollingDeltas(),
        x: p.0,
        y: p.1,
    }
}

/// Install the monitor. Main thread (Tauri `setup`).
pub fn install(app: &AppHandle) {
    *ADAPTER.lock() = Some(Adapter {
        engine: Engine::new(),
        commits: Vec::new(),
        zone: None,
        rec: None,
    });
    let app = app.clone();
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the duration of the call.
        let ev = unsafe { ev.as_ref() };
        if on_event(&app, ev) {
            std::ptr::null_mut()
        } else {
            ev as *const NSEvent as *mut NSEvent
        }
    });
    // SAFETY: the block returns null or the event it was given. The monitor lives for the whole process, so
    // the returned token is intentionally leaked.
    let token = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::ScrollWheel, &block)
    };
    std::mem::forget(token);
}

/// Returns true to swallow the event.
fn on_event(app: &AppHandle, ev: &NSEvent) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(p) = point_in_main(app, ev, mtm) else {
        return false;
    };
    on_sample(app, &sample_of(ev, p))
}

/// Remember a decision for the replay report.
fn note(a: &mut Adapter, step: &Step) {
    if let Some(c) = step.commit {
        a.commits.push((c.seq, c.dir));
    }
}

/// The engine's entry for a sample, real or replayed: same code, same decisions.
pub fn on_sample(app: &AppHandle, s: &Sample) -> bool {
    let mut guard = ADAPTER.lock();
    let Some(a) = guard.as_mut() else {
        return false;
    };
    let was_busy = a.engine.busy();
    let step = a.engine.feed(s, |x, y| {
        let z = hit(x, y)?;
        a.zone = Some(z.clone());
        Some(Hit {
            prev: z.prev,
            next: z.next,
            width: PAGE_TRAVEL,
        })
    });
    note(a, &step);
    let zone = a.zone.clone();
    if let Some(r) = a.rec.as_mut() {
        r.events.push(RecEvent {
            sample: Sample {
                t: s.t - r.events.first().map_or(s.t, |e| e.sample.t),
                ..*s
            },
            amount: step.frame.map(|f| f.amount),
            claimed: Some(step.swallow),
        });
    }
    let busy = a.engine.busy();
    drop(guard);
    publish(app, zone.as_ref(), &step);
    if busy && !was_busy {
        crate::swipe_clock::start(app);
    }
    step.swallow
}

/// Advance the engine clock; the display-rate timer calls this while a gesture is in flight.
pub fn on_tick(app: &AppHandle, now_ms: f64) -> bool {
    let mut guard = ADAPTER.lock();
    let Some(a) = guard.as_mut() else {
        return false;
    };
    let step = a.engine.tick(now_ms);
    note(a, &step);
    let zone = a.zone.clone();
    let busy = a.engine.busy();
    drop(guard);
    publish(app, zone.as_ref(), &step);
    busy
}

fn publish(app: &AppHandle, zone: Option<&Zone>, step: &Step) {
    let Some(z) = zone else { return };
    let id = z.id.as_deref();
    if step.threshold.is_some() {
        tick_haptic();
    }
    if let Some(f) = step.frame {
        let _ = app.emit(
            "swipe://frame",
            FrameMsg {
                zone: &z.zone,
                id,
                seq: f.seq,
                amount: f.amount,
                stage: f.stage,
            },
        );
    }
    if let Some(c) = step.commit {
        let _ = app.emit(
            "swipe://commit",
            CommitMsg {
                zone: &z.zone,
                id,
                seq: c.seq,
                dir: c.dir,
            },
        );
    }
}

/// Arm the recorder: from now on every scroll event the main window sees is appended (claimed or not).
pub fn record_start() -> Result<(), String> {
    let mut g = ADAPTER.lock();
    let a = g.as_mut().ok_or("the swipe monitor is not installed")?;
    if a.rec.is_some() {
        return Err("already recording".into());
    }
    a.rec = Some(Recording { events: Vec::new() });
    Ok(())
}

/// Stop recording and return the trace: `t` is ms from the first event, deltas as AppKit reported them.
pub fn record_stop() -> Result<Vec<RecEvent>, String> {
    let mut g = ADAPTER.lock();
    let a = g.as_mut().ok_or("the swipe monitor is not installed")?;
    a.rec
        .take()
        .map(|r| r.events)
        .ok_or_else(|| "not recording".into())
}

/// What a replay did, read back from the real adapter: the frames and decisions the frontend was sent.
#[derive(Serialize, Default)]
pub struct ReplayReport {
    /// The engine's `seq` before and after: gestures it claimed during the replay.
    pub first_seq: u64,
    pub last_seq: u64,
    /// What the engine decided, in order: (generation, dir) per commit: the ground truth the bench compares to.
    pub commits: Vec<(u64, i8)>,
    pub events: usize,
    pub swallowed: usize,
    pub passed: usize,
}

/// Feed `events` through [`on_sample`] (the code path of a real event after `sample_of`) at their own pace,
/// ticking with the real display clock in between. Runs on the main thread: it hops there per event, so the
/// webview and the display link keep running between events like they do for real input. Never touches the OS
/// input stream, so it cannot move the user's pointer or any other app.
pub async fn replay(app: &AppHandle, events: Vec<Sample>) -> ReplayReport {
    let mut report = ReplayReport::default();
    let Some(first) = events.first().map(|e| e.t) else {
        return report;
    };
    report.first_seq = ADAPTER.lock().as_mut().map_or(0, |a| {
        a.commits.clear();
        a.engine.seq()
    });
    let t0 = tokio::time::Instant::now();
    let base = crate::swipe_clock::now_ms();
    for s in events {
        tokio::time::sleep_until(
            t0 + std::time::Duration::from_secs_f64((s.t - first).max(0.0) / 1000.0),
        )
        .await;
        let (tx, rx) = std::sync::mpsc::channel();
        let a = app.clone();
        // Re-time the sample into the live clock so engine ticks (real uptime) and events agree.
        let s = Sample {
            t: base + (s.t - first),
            ..s
        };
        if app
            .run_on_main_thread(move || {
                let _ = tx.send(on_sample(&a, &s));
            })
            .is_err()
        {
            break;
        }
        let swallowed = tokio::task::spawn_blocking(move || rx.recv().unwrap_or(false))
            .await
            .unwrap_or(false);
        report.events += 1;
        if swallowed {
            report.swallowed += 1
        } else {
            report.passed += 1
        }
    }
    // The settle runs on the display clock after the last event: let it finish before reporting.
    while ADAPTER.lock().as_ref().is_some_and(|a| a.engine.busy()) {
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }
    if let Some(a) = ADAPTER.lock().as_ref() {
        report.last_seq = a.engine.seq();
        report.commits.clone_from(&a.commits);
    }
    report
}

/// Programmatic page change (keyboard, click, history): the engine's own spring, clock and decision, so it
/// looks and commits exactly like a swipe. Returns whether the engine took it (not while fingers are on the pad).
#[tauri::command]
pub async fn swipe_go(app: AppHandle, zone: String, id: Option<String>, dir: i8) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let a = app.clone();
    if app
        .run_on_main_thread(move || {
            let mut g = ADAPTER.lock();
            let Some(ad) = g.as_mut() else {
                let _ = tx.send(false);
                return;
            };
            let was_busy = ad.engine.busy();
            let Some(step) = ad.engine.go(dir, crate::swipe_clock::now_ms()) else {
                let _ = tx.send(false);
                return;
            };
            let z = Zone {
                zone,
                id,
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                },
                prev: true,
                next: true,
                pinned: false,
            };
            ad.zone = Some(z.clone());
            note(ad, &step);
            drop(g);
            publish(&a, Some(&z), &step);
            if !was_busy {
                crate::swipe_clock::start(&a);
            }
            let _ = tx.send(true);
        })
        .is_err()
    {
        return false;
    }
    rx.recv().unwrap_or(false)
}

/// Frontend → native: the swipe zones of the current layout (CSS px in the main webview).
#[tauri::command]
pub fn swipe_zones(zones: Vec<Zone>, scrollers: Option<Vec<Rect>>) {
    set_zones(zones);
    set_scrollers(scrollers.unwrap_or_default());
}

/// Whether native swipe tracking is active (otherwise the frontend keeps its wheel recognizer).
#[tauri::command]
pub async fn swipe_native(app: AppHandle) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    // `isSwipeTrackingFromScrollEventsEnabled` reads user defaults; ask on the main thread.
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(enabled());
        })
        .is_err()
    {
        return false;
    }
    rx.recv().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ZONES` and `SCROLLERS` are process statics: tests that set them take turns.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn zone(
        name: &str,
        id: Option<&str>,
        r: [f64; 4],
        prev: bool,
        next: bool,
        pinned: bool,
    ) -> Zone {
        Zone {
            zone: name.into(),
            id: id.map(Into::into),
            rect: Rect {
                x: r[0],
                y: r[1],
                w: r[2],
                h: r[3],
            },
            prev,
            next,
            pinned,
        }
    }

    #[test]
    fn innermost_zone_wins_and_needs_a_neighbour() {
        let _t = SERIAL.lock();
        set_scrollers(vec![]);
        // Outer first (the frontend sorts by area, largest first), nested last.
        set_zones(vec![
            zone(
                "history",
                None,
                [260.0, 0.0, 1000.0, 800.0],
                true,
                true,
                true,
            ),
            zone(
                "tab",
                Some("g1"),
                [260.0, 40.0, 500.0, 760.0],
                false,
                true,
                false,
            ),
            zone("space", None, [0.0, 0.0, 260.0, 800.0], true, true, false),
        ]);
        let h = hit(100.0, 100.0).map(|z| z.zone);
        assert_eq!(h.as_deref(), Some("space"));
        let h = hit(300.0, 100.0).map(|z| (z.zone, z.id));
        assert_eq!(h, Some(("tab".into(), Some("g1".into()))));
        // Outside the tab group, over the history zone only.
        assert_eq!(
            hit(900.0, 100.0).map(|z| z.zone).as_deref(),
            Some("history")
        );
        assert!(hit(5000.0, 5.0).is_none());
        // No neighbour either way: nothing to track.
        set_zones(vec![zone(
            "space",
            None,
            [0.0, 0.0, 260.0, 800.0],
            false,
            false,
            false,
        )]);
        assert!(hit(10.0, 10.0).is_none());
        // History only when pinned.
        set_zones(vec![zone(
            "history",
            None,
            [0.0, 0.0, 260.0, 800.0],
            true,
            true,
            false,
        )]);
        assert!(hit(10.0, 10.0).is_none());
    }

    /// The Back/Forward swipe over the content must never steal a horizontal scroll: where a terminal, an editor or an
    /// overflowing list sits under the pointer the `history` zone does not claim, whatever its `pinned` flag says.
    #[test]
    fn history_never_claims_where_something_scrolls_horizontally() {
        let _t = SERIAL.lock();
        set_zones(vec![zone(
            "history",
            None,
            [0.0, 0.0, 1000.0, 800.0],
            true,
            true,
            true,
        )]);
        set_scrollers(vec![Rect {
            x: 100.0,
            y: 100.0,
            w: 300.0,
            h: 200.0,
        }]);
        assert!(hit(150.0, 150.0).is_none(), "over a horizontal scroller");
        assert_eq!(
            hit(600.0, 600.0).map(|z| z.zone).as_deref(),
            Some("history")
        );
        // Only history yields: a tab group or the sidebar keeps its swipe over a scroller (its strip scrolls).
        set_zones(vec![zone(
            "tab",
            Some("g"),
            [0.0, 0.0, 1000.0, 800.0],
            true,
            true,
            false,
        )]);
        assert!(hit(150.0, 150.0).is_some());
        set_scrollers(vec![]);
    }

    #[test]
    fn phases_map_from_appkit_flags() {
        assert_eq!(phase_of(NSEventPhase::Began), Phase::Began);
        assert_eq!(phase_of(NSEventPhase::Ended), Phase::Ended);
        assert_eq!(phase_of(NSEventPhase::Cancelled), Phase::Cancelled);
        assert_eq!(phase_of(NSEventPhase::Changed), Phase::Changed);
        assert_eq!(phase_of(NSEventPhase::MayBegin), Phase::MayBegin);
        assert_eq!(phase_of(NSEventPhase::empty()), Phase::None);
        assert_eq!(momentum_of(NSEventPhase::empty()), Momentum::None);
        assert_eq!(momentum_of(NSEventPhase::Began), Momentum::Began);
        assert_eq!(momentum_of(NSEventPhase::Changed), Momentum::Changed);
        assert_eq!(momentum_of(NSEventPhase::Ended), Momentum::Ended);
    }
}
