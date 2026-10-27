//! Executes the [`Effect`]s of the capture session on macOS. Runs on a worker thread (ScreenCaptureKit blocks), and
//! hops to the main thread only for window ordering.
//!
//! The overlay is shown first and frozen second: the windows (built once, hidden between captures) are ordered in
//! from CoreGraphics' display list, and the freeze runs while they are already on screen. A region or a screen pick
//! is then cropped from that frozen frame in memory: the pixels saved are the ones the person saw when they let go,
//! never a new capture taken after the release.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use objc2::msg_send;
use objc2::runtime::AnyObject;
use tauri::{AppHandle, Emitter, Manager};

use super::freeze::{self, Frames, Frozen};
use super::geom::{self, Rect};
use super::history::Entry;
use super::naming::{self, Kind};
use super::session::{Effect, Event, Pick, Still};
use super::surface::{self, Surface};
use super::{
    Cursor, DisplayInfo, OverlayInit, OverlayWindow, QuickState, Shots, mac, pixels, sck, windows,
};

/// Run one effect. Errors never panic the worker: a failed capture tells the person and frees the session.
pub fn run(app: &AppHandle, fx: Effect) {
    let result = match fx {
        Effect::ShowOverlay(p) => show_overlay(app, p),
        Effect::SetOverlayMode(p) => set_overlay_mode(app, p),
        Effect::HideOverlay => {
            hide_overlay(app);
            Ok(())
        }
        Effect::Still(s) => still(app, s, Then::Save),
        Effect::Text(s) => still(app, s, Then::Text),
        other => super::record_mac::run(app, other),
    };
    if let Err(e) = result {
        eprintln!("shots: {e}");
        notify_error(app, &e);
        super::feed(app, Event::Failed);
    }
}

/// What a taken still becomes: a screenshot in the folder (the usual), or the text read from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Save,
    Text,
}

/// An error the person should read: shown on the quick card (which hosts messages too) so it appears where the
/// capture would have.
fn notify_error(app: &AppHandle, message: &str) {
    let _ = app.emit("shots://error", message);
    // A failed capture with the permission missing is the common case: a system notification says how to fix it.
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("Capture failed")
        .body(message)
        .show();
}

// ───────────────────────────── overlay ─────────────────────────────

/// The cursor in global points, top-left origin.
pub(super) fn cursor() -> Cursor {
    let (x, y) = mac::cursor_point();
    Cursor { x, y }
}

const NO_PERMISSION: &str = "Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas.";

/// The displays now, from CoreGraphics (microseconds; ScreenCaptureKit's listing takes a hundred milliseconds or
/// seconds on a loaded Mac and is not needed to put a window on screen): id, frame in global points (top-left
/// origin), backing scale.
fn displays_now() -> Vec<DisplayInfo> {
    crate::display_mac::active_displays()
        .into_iter()
        .map(|d| {
            let frame = Rect::new(d.bounds.x, d.bounds.y, d.bounds.w, d.bounds.h);
            let scale = crate::display_mac::display_mode(d.id)
                .filter(|m| frame.w > 0.0 && m.pixels.0 > 0.0)
                .map_or(1.0, |m| (m.pixels.0 / frame.w).max(1.0));
            DisplayInfo {
                id: d.id,
                frame,
                scale,
            }
        })
        .collect()
}

