//! Built-in capture of Atlas's own windows: video and stills without opening, focusing or driving
//! the user's desktop. `capture_start|stop|shot|status` are served on the ctl socket (every build,
//! token-gated, local only) and on the debug bridge; scripted recordings (`capture_run`) are
//! forwarded to the frontend, which drives the app through its stores and calls these commands.
//!
//! See `sck.rs` for what ScreenCaptureKit can and cannot do (occlusion, Spaces, permission).

#[cfg(target_os = "macos")]
mod chrome;
#[cfg(target_os = "macos")]
mod live;
#[cfg(target_os = "macos")]
pub(crate) mod occlusion;
#[cfg(target_os = "macos")]
mod png;
// Parsing is pure and tested everywhere; only the ScreenCaptureKit side uses it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod request;
#[cfg(target_os = "macos")]
mod sck;
#[cfg(target_os = "macos")]
mod snapshot;

#[cfg(target_os = "macos")]
use std::sync::Arc;

use serde_json::Value;
#[cfg(target_os = "macos")]
use serde_json::json;
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Commands this module answers itself (the rest of ctl goes to the frontend).
pub const COMMANDS: [&str; 4] = [
    "capture_start",
    "capture_stop",
    "capture_shot",
    "capture_status",
];

pub fn handles(cmd: &str) -> bool {
    COMMANDS.contains(&cmd)
}

/// A page region in CSS px at a device scale, for [`snapshot_to`].
#[cfg(target_os = "macos")]
pub fn snapshot_region(x: f64, y: f64, w: f64, h: f64, scale: f64) -> snapshot::Region {
    snapshot::Region { x, y, w, h, scale }
}

/// `WKWebView.takeSnapshot` of the main webview (a region of it, or all of it) into a PNG.
#[cfg(target_os = "macos")]
pub fn snapshot_to(
    app: &AppHandle,
    region: Option<snapshot::Region>,
    out: &std::path::Path,
) -> Result<(), String> {
    let view = app.get_webview("main").ok_or("no webview \"main\"")?;
    snapshot::region_png(&view, region, out)
}

