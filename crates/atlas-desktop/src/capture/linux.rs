//! Stills on Linux (and the glue of recording and live frames): `grim` against the private sway of an agent instance (#282, #280).
//!
//! An agent instance (`atlas desktop --agent`) renders on a headless compositor nobody can see, so its pixels are
//! read from that compositor directly: no portal, no consent prompt, nothing of the user's session involved. That is
//! also the rule here: every capture first checks (`atlas_core::agent_display::verify_linux`) that the process runs in
//! an agent instance and that its environment points at the instance's own display, and refuses otherwise. The
//! user's own Atlas on Linux has no stills (a screenshot of his session would need a portal and his consent).
//!
//! Window = the compositor's picture of the window (`grim -g <window rect>`, from sway's tree). Page = the webview's
//! rectangle, or a CSS-px region of it: sway lays out in logical pixels, which are CSS pixels, so a region maps by
//! adding the webview's origin; `grim` writes physical pixels (`ATLAS_AGENT_SCALE=2` gives a retina file).

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::{linux_rec, request};

/// A rectangle in the compositor's logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// The grim `-g` argument: `x,y WxH`.
    pub fn geometry(self) -> String {
        format!("{},{} {}x{}", self.x, self.y, self.w, self.h)
    }
}

/// The smallest whole-pixel rectangle holding a CSS-px region `(x, y, w, h)` that starts at `origin`.
pub fn region_rect(origin: (i32, i32), region: (f64, f64, f64, f64)) -> Rect {
    let (x0, y0) = (region.0.floor() as i32, region.1.floor() as i32);
    let (x1, y1) = (
        (region.0 + region.2).ceil() as i32,
        (region.1 + region.3).ceil() as i32,
    );
    Rect {
        x: origin.0 + x0,
        y: origin.1 + y0,
        w: (x1 - x0).max(1),
        h: (y1 - y0).max(1),
    }
}

fn int(v: &Value, k: &str) -> Option<i32> {
    i32::try_from(v[k].as_i64()?).ok()
}

/// The window of process `pid` in sway's `get_tree`, as the surface the client draws (its container's rect plus
/// the offset of `window_rect`, which excludes borders). A process with several windows (popovers, HUDs) is
/// answered with its largest: the main window.
pub fn window_rect(tree: &Value, pid: u32) -> Option<Rect> {
    let mut best: Option<Rect> = None;
    let mut stack = vec![tree];
    while let Some(n) = stack.pop() {
        for k in ["nodes", "floating_nodes"] {
            stack.extend(n[k].as_array().into_iter().flatten());
        }
        if n["pid"].as_u64() != Some(u64::from(pid)) {
            continue;
        }
        let (r, w) = (&n["rect"], &n["window_rect"]);
        let (Some(x), Some(y), Some(ww), Some(wh)) = (
            int(r, "x").zip(int(w, "x")).map(|(a, b)| a + b),
            int(r, "y").zip(int(w, "y")).map(|(a, b)| a + b),
            int(w, "width"),
            int(w, "height"),
        ) else {
            continue;
        };
        let cand = Rect { x, y, w: ww, h: wh };
        if ww > 0
            && wh > 0
            && best.is_none_or(|b| i64::from(b.w) * i64::from(b.h) < i64::from(ww) * i64::from(wh))
        {
            best = Some(cand);
        }
    }
    best
}

/// Refuse unless this process is an agent instance running on its own private display.
fn require_private_display() -> Result<(), String> {
    if crate::agent_mode::Mode::current() != crate::agent_mode::Mode::Agent {
        return Err("stills on Linux work only in an agent instance (`atlas desktop --agent`): they are read from its private display".into());
    }
    atlas_core::agent_display::verify_linux(|k| std::env::var(k).ok())
        .map_err(|e| format!("not capturing: this process is not on its private display ({e})"))
}

fn tool_error(tool: &str, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!(
            "{tool} not found: stills, recordings and the live view need grim and sway (Arch: `pacman -S sway grim`; Debian/Ubuntu: `apt install sway grim`)"
        )
    } else {
        format!("{tool}: {e}")
    }
}