/// Open a capture session: the overlay on every display at once, then the freeze.
///
/// Order matters. The windows are ordered in first, from CoreGraphics' display list and without waiting for the
/// page, ScreenCaptureKit or any other window: that is hotkey to overlay. The freeze (listing, then one screenshot
/// per display, in parallel) runs while the overlay is already up and excludes it by window id, so the frame is the
/// screen without the overlay on it. A selection released before the freeze is ready waits for it on the worker
/// (`freeze::Slot::wait`); it never takes a second capture.
fn show_overlay(app: &AppHandle, pick: Pick) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    if !sck::permission_granted() {
        // Ask once (the system prompt), and say what to do: macOS only applies the grant after a restart.
        sck::request_permission();
        return Err(NO_PERMISSION.into());
    }
    let shots = app.state::<Shots>();
    let session = shots
        .session
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;
    *shots.content.lock() = None;
    let prefs = shots.prefs.lock().clone();
    let has_previous = shots.last_area.lock().is_some();
    let cur = cursor();
    let displays = displays_now();
    let mut inits = std::collections::HashMap::new();
    for d in &displays {
        inits.insert(
            d.id,
            OverlayInit {
                session,
                display: d.clone(),
                mode: pick,
                windows: Vec::new(),
                cursor: cur,
                hud: super::Hud {
                    last_mode: prefs.still.last_mode,
                    timer_secs: prefs.still.timer_secs,
                    has_previous,
                    capture_on_release: prefs.still.capture_on_release,
                },
            },
        );
    }
    *shots.overlay.lock() = inits.clone();

    // The overlay (agent instances never build it: it covers real displays, so they only freeze).
    if crate::agent_mode::Mode::current().focus_allowed() {
        let ids: Vec<u32> = displays.iter().map(|d| d.id).collect();
        let existing: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| {
                app.get_webview_window(&Surface::Overlay.label(*id))
                    .is_some()
            })
            .collect();
        let plan = freeze::overlay_plan(&existing, &ids);
        let tb = std::time::Instant::now();
        for id in &plan.build {
            surface::build(app, Surface::Overlay, *id, "")?;
        }
        if !plan.build.is_empty() {
            super::perf::mark("overlay.build", tb);
        }
        let mut order = Vec::new();
        let mut key_label = None;
        for d in &displays {
            let label = Surface::Overlay.label(d.id);
            let Some(win) = app.get_window(&label) else {
                continue;
            };
            if d.frame.contains(cur.x, cur.y) || key_label.is_none() {
                key_label = Some(label);
            }
            order.push((win, d.frame));
        }
        let tp = std::time::Instant::now();
        order_in(app, order, key_label);
        super::perf::mark("overlay.place-show", tp);
        super::perf::mark("overlay.visible", t0);
        // The pages are live now, while the screen is still being frozen; they get the windows they can snap to after.
        emit_inits(app, &inits);
    }

    // The freeze, with the overlay already up. Failing here must close the overlay again: a dimmed screen that
    // cannot capture anything is a trap.
    if let Err(e) = freeze_session(app, &shots, &displays, &mut inits, t0) {
        shots.overlay.lock().clear();
        hide_overlay(app);
        return Err(e);
    }
    emit_inits(app, &inits);
    super::perf::mark("overlay.ready", t0);
    Ok(())
}

/// Tell each display's overlay page its session (`shots://overlay-init`).
fn emit_inits(app: &AppHandle, inits: &std::collections::HashMap<u32, OverlayInit>) {
    for (id, init) in inits {
        let _ = app.emit_to(Surface::Overlay.label(*id), "shots://overlay-init", init);
    }
}

/// Freeze every display (in parallel) with the overlay excluded by window id, keep the listing for a window pick,
/// give each display's overlay the windows it may pick, and publish the frames.
fn freeze_session(
    app: &AppHandle,
    shots: &Shots,
    displays: &[DisplayInfo],
    inits: &mut std::collections::HashMap<u32, OverlayInit>,
    t0: std::time::Instant,
) -> Result<(), String> {
    let content = Arc::new(sck::Content::fetch()?);
    super::perf::mark("overlay.content", t0);
    let me = std::process::id() as i32;
    let own = own_window_ids(app, &content);
    let frames: Vec<Result<(u32, Frozen), String>> = std::thread::scope(|s| {
        let handles: Vec<_> = displays
            .iter()
            .map(|d| {
                let (content, own) = (&content, &own);
                s.spawn(move || {
                    mac::set_qos(mac::Qos::Interactive);
                    let tf = std::time::Instant::now();
                    let img = sck::still_rgba(
                        &sck::Target::Display {
                            content,
                            id: d.id,
                            region: None,
                        },
                        own,
                        sck::StillOpts::default(),
                    )?;
                    super::perf::mark("overlay.freeze", tf);
                    Ok((
                        d.id,
                        Frozen {
                            rgba: img.data,
                            size: img.size,
                        },
                    ))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("the freeze thread panicked".into()))
            })
            .collect()
    });
    let mut map = Frames::new();
    for f in frames {
        let (id, frozen) = f?;
        map.insert(id, Arc::new(frozen));
    }
    for d in displays {
        let wins = windows::pickable(&content.windows, me, &own, d.frame);
        if let Some(init) = inits.get_mut(&d.id) {
            init.windows = wins
                .iter()
                .map(|w| OverlayWindow {
                    id: w.id,
                    frame: w.frame,
                    title: w.title.clone(),
                })
                .collect();
        }
    }
    *shots.overlay.lock() = inits.clone();
    *shots.content.lock() = Some(content);
    shots.frozen.publish(freeze::Session {
        frames: map,
        displays: displays
            .iter()
            .map(|d| freeze::Disp {
                id: d.id,
                frame: d.frame,
                scale: d.scale,
            })
            .collect(),
    });
    super::perf::mark("overlay.frozen", t0);
    Ok(())
}

/// Place every overlay window on its display and order them all in, in ONE main-thread hop, then make the window
/// under the pointer the key one (Esc, Space) without activating the app. The wait for the main thread is bounded:
/// the freeze must not queue behind a busy event loop for long.
fn order_in(app: &AppHandle, order: Vec<(tauri::Window, Rect)>, key: Option<String>) {
    let h = crate::display_mac::primary_height();
    let a = app.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let queued = app.run_on_main_thread(move || {
        for (win, frame) in &order {
            if let Ok(ns) = win.ns_window() {
                let rect = objc2_foundation::NSRect::new(
                    objc2_foundation::NSPoint::new(frame.x, h - (frame.y + frame.h)),
                    objc2_foundation::NSSize::new(frame.w, frame.h),
                );
                // SAFETY: Tauri's live NSWindow on the main thread; `setFrame:display:` takes a rect.
                unsafe {
                    let w = ns as *mut AnyObject;
                    let _: () = msg_send![w, setFrame: rect, display: false];
                    // above the menu bar and the Dock: the whole display is the canvas
                    let _: () = msg_send![w, setLevel: 2002_isize];
                }
            }
        }
        // Ordered in first (`makeKeyAndOrderFront` of a non-activating panel: on screen, the app stays in the
        // background), then the window under the pointer becomes the key one so Esc and Space reach its page.
        for (win, _) in &order {
            if win
                .ns_window()
                .is_ok_and(crate::dock_mac::ensure_nonactivating)
            {
                let _ = win.show();
            }
        }
        let win = key.and_then(|l| a.get_window(&l));
        crate::dock_mac::overlay_keys(win.as_ref());
        let _ = tx.send(());
    });
    if queued.is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
    }
}