/// Non-macOS: no `WKWebView.takeSnapshot`; the region is kept as (x, y, w, h, scale) only so the
/// debug bridge (debug builds) type-checks, and [`snapshot_to`] refuses.
#[cfg(all(not(target_os = "macos"), debug_assertions))]
pub fn snapshot_region(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (f64, f64, f64, f64, f64) {
    (x, y, w, h, scale)
}

#[cfg(all(not(target_os = "macos"), debug_assertions))]
pub fn snapshot_to(
    _: &AppHandle,
    _: Option<(f64, f64, f64, f64, f64)>,
    _: &std::path::Path,
) -> Result<(), String> {
    Err("page snapshots need macOS (WKWebView.takeSnapshot)".into())
}

#[derive(Default)]
pub struct Capture {
    #[cfg(target_os = "macos")]
    active: parking_lot::Mutex<Option<Active>>,
}

#[cfg(target_os = "macos")]
struct Active {
    rec: sck::Recorder,
    out: std::path::PathBuf,
    started: std::time::Instant,
    /// Wall clock (ms since epoch) when the start call returned: the origin scripts align to.
    started_epoch_ms: u128,
    fps: u32,
    deadline: tauri::async_runtime::JoinHandle<()>,
    /// The window's occlusion bit forced to visible so the page keeps running; the real bit is restored on stop.
    occlusion: Option<occlusion::Forced>,
    /// The window's standard buttons, hidden so the OS "being shared" pill is not in the picture; restored on stop.
    chrome: Option<chrome::Hidden>,
}

/// The frontend's runner calls the same handlers the ctl socket serves (`capture_start|stop|shot|status`).
#[tauri::command]
pub async fn capture_cmd(app: AppHandle, cmd: String, args: Value) -> Result<Value, String> {
    if !handles(&cmd) {
        return Err(format!("unknown capture command {cmd:?}"));
    }
    handle(&app, &cmd, &args).await
}

#[cfg(not(target_os = "macos"))]
pub async fn handle(_: &AppHandle, _: &str, _: &Value) -> Result<Value, String> {
    Err("capture needs macOS (ScreenCaptureKit)".into())
}

#[cfg(target_os = "macos")]
pub async fn handle(app: &AppHandle, cmd: &str, args: &Value) -> Result<Value, String> {
    let state = app.state::<Arc<Capture>>().inner().clone();
    match cmd {
        "capture_status" => Ok(status(&state)),
        "capture_start" => {
            let req = request::parse_start(args)?;
            let app2 = app.clone();
            let st = state.clone();
            tauri::async_runtime::spawn_blocking(move || start(&app2, &st, req))
                .await
                .map_err(|e| e.to_string())?
        }
        "capture_stop" => {
            let st = state.clone();
            tauri::async_runtime::spawn_blocking(move || stop(&st))
                .await
                .map_err(|e| e.to_string())?
        }
        "capture_shot" => {
            let req = request::parse_shot(args)?;
            let app2 = app.clone();
            tauri::async_runtime::spawn_blocking(move || shot(&app2, req))
                .await
                .map_err(|e| e.to_string())?
        }
        other => Err(format!("unknown capture command {other:?}")),
    }
}

#[cfg(target_os = "macos")]
fn status(state: &Capture) -> Value {
    let a = state.active.lock();
    json!({
        "permission": sck::permission_granted(),
        "recording": a.is_some(),
        "out": a.as_ref().map(|a| a.out.to_string_lossy().into_owned()),
        "elapsed_ms": a.as_ref().map(|a| a.started.elapsed().as_millis() as u64),
    })
}

/// Logical size and display scale of `win`, after resizing it to `size` (logical px) if asked.
#[cfg(target_os = "macos")]
pub(crate) fn prepare_window(
    app: &AppHandle,
    label: &str,
    size: Option<(u32, u32)>,
) -> Result<(tauri::Window, u32, (f64, f64), f64), String> {
    let win = app
        .get_window(label)
        .ok_or_else(|| format!("no window {label:?}"))?;
    if !win.is_visible().unwrap_or(false) {
        return Err(format!(
            "window {label:?} is hidden; a hidden window produces no frames (use mode \"snapshot\" for stills)"
        ));
    }
    if let Some((w, h)) = size {
        win.set_size(tauri::LogicalSize::new(f64::from(w), f64::from(h)))
            .map_err(|e| e.to_string())?;
        // Layout needs a beat to settle at the new size before the first frame.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let inner = win.inner_size().map_err(|e| e.to_string())?;
    let logical = (
        f64::from(inner.width) / scale,
        f64::from(inner.height) / scale,
    );
    let id = crate::window_number(&win).ok_or("the window has no native id")?;
    Ok((win, id as u32, logical, scale))
}

#[cfg(target_os = "macos")]
fn require_permission() -> Result<(), String> {
    if sck::permission_granted() {
        return Ok(());
    }
    // Asks once (the system prompt); the grant takes effect after a relaunch, so fail with the how-to.
    sck::request_permission();
    Err("Screen Recording permission is missing. Grant it to Atlas (or to the terminal that launched it) in System Settings > Privacy & Security > Screen Recording, then restart. `capture_shot` with mode \"snapshot\" needs no permission.".into())
}

#[cfg(target_os = "macos")]
fn start(app: &AppHandle, state: &Arc<Capture>, req: request::Start) -> Result<Value, String> {
    if state.active.lock().is_some() {
        return Err("a recording is already running; stop it first (capture_stop)".into());
    }
    require_permission()?;
    let (win, id, logical, scale) = prepare_window(app, &req.window, req.size)?;
    // The window's inside, in pixels. The title bar is part of the window, so it is part of the picture.
    let outer = app
        .get_window(&req.window)
        .and_then(|w| w.outer_size().ok())
        .map(|s| (f64::from(s.width) / scale, f64::from(s.height) / scale))
        .unwrap_or(logical);
    let (pw, ph) = request::pixel_size(outer, scale);
    if let Some(dir) = req.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // WebKit pauses a page its window does not show (rAF stops, animations freeze): run it as visible for the recording.
    let forced = occlusion::Forced::force_visible(&win).ok().flatten();
    let pinned = forced.is_some();
    let win_is_key = win.is_focused().unwrap_or(false);
    // macOS draws a "being shared" pill where the traffic lights are, into the frames: hide the buttons first.
    let hidden = chrome::Hidden::hide(&win).ok();
    let rec = match sck::Recorder::start(
        id,
        (pw as usize, ph as usize),
        req.fps,
        req.cursor,
        &req.out,
    ) {
        Ok(r) => r,
        Err(e) => {
            if let Some(f) = &forced {
                f.restore();
            }
            if let Some(h) = &hidden {
                h.restore();
            }
            return Err(e);
        }
    };
    let started_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // A recording nobody stops must not run forever.
    let st = state.clone();
    let deadline = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(request::MAX_RECORD_SECS)).await;
        let _ = tauri::async_runtime::spawn_blocking(move || stop(&st)).await;
    });
    let out = req.out.clone();
    *state.active.lock() = Some(Active {
        rec,
        out: req.out,
        started: std::time::Instant::now(),
        started_epoch_ms,
        fps: req.fps,
        deadline,
        occlusion: forced,
        chrome: hidden,
    });
    Ok(json!({
        "out": out,
        "width": pw,
        "height": ph,
        "scale": scale,
        "window": { "width": logical.0, "height": logical.1, "outer_width": outer.0, "outer_height": outer.1 },
        "fps": req.fps,
        "started_epoch_ms": started_epoch_ms,
        "page_pinned_visible": pinned,
        // WebKit renders a window that is not key at 30 Hz; the video container runs at `fps`, the page content at this.
        "page_fps_cap": request::page_fps_cap(win_is_key, req.fps),
    }))
}

