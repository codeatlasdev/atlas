//! Executes the [`Effect`]s of the capture session on macOS. Runs on a worker thread (ScreenCaptureKit blocks), and
//! hops to the main thread only for window work.

use std::path::{Path, PathBuf};

use objc2::msg_send;
use objc2::runtime::AnyObject;
use tauri::{AppHandle, Emitter, Manager};

use super::geom::{self, Rect};
use super::history::Entry;
use super::naming::{self, Kind};
use super::session::{Effect, Event, Pick, Still};
use super::surface::{self, Surface};
use super::{
    Cursor, DisplayInfo, Frozen, OverlayInit, OverlayWindow, QuickState, Shots, mac, pixels, sck,
    windows,
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

fn show_overlay(app: &AppHandle, pick: Pick) -> Result<(), String> {
    if !sck::permission_granted() {
        // Ask once (the system prompt), and say what to do: macOS only applies the grant after a restart.
        sck::request_permission();
        return Err("Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas.".into());
    }
    let t0 = std::time::Instant::now();
    let content = sck::Content::fetch()?;
    super::perf::mark("overlay.content", t0);
    let me = std::process::id() as i32;
    let own = own_window_ids(app, &content);
    let shots = app.state::<Shots>();
    let prefs = shots.prefs.lock().clone();
    let has_previous = shots.last_area.lock().is_some();
    let cur = cursor();
    let mut inits = std::collections::HashMap::new();
    let mut frozen = std::collections::HashMap::new();
    for d in &content.displays {
        // Freeze this display BEFORE any overlay window exists: the loupe then shows the real screen.
        let tf = std::time::Instant::now();
        let img = sck::still_rgba(
            &sck::Target::Display {
                content: &content,
                id: d.id,
                region: None,
            },
            &own,
            sck::StillOpts::default(),
        )?;
        super::perf::mark("overlay.freeze", tf);
        frozen.insert(
            d.id,
            Frozen {
                rgba: img.data,
                size: img.size,
            },
        );
        let wins = windows::pickable(&content.windows, me, &own, d.frame);
        inits.insert(
            d.id,
            OverlayInit {
                display: DisplayInfo {
                    id: d.id,
                    frame: d.frame,
                    scale: d.scale,
                },
                mode: pick,
                windows: wins
                    .iter()
                    .map(|w| OverlayWindow {
                        id: w.id,
                        frame: w.frame,
                        title: w.title.clone(),
                    })
                    .collect(),
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
    *shots.frozen.lock() = frozen;
    *shots.overlay.lock() = inits.clone();
    let mut key_win = None;
    for (id, init) in &inits {
        let tw = std::time::Instant::now();
        let win = surface::build(app, Surface::Overlay, *id, "")?;
        super::perf::mark("overlay.build", tw);
        let tp = std::time::Instant::now();
        place_overlay(app, &win, init.display.frame)?;
        surface::show(app, &win);
        super::perf::mark("overlay.place-show", tp);
        if init.display.frame.contains(cur.x, cur.y) || key_win.is_none() {
            key_win = Some(win);
        }
    }
    // The overlay takes keys (Esc, Space) and the pointer; it never activates the app. The window under the
    // pointer is the key one, so Esc reaches a page that is looking at the user.
    set_overlay_keys(app, key_win.as_ref().map(|w| w.label().to_string()));
    super::perf::mark("overlay.visible", t0);
    Ok(())
}

/// Cover `frame` (global points, top-left) exactly with the overlay window, above everything, key-capable (Esc).
fn place_overlay(app: &AppHandle, win: &tauri::WebviewWindow, frame: Rect) -> Result<(), String> {
    let ns = win.ns_window().map_err(|e| e.to_string())? as usize;
    let (tx, rx) = std::sync::mpsc::channel();
    let h = crate::display_mac::primary_height();
    app.run_on_main_thread(move || {
        let w = ns as *mut AnyObject;
        let rect = objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(frame.x, h - (frame.y + frame.h)),
            objc2_foundation::NSSize::new(frame.w, frame.h),
        );
        // SAFETY: Tauri's live NSWindow on the main thread; `setFrame:display:` takes a rect.
        unsafe {
            let _: () = msg_send![w, setFrame: rect, display: false];
            // above the menu bar and the Dock: the whole display is the canvas
            let _: () = msg_send![w, setLevel: 2002_isize];
        }
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
    Ok(())
}

/// Let the overlay window `label` take keys (and make it the key window), or revoke key status (`None`). On the main
/// thread: AppKit window calls.
fn set_overlay_keys(app: &AppHandle, label: Option<String>) {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let win = label.and_then(|l| a.get_window(&l));
        crate::dock_mac::overlay_keys(win.as_ref());
    });
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

fn hide_overlay(app: &AppHandle) {
    let shots = app.state::<Shots>();
    let ids: Vec<u32> = shots.overlay.lock().keys().copied().collect();
    shots.overlay.lock().clear();
    shots.frozen.lock().clear();
    set_overlay_keys(app, None);
    for id in ids {
        if let Some(w) = app.get_webview_window(&Surface::Overlay.label(id)) {
            let _ = w.destroy();
        }
    }
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

fn still(app: &AppHandle, s: Still, then: Then) -> Result<(), String> {
    if !sck::permission_granted() {
        sck::request_permission();
        return Err("Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas.".into());
    }
    let t0 = std::time::Instant::now();
    let shots = app.state::<Shots>();
    let prefs = shots.prefs.lock().clone();
    // Self-timer: stills wait (the overlay is already gone); a text read has no timer, it is instant by nature.
    if then == Then::Save && prefs.still.timer_secs > 0 {
        super::timer::wait(app, prefs.still.timer_secs);
    }
    let content = sck::Content::fetch_with(prefs.still.hide_icons)?;
    super::perf::mark("still.content", t0);
    let mut own = own_window_ids(app, &content);
    if prefs.still.hide_icons {
        // Finder's desktop-icon windows are left out of the picture: no setting changes, no Finder restart.
        own.extend(windows::desktop_icon_ids(&content.windows));
    }
    let dir = match then {
        Then::Save => prefs.dir(),
        // the text read's screenshot is a scratch file: it never reaches the folder or the history
        Then::Text => std::env::temp_dir(),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = match then {
        Then::Save => save_path(&dir, Kind::Screenshot),
        Then::Text => dir.join(format!("atlas-shots-text-{}.png", std::process::id())),
    };
    let display_of = |id: u32| content.display(id).ok_or("that display is gone");
    let look = geom::window_look(prefs.still.window_shadow, prefs.still.window_padding);
    // The shadow and the padding are drawn in pixels afterwards (deterministic, tested), so SCK leaves its own out.
    let opts = sck::StillOpts {
        one_x: prefs.still.scale == super::prefs::Scale::One,
        ignore_shadow: look != geom::WindowLook::Plain,
    };
    let tc = std::time::Instant::now();
    let (mut w, mut h) = match s {
        Still::Screen | Still::Display { .. } => {
            let d = if let Still::Display { display } = s {
                display_of(display)?
            } else {
                let cur = cursor();
                content
                    .displays
                    .iter()
                    .find(|d| d.frame.contains(cur.x, cur.y))
                    .or(content.displays.first())
                    .ok_or("no display to capture")?
            };
            sck::still_with(
                &sck::Target::Display {
                    content: &content,
                    id: d.id,
                    region: None,
                },
                &own,
                &out,
                opts,
            )?
        }
        Still::Region { rect, display } => {
            let d = display_of(display)?;
            // Clip to the display and take display-relative points (ScreenCaptureKit's source rect), so a region
            // that runs off the edge, or sits on a second display, is cropped instead of failing the capture.
            let (px, py, pw, ph) = geom::to_pixels(rect, d.frame, d.scale)
                .ok_or("That region is outside the display.")?;
            let local = Rect::new(
                f64::from(px) / d.scale,
                f64::from(py) / d.scale,
                f64::from(pw) / d.scale,
                f64::from(ph) / d.scale,
            );
            sck::still_with(
                &sck::Target::Display {
                    content: &content,
                    id: display,
                    region: Some(local),
                },
                &own,
                &out,
                opts,
            )?
        }
        Still::Window { id } => sck::still_with(
            &sck::Target::Window {
                content: &content,
                id,
            },
            &own,
            &out,
            opts,
        )?,
    };
    if let Still::Window { id } = s
        && look != geom::WindowLook::Plain
        && then == Then::Save
    {
        (w, h) = frame_window_file(&out, look, &content, opts, id)?;
    }
    super::perf::mark("still.sck", tc);
    super::perf::mark("still", t0);
    match then {
        Then::Save => {
            finish(app, &prefs, out, Kind::Screenshot, (w, h), None)?;
        }
        Then::Text => {
            let r = super::ocr::from_file(app, &out);
            let _ = std::fs::remove_file(&out);
            r?;
        }
    }
    super::perf::mark("capture-to-card", t0);
    super::feed(app, Event::CaptureDone);
    Ok(())
}

/// Put the window capture at `path` (no shadow) on its shadow margin or padded background and rewrite the file;
/// returns the new pixel size. The margin is in points (`geom::window_canvas`), so it looks the same on a 2x and a
/// 1x capture.
fn frame_window_file(
    path: &Path,
    look: geom::WindowLook,
    content: &sck::Content,
    opts: sck::StillOpts,
    id: u32,
) -> Result<(u32, u32), String> {
    let frame = content
        .windows
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.frame)
        .ok_or("that window is gone")?;
    let display_scale = content
        .displays
        .iter()
        .find(|d| d.frame.intersect(&frame).is_some())
        .map_or(2.0, |d| d.scale);
    let scale = geom::effective_scale(display_scale, opts.one_x);
    let canvas = geom::window_canvas((frame.w, frame.h), scale, look);
    let img = pixels::decode_png(&std::fs::read(path).map_err(|e| e.to_string())?)?;
    let framed = pixels::frame_window(&img, look, canvas.inset, scale)?;
    let png = pixels::encode_png(&framed.data, framed.size.0, framed.size.1)?;
    std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(framed.size)
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

/// A finished file: clipboard, sound, history, quick card.
pub fn finish(
    app: &AppHandle,
    prefs: &super::CapturePrefs,
    path: PathBuf,
    kind: Kind,
    size: (u32, u32),
    secs: Option<f64>,
) -> Result<Entry, String> {
    let shots = app.state::<Shots>();
    let tf = std::time::Instant::now();
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
    let copied = prefs.after.copy
        && match kind {
            Kind::Screenshot => mac::copy_image(&path).is_ok(),
            _ => mac::copy_file(&path).is_ok(),
        };
    super::perf::mark("finish.clipboard", tf);
    if prefs.after.sound && kind == Kind::Screenshot {
        mac::shutter();
    }
    shots
        .history
        .lock()
        .push(entry.clone(), prefs.history as usize);
    shots.save_history();
    super::perf::mark("finish.history", tf);
    if prefs.after.quick_access {
        let tt = std::time::Instant::now();
        let thumb = thumb_of(&path, kind).unwrap_or_default();
        super::perf::mark("finish.thumb", tt);
        let q = QuickState {
            entry: entry.clone(),
            thumb,
            dismiss_secs: prefs.after.dismiss_secs,
            copied,
        };
        let tq = std::time::Instant::now();
        super::quick::show(app, &q)?;
        super::perf::mark("finish.card", tq);
    }
    Ok(entry)
}

/// A 440 px-wide thumbnail data URL for the card (PNG only; videos get a poster in `record`).
fn thumb_of(path: &Path, kind: Kind) -> Option<String> {
    if kind != Kind::Screenshot {
        return super::record_mac::poster(path);
    }
    let img = pixels::decode_png(&std::fs::read(path).ok()?).ok()?;
    let (data, (w, h)) = pixels::thumbnail(&img, 440);
    pixels::data_url(&data, w, h).ok()
}