fn set_overlay_mode(app: &AppHandle, pick: Pick) -> Result<(), String> {
    let shots = app.state::<Shots>();
    for init in shots.overlay.lock().values_mut() {
        init.mode = pick;
    }
    let mode = match pick {
        Pick::Window => "window",
        _ => "region",
    };
    let _ = app.emit("shots://overlay-mode", mode);
    Ok(())
}

/// Take the overlay off the screen: ordered out and reset, never destroyed (the windows are reused by the next
/// capture and their pages stay loaded). One main-thread hop. The frozen frames stay only while a capture still has
/// to take its pixels from them (state `Capturing`); otherwise they go with the overlay.
pub fn hide_overlay(app: &AppHandle) {
    let shots = app.state::<Shots>();
    let labels: Vec<String> = shots
        .overlay
        .lock()
        .drain()
        .map(|(id, _)| Surface::Overlay.label(id))
        .collect();
    if !matches!(*shots.state.lock(), super::session::State::Capturing) {
        shots.frozen.clear();
        *shots.content.lock() = None;
    }
    let a = app.clone();
    let released = *shots.released.lock();
    let _ = app.run_on_main_thread(move || {
        crate::dock_mac::overlay_keys(None);
        for l in &labels {
            if let Some(w) = a.get_webview_window(l) {
                let _ = w.hide();
            }
            let _ = a.emit_to(l.as_str(), "shots://overlay-reset", ());
        }
        super::perf::since_release("release.hidden", released);
    });
}

/// Every window of this process a capture must not contain, as ScreenCaptureKit ids: the capture surfaces, the dock
/// pill and any untitled helper window (`windows::exclusions`).
pub fn own_window_ids(app: &AppHandle, content: &sck::Content) -> Vec<u32> {
    let me = std::process::id() as i32;
    let known: Vec<u32> = app
        .webview_windows()
        .values()
        .filter(|w| surface::is_excluded_label(w.label()))
        .filter_map(|w| w.ns_window().ok())
        .filter_map(|ns| window_number(ns as *mut AnyObject))
        .collect();
    windows::exclusions(&content.windows, me, &known, None, false)
}

fn window_number(ns: *mut AnyObject) -> Option<u32> {
    if ns.is_null() {
        return None;
    }
    // SAFETY: plain getter on a live NSWindow; `windowNumber` is the CGWindowID ScreenCaptureKit uses.
    let n: isize = unsafe { msg_send![ns, windowNumber] };
    u32::try_from(n).ok().filter(|n| *n > 0)
}