#[cfg(target_os = "macos")]
fn stop(state: &Capture) -> Result<Value, String> {
    let a = state
        .active
        .lock()
        .take()
        .ok_or("no recording is running")?;
    a.deadline.abort();
    let wall_ms = a.started.elapsed().as_millis() as u64;
    let (width, height) = a.rec.pixels;
    let stopped = a.rec.stop();
    // The stream is stopped: the buttons come back after the last frame, even when stopping failed.
    if let Some(h) = &a.chrome {
        h.restore();
    }
    if let Some(f) = &a.occlusion {
        f.restore();
    }
    let (stats, bytes) = stopped?;
    // First frame time relative to the start call: scripts shift their timeline by this much.
    let first_frame_offset_ms = if stats.frames > 0 {
        Some(first_frame_offset(a.started_epoch_ms, stats.first_pts))
    } else {
        None
    };
    if stats.frames == 0 {
        return Err(
            "no frames were captured: the window was minimized, hidden or on another Space".into(),
        );
    }
    let duration_ms = ((stats.last_pts - stats.first_pts) * 1000.0).round() as u64;
    Ok(json!({
        "out": a.out,
        "width": width,
        "height": height,
        "fps": a.fps,
        "frames": stats.frames,
        "duration_ms": duration_ms,
        "wall_ms": wall_ms,
        "max_gap_ms": stats.max_gap_ms,
        "avg_fps": if duration_ms > 0 { stats.frames as f64 * 1000.0 / duration_ms as f64 } else { 0.0 },
        "bytes": bytes,
        "first_frame_offset_ms": first_frame_offset_ms,
        "started_epoch_ms": a.started_epoch_ms,
    }))
}

/// Offset in ms from the moment the start call returned to the first delivered frame. The stream's
/// presentation timestamps are on the host clock; map them onto the wall clock via "now".
#[cfg(target_os = "macos")]
fn first_frame_offset(started_epoch_ms: u128, first_pts: f64) -> i64 {
    let now_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis()) as f64;
    let now_host = sck::host_clock_seconds();
    let first_epoch_ms = now_epoch_ms - (now_host - first_pts) * 1000.0;
    (first_epoch_ms - started_epoch_ms as f64).round() as i64
}