/// The main window of this process, from the compositor.
pub fn own_window() -> Result<Rect, String> {
    require_private_display()?;
    let out = Command::new("swaymsg")
        .args(["-t", "get_tree", "-r"])
        .output()
        .map_err(|e| tool_error("swaymsg", &e))?;
    if !out.status.success() {
        return Err(format!(
            "swaymsg: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let tree: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("swaymsg: {e}"))?;
    window_rect(&tree, std::process::id())
        .ok_or_else(|| "the compositor has no window of this process (is it shown?)".into())
}

/// Run `grim -g <rect> <args> <dest>` and return what it wrote to stdout (`dest` is `-`) or nothing (a file path).
fn grim_run(
    rect: Rect,
    args: &[&str],
    dest: impl AsRef<std::ffi::OsStr>,
) -> Result<Vec<u8>, String> {
    require_private_display()?;
    let res = Command::new("grim")
        .args(args)
        .arg("-g")
        .arg(rect.geometry())
        .arg(dest)
        .output()
        .map_err(|e| tool_error("grim", &e))?;
    if !res.status.success() {
        return Err(format!(
            "grim: {}",
            String::from_utf8_lossy(&res.stderr).trim()
        ));
    }
    Ok(res.stdout)
}

/// Write the PNG of `rect` (compositor logical pixels) to `out`.
pub fn grim(rect: Rect, out: &Path) -> Result<(), String> {
    grim_run(rect, &[], out).map(drop)
}

/// One frame of `rect` as bytes: `["-t", "ppm"]` for the recorder, `["-t", "jpeg", "-q", "60"]` for the live view.
pub fn grim_frame(rect: Rect, args: &[&str]) -> Result<Vec<u8>, String> {
    grim_run(rect, args, "-")
}

/// Resize the main window to `(w, h)` logical px and give layout and the compositor a beat to settle.
pub fn resize_window(app: &AppHandle, (w, h): (u32, u32)) -> Result<(), String> {
    let win = app.get_window("main").ok_or("no window \"main\"")?;
    win.set_size(tauri::LogicalSize::new(f64::from(w), f64::from(h)))
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

/// A page region in CSS px at a device scale (the scale is the page's: sway's logical pixels are CSS pixels, so only
/// the origin matters here).
pub type Region = (f64, f64, f64, f64, f64);

/// The main webview's origin in the compositor's logical pixels, and the window it sits in.
fn view_origin(app: &AppHandle) -> Result<((i32, i32), Rect), String> {
    let win = own_window()?;
    let view = app.get_webview("main").ok_or("no webview \"main\"")?;
    let scale = view.window().scale_factor().map_err(|e| e.to_string())?;
    let pos = view.position().map_err(|e| e.to_string())?;
    let origin = (
        win.x + (f64::from(pos.x) / scale).round() as i32,
        win.y + (f64::from(pos.y) / scale).round() as i32,
    );
    Ok((origin, win))
}

/// The main webview as the compositor shows it: what a page still and the live view capture. The webview fills the
/// window below its own origin; the compositor's window size is the truth (`WebviewWindow::size` left out the bottom
/// 27 px in a live run).
pub fn view_rect(app: &AppHandle) -> Result<Rect, String> {
    let (origin, win) = view_origin(app)?;
    Ok(Rect {
        x: origin.0,
        y: origin.1,
        w: (win.x + win.w - origin.0).max(1),
        h: (win.y + win.h - origin.1).max(1),
    })
}

/// The main webview (a CSS-px region of it, or all of it) into a PNG, read from the instance's compositor.
pub fn snapshot_to(app: &AppHandle, region: Option<Region>, out: &Path) -> Result<(), String> {
    let rect = match region {
        Some((x, y, w, h, _)) => region_rect(view_origin(app)?.0, (x, y, w, h)),
        None => view_rect(app)?,
    };
    grim(rect, out)
}

/// `capture_shot`: the window as the compositor shows it (`window`) or the page only (`snapshot`).
pub fn shot(app: &AppHandle, req: request::Shot) -> Result<Value, String> {
    main_window_only(&req.window, "stills")?;
    if let Some(dir) = req.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if let Some(size) = req.size {
        resize_window(app, size)?;
    }
    match req.mode {
        request::ShotMode::Window => grim(own_window()?, &req.out)?,
        request::ShotMode::Snapshot => snapshot_to(app, None, &req.out)?,
    }
    let meta = std::fs::metadata(&req.out).map_err(|e| e.to_string())?;
    Ok(
        json!({ "out": req.out, "bytes": meta.len(), "mode": format!("{:?}", req.mode).to_lowercase() }),
    )
}

fn main_window_only(window: &str, what: &str) -> Result<(), String> {
    if window == "main" {
        return Ok(());
    }
    Err(format!(
        "{what} on Linux are of the main window, not {window:?}"
    ))
}

/// `capture_start`: film the window (as the compositor shows it, title row included) until `capture_stop`. The
/// first frame is taken here, so a missing `grim`/`ffmpeg`, or a window that is not shown, fails this call.
pub fn start(
    app: &AppHandle,
    state: &super::Capture,
    req: request::Start,
) -> Result<Value, String> {
    main_window_only(&req.window, "recordings")?;
    require_private_display()?;
    let mut active = state.active.lock();
    if active.is_some() {
        return Err("a recording is already running; stop it first (capture_stop)".into());
    }
    if let Some(dir) = req.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if let Some(size) = req.size {
        resize_window(app, size)?;
    }
    let rect = own_window()?;
    let args: &'static [&'static str] = if req.cursor {
        &["-t", "ppm", "-c"]
    } else {
        &["-t", "ppm"]
    };
    let first = grim_frame(rect, args)?;
    let rec = linux_rec::Recorder::start(first, move || grim_frame(rect, args), req.fps, &req.out)?;
    let (width, height) = rec.pixels;
    let started_epoch_ms = rec.started_epoch_ms;
    *active = Some(rec);
    Ok(json!({
        "out": req.out,
        "width": width,
        "height": height,
        "window": { "width": rect.w, "height": rect.h },
        "fps": req.fps,
        "started_epoch_ms": started_epoch_ms,
    }))
}

/// `capture_stop`: close the file and report what was captured (`frames` are grabs, the file is constant `fps`).
pub fn stop(state: &super::Capture) -> Result<Value, String> {
    let rec = state
        .active
        .lock()
        .take()
        .ok_or("no recording is running")?;
    let (out, fps, (width, height), started_epoch_ms) =
        (rec.out.clone(), rec.fps, rec.pixels, rec.started_epoch_ms);
    let wall_ms = rec.started.elapsed().as_millis() as u64;
    let tally = rec.stop()?;
    let bytes = std::fs::metadata(&out).map_err(|e| e.to_string())?.len();
    let duration_ms = tally.duration_ms();
    Ok(json!({
        "out": out,
        "width": width,
        "height": height,
        "fps": fps,
        "frames": tally.frames,
        "duration_ms": duration_ms,
        "wall_ms": wall_ms,
        "max_gap_ms": tally.max_gap_ms,
        "avg_fps": if duration_ms > 0 { tally.frames as f64 * 1000.0 / duration_ms as f64 } else { 0.0 },
        "bytes": bytes,
        "first_frame_offset_ms": tally.first_ms,
        "started_epoch_ms": started_epoch_ms,
    }))
}

pub fn status(state: &super::Capture) -> Value {
    let a = state.active.lock();
    json!({
        "recording": a.is_some(),
        "out": a.as_ref().map(|r| r.out.to_string_lossy().into_owned()),
        "elapsed_ms": a.as_ref().map(|r| r.started.elapsed().as_millis() as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree() -> Value {
        // Shape of `swaymsg -t get_tree -r`: root > output > workspace > floating window, plus a second window of
        // the same process (a popover) and one of another process.
        let win = |pid: u64, x: i64, y: i64, w: i64, h: i64| {
            json!({"type": "floating_con", "pid": pid, "rect": {"x": x, "y": y, "width": w, "height": h},
                   "window_rect": {"x": 0, "y": 0, "width": w, "height": h}, "nodes": [], "floating_nodes": []})
        };
        json!({"type": "root", "nodes": [{"type": "output", "nodes": [
            {"type": "workspace", "nodes": [], "floating_nodes": [
                win(7, 100, 40, 1280, 820), win(7, 0, 0, 300, 200), win(9, 5, 5, 1440, 900)]}
        ], "floating_nodes": []}], "floating_nodes": []})
    }

    #[test]
    fn the_window_is_this_processs_largest_and_no_other_processs() {
        let r = window_rect(&tree(), 7).unwrap();
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 40,
                w: 1280,
                h: 820
            }
        );
        assert_eq!(window_rect(&tree(), 8), None);
    }

    #[test]
    fn a_decorated_window_is_cropped_to_what_the_client_draws() {
        let t = json!({"pid": 7, "rect": {"x": 10, "y": 20, "width": 600, "height": 400},
                       "window_rect": {"x": 2, "y": 24, "width": 596, "height": 374}, "nodes": [], "floating_nodes": []});
        assert_eq!(
            window_rect(&t, 7),
            Some(Rect {
                x: 12,
                y: 44,
                w: 596,
                h: 374
            })
        );
    }

    #[test]
    fn a_css_region_is_the_whole_pixels_around_it_from_the_webviews_origin() {
        let r = region_rect((100, 40), (10.4, 20.5, 100.2, 30.0));
        assert_eq!(
            r,
            Rect {
                x: 110,
                y: 60,
                w: 101,
                h: 31
            }
        );
        assert_eq!(r.geometry(), "110,60 101x31");
        assert_eq!(
            region_rect((0, 0), (5.0, 5.0, 0.0, 0.0)).w,
            1,
            "never an empty geometry"
        );
    }
}