// ───────────────────────────── stills ─────────────────────────────

/// Pixels of a still plus where they came from.
struct Taken {
    img: pixels::Rgba,
    /// The window's frame in points, when the still is a window (the shadow and padding are sized from it).
    window: Option<(Rect, f64)>,
}

fn still(app: &AppHandle, s: Still, then: Then) -> Result<(), String> {
    if !sck::permission_granted() {
        sck::request_permission();
        return Err(NO_PERMISSION.into());
    }
    let t0 = std::time::Instant::now();
    let shots = app.state::<Shots>();
    let prefs = shots.prefs.lock().clone();
    let one_x = prefs.still.scale == super::prefs::Scale::One;
    let timed = then == Then::Save && prefs.still.timer_secs > 0;
    let look = geom::window_look(prefs.still.window_shadow, prefs.still.window_padding);
    // A pick made in the overlay (a region, a whole screen, a window): it has a frozen frame behind it.
    let from_overlay = matches!(
        s,
        Still::Region { .. } | Still::Display { .. } | Still::Window { .. }
    );

    // A region or a screen picked in the overlay is cut from the frame frozen when the overlay opened: what the
    // person saw when they released, in memory, with no listing, no ScreenCaptureKit and no wait for the overlay
    // to leave. A self-timer is the one exception: it exists to capture a LATER state, so it captures after the wait.
    // Only a pick from the overlay waits for a freeze that is still being taken (a very fast release); the
    // fullscreen key has no overlay and nothing to wait for.
    let from_freeze = if timed || !from_overlay {
        None
    } else {
        match shots.frozen.wait(std::time::Duration::from_secs(5)) {
            Some(session) => freeze::still_of(&session, &s, one_x)?,
            None => None,
        }
    };
    let overlay_content = shots.content.lock().clone();
    // The frozen frames have served this capture (or a self-timer wants a later state): the next session freezes its
    // own, and nothing keeps 24 MB per display alive.
    if from_overlay || timed {
        shots.frozen.clear();
    }
    let taken = match from_freeze {
        Some(img) => Taken { img, window: None },
        None => {
            if timed {
                super::timer::wait(app, prefs.still.timer_secs);
            }
            live_still(app, &prefs, &s, one_x, look, overlay_content)?
        }
    };
    super::perf::mark("still.pixels", t0);
    // The pixels are in hand: the next capture may start while this one is encoded and written.
    super::feed(app, Event::CaptureDone);

    let img = match (&s, taken.window) {
        (Still::Window { .. }, Some((frame, scale)))
            if look != geom::WindowLook::Plain && then == Then::Save =>
        {
            let canvas = geom::window_canvas((frame.w, frame.h), scale, look);
            pixels::frame_window(&taken.img, look, canvas.inset, scale)?
        }
        _ => taken.img,
    };
    super::perf::mark("still", t0);
    match then {
        Then::Save => {
            let dir = prefs.dir();
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            finish_image(app, &prefs, save_path(&dir, Kind::Screenshot), &img)?;
        }
        Then::Text => {
            // the text read's screenshot is a scratch file: it never reaches the folder or the history
            let out =
                std::env::temp_dir().join(format!("atlas-shots-text-{}.png", std::process::id()));
            std::fs::write(&out, pixels::encode_png(&img.data, img.size.0, img.size.1)?)
                .map_err(|e| format!("{}: {e}", out.display()))?;
            let r = super::ocr::from_file(app, &out);
            let _ = std::fs::remove_file(&out);
            r?;
        }
    }
    super::perf::mark("capture-to-card", t0);
    Ok(())
}