#[cfg(target_os = "macos")]
fn shot(app: &AppHandle, req: request::Shot) -> Result<Value, String> {
    if let Some(dir) = req.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    match req.mode {
        request::ShotMode::Snapshot => {
            if let Some((w, h)) = req.size {
                let win = app
                    .get_window(&req.window)
                    .ok_or_else(|| format!("no window {:?}", req.window))?;
                win.set_size(tauri::LogicalSize::new(f64::from(w), f64::from(h)))
                    .map_err(|e| e.to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
            let view = app
                .get_webview(&req.window)
                .ok_or_else(|| format!("no webview {:?}", req.window))?;
            snapshot::page_png(&view, &req.out)?;
        }
        request::ShotMode::Window => {
            require_permission()?;
            let (_w, id, logical, scale) = prepare_window(app, &req.window, req.size)?;
            let (pw, ph) = request::pixel_size(logical, scale);
            // Window capture includes the title bar: ask for the outer size.
            let outer = app
                .get_window(&req.window)
                .and_then(|w| w.outer_size().ok())
                .map_or((pw, ph), |s| (s.width, s.height));
            sck::window_png(id, (outer.0 as usize, outer.1 as usize), &req.out)?;
        }
    }
    let meta = std::fs::metadata(&req.out).map_err(|e| e.to_string())?;
    Ok(
        json!({ "out": req.out, "bytes": meta.len(), "mode": format!("{:?}", req.mode).to_lowercase() }),
    )
}

/// One live frame: sequence number, JPEG bytes, pixel size.
pub type Frame = (u64, std::sync::Arc<Vec<u8>>, (usize, usize));

/// The live stream behind `debug frame`, started on the first pull and stopped after `live::IDLE` without one.
#[cfg(target_os = "macos")]
static LIVE: parking_lot::Mutex<Option<live::Live>> = parking_lot::Mutex::new(None);

/// Newest frame of the main window after `since`, waiting up to `wait` for one.
#[cfg(target_os = "macos")]
pub fn live_frame(app: &AppHandle, since: u64, wait: std::time::Duration) -> Result<Frame, String> {
    require_permission()?;
    let (_w, id, logical, scale) = prepare_window(app, "main", None)?;
    let (pw, ph) = request::pixel_size(logical, scale);
    let shared = {
        let mut guard = LIVE.lock();
        if guard.as_ref().is_none_or(|l| l.window() != id) {
            if let Some(old) = guard.take() {
                old.stop();
            }
            *guard = Some(live::Live::start(id, (pw as usize, ph as usize))?);
            std::thread::spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let mut g = LIVE.lock();
                    if g.as_ref().is_none_or(|l| l.idle_for() >= live::IDLE) {
                        if let Some(l) = g.take() {
                            l.stop();
                        }
                        return;
                    }
                }
            });
        }
        let l = guard.as_mut().expect("started above");
        l.touch();
        l.shared()
    };
    // Wait without the global lock: other pulls and the idle watcher keep going.
    Ok(live::pull(&shared, since, wait))
}

#[cfg(not(target_os = "macos"))]
pub fn live_frame(_: &AppHandle, _: u64, _: std::time::Duration) -> Result<Frame, String> {
    Err("live frames need macOS (ScreenCaptureKit)".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_commands_are_routed_here_and_everything_else_goes_to_the_frontend() {
        for c in [
            "capture_start",
            "capture_stop",
            "capture_shot",
            "capture_status",
        ] {
            assert!(handles(c), "{c}");
        }
        // `capture_run` is the scripted recording: it needs the app's stores, so the frontend answers it.
        for c in [
            "capture_run",
            "capture",
            "worktree_create",
            "capture.start",
            "",
        ] {
            assert!(
                !handles(c),
                "{c:?} must not be swallowed by the native handler"
            );
        }
    }
}