/// A still that is taken live (ScreenCaptureKit) because the freeze cannot answer it: a window (its own pixels and
/// alpha), the fullscreen key (no overlay, so the instant of the key is the instant to capture), a self-timer
/// (a later state on purpose), or a session with no freeze (an agent instance). The listing the overlay opened with is
/// reused for a window pick; only a capture with no overlay behind it lists the windows itself.
fn live_still(
    app: &AppHandle,
    prefs: &super::CapturePrefs,
    s: &Still,
    one_x: bool,
    look: geom::WindowLook,
    overlay_content: Option<Arc<sck::Content>>,
) -> Result<Taken, String> {
    let content = match (overlay_content, prefs.still.hide_icons) {
        // the overlay's listing has no desktop-icon windows, so "hide icons" lists again
        (Some(c), false) => c,
        _ => Arc::new(sck::Content::fetch_with(prefs.still.hide_icons)?),
    };
    super::perf::mark("still.content", std::time::Instant::now());
    let mut own = own_window_ids(app, &content);
    if prefs.still.hide_icons {
        // Finder's desktop-icon windows are left out of the picture: no setting changes, no Finder restart.
        own.extend(windows::desktop_icon_ids(&content.windows));
    }
    // The shadow and the padding are drawn in pixels afterwards (deterministic, tested), so SCK leaves its own out.
    let opts = sck::StillOpts {
        one_x,
        ignore_shadow: look != geom::WindowLook::Plain,
    };
    let display_of = |id: u32| content.display(id).ok_or("that display is gone");
    let target_display = |s: &Still| -> Result<u32, String> {
        match s {
            Still::Display { display } | Still::Region { display, .. } => {
                Ok(display_of(*display)?.id)
            }
            _ => {
                let cur = cursor();
                content
                    .displays
                    .iter()
                    .find(|d| d.frame.contains(cur.x, cur.y))
                    .or(content.displays.first())
                    .map(|d| d.id)
                    .ok_or_else(|| "no display to capture".to_string())
            }
        }
    };
    let tc = std::time::Instant::now();
    let (img, window) = match s {
        Still::Screen | Still::Display { .. } => {
            let id = target_display(s)?;
            let img = sck::still_rgba(
                &sck::Target::Display {
                    content: &content,
                    id,
                    region: None,
                },
                &own,
                opts,
            )?;
            (img, None)
        }
        Still::Region { rect, display } => {
            let d = display_of(*display)?;
            // Clip to the display and take display-relative points (ScreenCaptureKit's source rect), so a region
            // that runs off the edge, or sits on a second display, is cropped instead of failing the capture.
            let (px, py, pw, ph) = geom::to_pixels(*rect, d.frame, d.scale)
                .ok_or("That region is outside the display.")?;
            let local = Rect::new(
                f64::from(px) / d.scale,
                f64::from(py) / d.scale,
                f64::from(pw) / d.scale,
                f64::from(ph) / d.scale,
            );
            let img = sck::still_rgba(
                &sck::Target::Display {
                    content: &content,
                    id: *display,
                    region: Some(local),
                },
                &own,
                opts,
            )?;
            (img, None)
        }
        Still::Window { id } => {
            let frame = content
                .windows
                .iter()
                .find(|x| x.id == *id)
                .map(|x| x.frame)
                .ok_or("that window is gone")?;
            let display_scale = content
                .displays
                .iter()
                .find(|d| d.frame.intersect(&frame).is_some())
                .map_or(2.0, |d| d.scale);
            let scale = geom::effective_scale(display_scale, one_x);
            let img = sck::still_rgba_alpha(
                &sck::Target::Window {
                    content: &content,
                    id: *id,
                },
                &own,
                opts,
            )?;
            (img, Some((frame, scale)))
        }
    };
    super::perf::mark("still.sck", tc);
    Ok(Taken { img, window })
}

/// Encode `img` once and hand the same bytes to the file, the clipboard and (through the pixels) the card. The file
/// and the clipboard are written on two threads at once: neither waits for the other, and the card does not wait for
/// the PNG to be read back and decoded as it used to.
pub fn finish_image(
    app: &AppHandle,
    prefs: &super::CapturePrefs,
    path: PathBuf,
    img: &pixels::Rgba,
) -> Result<Entry, String> {
    let clipboard = prefs.after.copy;
    let quick = prefs.after.quick_access;
    // Three jobs on the same pixels, none waiting for another more than it must: the thumbnail for the card, the
    // PNG encode, and then (needing only the encoded bytes) the file and the clipboard side by side.
    let (thumb, written, copied) = std::thread::scope(|s| {
        let thumb = s.spawn(|| {
            mac::set_qos(mac::Qos::Interactive);
            let tt = std::time::Instant::now();
            let t = if quick { thumb_of_pixels(img) } else { None };
            super::perf::mark("finish.thumb", tt);
            t
        });
        let te = std::time::Instant::now();
        let png = pixels::encode_png_fast(&img.data, img.size.0, img.size.1);
        super::perf::mark("finish.encode", te);
        let (written, copied) = match &png {
            Ok(png) => std::thread::scope(|s2| {
                let copy = s2.spawn(|| {
                    mac::set_qos(mac::Qos::Interactive);
                    clipboard && mac::copy_png_bytes(png, &path).is_ok()
                });
                let write =
                    std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()));
                (write, copy.join().unwrap_or(false))
            }),
            Err(e) => (Err(e.clone()), false),
        };
        (thumb.join().unwrap_or(None), written, copied)
    });
    written?;
    finish_with(
        app,
        prefs,
        Landed {
            path,
            kind: Kind::Screenshot,
            size: img.size,
            secs: None,
            copied,
        },
        Thumb::Ready(thumb),
    )
}

/// `dir/<name>` for a new capture of `kind`, unused.
pub fn save_path(dir: &Path, kind: Kind) -> PathBuf {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let name = naming::file_name(
        kind,
        (
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
        ),
    );
    naming::unique(dir, &name, |p| p.exists())
}

/// A finished file: clipboard, sound, history, quick card. Recordings and GIFs come here (a file on disk, no pixels
/// in memory); screenshots go through [`finish_image`].
pub fn finish(
    app: &AppHandle,
    prefs: &super::CapturePrefs,
    path: PathBuf,
    kind: Kind,
    size: (u32, u32),
    secs: Option<f64>,
) -> Result<Entry, String> {
    let tf = std::time::Instant::now();
    let copied = prefs.after.copy
        && match kind {
            Kind::Screenshot => mac::copy_image(&path).is_ok(),
            _ => mac::copy_file(&path).is_ok(),
        };
    super::perf::mark("finish.clipboard", tf);
    finish_with(
        app,
        prefs,
        Landed {
            path,
            kind,
            size,
            secs,
            copied,
        },
        Thumb::FromFile,
    )
}

/// Where the card's thumbnail comes from.
enum Thumb {
    /// Already made (a screenshot's thumbnail is built from its pixels while the file is encoded and written).
    Ready(Option<String>),
    /// Read from the file just written (a recording's poster, a GIF's first frame).
    FromFile,
}

/// A capture's file once it is on disk, and what was done with the clipboard.
struct Landed {
    path: PathBuf,
    kind: Kind,
    size: (u32, u32),
    secs: Option<f64>,
    copied: bool,
}

/// The shared tail of a finished capture: the entry, then the card (before anything that touches the disk again),
/// then the shutter, then the history.
fn finish_with(
    app: &AppHandle,
    prefs: &super::CapturePrefs,
    landed: Landed,
    thumb: Thumb,
) -> Result<Entry, String> {
    let Landed {
        path,
        kind,
        size,
        secs,
        copied,
    } = landed;
    let shots = app.state::<Shots>();
    naming::make_shareable(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    let entry = Entry {
        path: path.clone(),
        kind,
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        width: size.0,
        height: size.1,
        bytes,
        secs,
    };
    super::perf::since_release("release.file", *shots.released.lock());
    let card = if prefs.after.quick_access {
        let thumb = match thumb {
            Thumb::Ready(t) => t,
            Thumb::FromFile => {
                let tt = std::time::Instant::now();
                let t = thumb_of(&path, kind);
                super::perf::mark("finish.thumb", tt);
                t
            }
        }
        .unwrap_or_default();
        let q = QuickState {
            entry: entry.clone(),
            thumb,
            dismiss_secs: prefs.after.dismiss_secs,
            copied,
        };
        let tq = std::time::Instant::now();
        let shown = super::quick::show(app, &q);
        super::perf::mark("finish.card", tq);
        super::perf::since_release("release.card", *shots.released.lock());
        shown
    } else {
        Ok(())
    };
    // after the card is up: the sound and the history file (a disk write) never sit in front of it, and the
    // interactive scheduler class (the worker's, for the instant path) is given back before them
    mac::set_qos(mac::Qos::Default);
    if prefs.after.sound && kind == Kind::Screenshot {
        mac::shutter();
    }
    shots
        .history
        .lock()
        .push(entry.clone(), prefs.history as usize);
    shots.save_history();
    card?;
    Ok(entry)
}

/// A 440 px-wide thumbnail data URL of pixels already in memory.
fn thumb_of_pixels(img: &pixels::Rgba) -> Option<String> {
    let (data, (w, h)) = pixels::thumbnail(img, 440);
    pixels::data_url(&data, w, h).ok()
}

/// A 440 px-wide thumbnail data URL for the card (PNG only; videos get a poster in `record`).
fn thumb_of(path: &Path, kind: Kind) -> Option<String> {
    if kind != Kind::Screenshot {
        return super::record_mac::poster(path);
    }
    let img = pixels::decode_png(&std::fs::read(path).ok()?).ok()?;
    thumb_of_pixels(&img)
}
