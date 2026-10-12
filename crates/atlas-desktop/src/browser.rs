//! Embedded browser: one child webview per `browser` tab, laid over the tab's
//! pane (the frontend sends the pane rect). Only http(s) navigation.
//!
//! Design Mode script (`browser_grab.js`) is injected into every page; it holds
//! no secret. The app reads results with `eval_with_callback`, never over the network.
//!
//! Events: `browser://<tab>` `{event: "load"|"title"|"nav", url, title?}`.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Url, WebviewUrl};
use tokio::sync::oneshot;

const GRAB_JS: &str = include_str!("browser_grab.js");
/// Hidden child webviews are parked here (no per-child hide on every platform).
const OFFSCREEN: f64 = -20_000.0;
/// Below this many logical px a rect is a layout in flight (hidden pane, 0×0 area), not a pane.
const MIN_SIDE: f64 = 4.0;
/// The window every child webview belongs to; the tray popover is another window.
const MAIN: &str = "main";

fn label(tab: &str) -> String {
    format!(
        "browser-{}",
        tab.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "")
    )
}

/// Inverse of `label` (tab ids are UUIDs, which `label` leaves untouched).
fn tab_of(label: &str) -> &str {
    label.strip_prefix("browser-").unwrap_or(label)
}

/// The tab's child webview. Children always belong to the main window: a same-label
/// webview in any other window (the tray popover) is never ours.
fn webview(app: &AppHandle, tab: &str) -> Result<tauri::Webview, String> {
    app.get_webview(&label(tab))
        .filter(|v| v.window().label() == MAIN)
        .ok_or_else(|| format!("no browser for tab {tab}"))
}

/// A pane rect in logical px, relative to the window content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pane {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Pane {
    /// A rect that can really hold a page: finite, positive, not a transient 0×0 or 1×1.
    fn usable(&self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.w >= MIN_SIDE
            && self.h >= MIN_SIDE
    }
}

/// Where a child webview must sit: over its pane, or parked offscreen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    Over(Pane),
    Parked,
}

impl Placement {
    /// The pane only counts when it is visible AND usable; anything else parks.
    fn resolve(pane: Pane, visible: bool) -> Self {
        if visible && pane.usable() {
            Placement::Over(pane)
        } else {
            Placement::Parked
        }
    }

    /// Native bounds: parked views keep a sane size so a later unpark never flashes 1×1.
    fn bounds(self, last: Option<Pane>) -> Rect {
        let (x, y, w, h) = match self {
            Placement::Over(p) => (p.x, p.y, p.w, p.h),
            Placement::Parked => {
                let (w, h) = last.map_or((MIN_SIDE, MIN_SIDE), |p| (p.w, p.h));
                (OFFSCREEN, OFFSCREEN, w, h)
            }
        };
        Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(w, h).into(),
        }
    }
}

/// What `browser_open` must do for a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenPlan {
    /// No live webview: build one.
    Create,
    /// A live webview exists (e.g. it outlived a frontend reload): only reposition it.
    Reuse,
}

/// Per-tab bookkeeping, keyed by tab id. The native webview is the authority on
/// *existence* (`live` is passed in); this records the last placement we asked for,
/// so a recreated component can be told apart from a fresh tab and a parked view
/// keeps its size.
#[derive(Default)]
struct Registry {
    tabs: HashMap<String, Placed>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    placement: Placement,
    /// Last usable pane, to size parked views.
    last: Option<Pane>,
}

impl Registry {
    /// Idempotent open: reuse when the native view is alive, else (re)create. An entry
    /// whose view is gone (window recreated, webview crashed) is dropped first.
    fn open(&mut self, tab: &str, live: bool, pane: Pane, visible: bool) -> (OpenPlan, Placement) {
        let plan = if live {
            OpenPlan::Reuse
        } else {
            self.tabs.remove(tab);
            OpenPlan::Create
        };
        (plan, self.place(tab, pane, visible))
    }

    /// Record and return where `tab` must sit for this report.
    fn place(&mut self, tab: &str, pane: Pane, visible: bool) -> Placement {
        let placement = Placement::resolve(pane, visible);
        let prev = self.tabs.get(tab).and_then(|p| p.last);
        let last = if pane.usable() { Some(pane) } else { prev };
        self.tabs.insert(tab.into(), Placed { placement, last });
        placement
    }

    /// Park without a rect (component destroyed, boot): keep the last size.
    fn park(&mut self, tab: &str) -> Option<Placed> {
        let p = self.tabs.get_mut(tab)?;
        p.placement = Placement::Parked;
        Some(*p)
    }

    fn close(&mut self, tab: &str) -> bool {
        self.tabs.remove(tab).is_some()
    }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Default::default);
/// Serializes open/close so two racing `browser_open`s cannot both `add_child` one label.
static LIFECYCLE: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

fn apply(v: &tauri::Webview, at: Placement, last: Option<Pane>) -> Result<(), String> {
    // One native call: position and size never disagree for a frame.
    v.set_bounds(at.bounds(last)).map_err(|e| e.to_string())
}

/// Every live browser child of the main window, as tab ids' labels (`browser-<id>`).
fn live_labels(app: &AppHandle) -> Vec<String> {
    app.webviews()
        .into_iter()
        .filter(|(l, v)| l.starts_with("browser-") && v.window().label() == MAIN)
        .map(|(l, _)| l)
        .collect()
}

pub fn parse_url(raw: &str) -> Result<Url, String> {
    let u = Url::parse(raw).map_err(|e| format!("{raw}: {e}"))?;
    if matches!(u.scheme(), "http" | "https") {
        Ok(u)
    } else {
        Err(format!("only http(s) URLs: {raw}"))
    }
}

/// Open (or re-adopt) the child webview for `tab`. Idempotent: an existing webview is
/// only repositioned. Created parked unless `visible` and the rect is usable, so a
/// restored background tab never flashes over the UI.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn browser_open(
    app: AppHandle,
    tab: String,
    url: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    visible: bool,
) -> Result<(), String> {
    let _gate = LIFECYCLE.lock().await;
    let pane = Pane { x, y, w, h };
    let existing = webview(&app, &tab).ok();
    let (plan, at) = REGISTRY
        .lock()
        .open(&tab, existing.is_some(), pane, visible);
    let last = REGISTRY.lock().tabs.get(&tab).and_then(|p| p.last);
    if plan == OpenPlan::Reuse
        && let Some(v) = existing
    {
        return apply(&v, at, last);
    }
    let u = parse_url(&url)?;
    let window = app.get_window(MAIN).ok_or("no main window")?;
    let (a1, a2, a3, t1, t2, t3) = (
        app.clone(),
        app.clone(),
        app.clone(),
        tab.clone(),
        tab.clone(),
        tab.clone(),
    );
    let builder = WebviewBuilder::new(label(&tab), WebviewUrl::External(u))
        .initialization_script(GRAB_JS)
        .on_navigation(move |u| {
            let ok = matches!(u.scheme(), "http" | "https" | "about");
            if ok {
                let _ = a1.emit(
                    &format!("browser://{t1}"),
                    json!({"event": "nav", "url": u.as_str()}),
                );
            }
            ok
        })
        .on_page_load(move |_, p| {
            if matches!(p.event(), PageLoadEvent::Finished) {
                let _ = a2.emit(
                    &format!("browser://{t2}"),
                    json!({"event": "load", "url": p.url().as_str()}),
                );
            }
        })
        .on_document_title_changed(move |_, title| {
            let _ = a3.emit(
                &format!("browser://{t3}"),
                json!({"event": "title", "title": title}),
            );
        });
    // Born at its final placement: never full-window, never over the UI first.
    let b = at.bounds(last);
    let created = window.add_child(builder, b.position, b.size);
    if let Err(e) = created {
        REGISTRY.lock().close(&tab);
        return Err(e.to_string());
    }
    Ok(())
}

/// Move/resize over the pane. Parks (keeping the last good size) when `visible` is false
/// or the rect is degenerate (hidden pane, 0×0 area, NaN).
#[tauri::command]
pub fn browser_bounds(
    app: AppHandle,
    tab: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    visible: bool,
) -> Result<(), String> {
    let v = webview(&app, &tab)?;
    let (at, last) = {
        let mut r = REGISTRY.lock();
        let at = r.place(&tab, Pane { x, y, w, h }, visible);
        (at, r.tabs.get(&tab).and_then(|p| p.last))
    };
    apply(&v, at, last)
}

/// Park one webview without a rect (its component went away; the tab may only be moving).
#[tauri::command]
pub fn browser_park(app: AppHandle, tab: String) -> Result<(), String> {
    let Ok(v) = webview(&app, &tab) else {
        return Ok(());
    };
    let last = REGISTRY.lock().park(&tab).and_then(|p| p.last);
    apply(&v, Placement::Parked, last)
}

/// Park every browser webview (frontend boot: no pane owns them yet).
#[tauri::command]
pub fn browser_hide_all(app: AppHandle) {
    for label in live_labels(&app) {
        if let Some(v) = app.get_webview(&label) {
            let tab = tab_of(&label);
            let last = REGISTRY.lock().park(tab).and_then(|p| p.last);
            let _ = apply(&v, Placement::Parked, last);
        }
    }
}

/// Tab ids (label form) of every live browser child of the main window. After a
/// frontend reload the page asks this to close views whose tab no longer exists.
#[tauri::command]
pub fn browser_list(app: AppHandle) -> Vec<String> {
    let mut v: Vec<_> = live_labels(&app)
        .into_iter()
        .map(|l| tab_of(&l).to_string())
        .collect();
    v.sort();
    v
}

#[tauri::command]
pub fn browser_navigate(app: AppHandle, tab: String, url: String) -> Result<(), String> {
    webview(&app, &tab)?
        .navigate(parse_url(&url)?)
        .map_err(|e| e.to_string())
}

/// `back` | `forward` | `reload`
#[tauri::command]
pub fn browser_nav(app: AppHandle, tab: String, action: String) -> Result<(), String> {
    let v = webview(&app, &tab)?;
    match action.as_str() {
        "reload" => v.reload(),
        "back" => v.eval("history.back()"),
        "forward" => v.eval("history.forward()"),
        other => return Err(format!("unknown action {other}")),
    }
    .map_err(|e| e.to_string())
}

/// Close the tab's webview. Serialized with `browser_open` so a close that follows an
/// in-flight open (⌘W right after opening) always lands after the view exists.
#[tauri::command]
pub async fn browser_close(app: AppHandle, tab: String) -> Result<(), String> {
    let _gate = LIFECYCLE.lock().await;
    REGISTRY.lock().close(&tab);
    match webview(&app, &tab) {
        Ok(v) => v.close().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

/// Evaluate `expr` in the page and return its JSON value (5 s timeout).
pub async fn eval_json(app: &AppHandle, tab: &str, expr: &str) -> Result<Value, String> {
    let v = webview(app, tab)?;
    let (tx, rx) = oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    let js = format!(
        "(() => {{ try {{ return {{ ok: true, v: ({expr}) }}; }} catch (e) {{ return {{ ok: false, e: String(e) }}; }} }})()"
    );
    v.eval_with_callback(js, move |s| {
        if let Some(tx) = tx.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(s);
        }
    })
    .map_err(|e| e.to_string())?;
    let s = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| "page did not answer (still loading?)".to_string())?
        .map_err(|_| "eval dropped".to_string())?;
    let r: Value = serde_json::from_str(&s).map_err(|e| format!("eval result: {e}"))?;
    if r["ok"] == true {
        Ok(r["v"].clone())
    } else {
        Err(r["e"].as_str().unwrap_or("eval failed").to_string())
    }
}

#[tauri::command]
pub async fn browser_eval(app: AppHandle, tab: String, expr: String) -> Result<Value, String> {
    eval_json(&app, &tab, &expr).await
}

/// Design Mode: enable picking, wait until the user clicks an element (or
/// cancels with Esc in the app), return the captured element.
#[tauri::command]
pub async fn browser_pick(app: AppHandle, tab: String) -> Result<Value, String> {
    eval_json(&app, &tab, "window.__atlasGrab.enable(), true").await?;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let r = eval_json(
            &app,
            &tab,
            "[window.__atlasGrab.take(), window.__atlasGrab.active()]",
        )
        .await?;
        if !r[0].is_null() {
            return Ok(r[0].clone());
        }
        if r[1] == false {
            return Err("cancelled".into());
        }
    }
}

#[tauri::command]
pub async fn browser_pick_cancel(app: AppHandle, tab: String) -> Result<(), String> {
    eval_json(&app, &tab, "window.__atlasGrab.disable(), true")
        .await
        .map(|_| ())
}

/// PNG of a region of the app window (logical px relative to the window's
/// content), captured by window id so overlapping windows never leak in.
#[tauri::command]
pub async fn window_region_png(
    app: AppHandle,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<String, String> {
    let win = app.get_window("main").ok_or("no main window")?;
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let id = crate::window_number(&win).ok_or("no window id")?;
    let dir = std::env::temp_dir();
    let full = dir.join(format!("atlas-win-{}.png", std::process::id()));
    let out = dir.join(format!(
        "atlas-grab-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    let ok = tokio::process::Command::new("screencapture")
        .args(["-x", "-o", "-l", &id.to_string()])
        .arg(&full)
        .status()
        .await
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("screencapture failed".into());
    }
    // Title bar height: window frame minus content, in logical px.
    let outer = win.outer_size().map_err(|e| e.to_string())?;
    let inner = win.inner_size().map_err(|e| e.to_string())?;
    let bar = (outer.height.saturating_sub(inner.height)) as f64;
    let px = |v: f64| ((v * scale).round() as i64).max(1).to_string();
    let ok = tokio::process::Command::new("sips")
        .args([
            "-c",
            &px(h),
            &px(w),
            "--cropOffset",
            &((y * scale + bar).round() as i64).to_string(),
        ])
        .arg(((x * scale).round() as i64).to_string())
        .arg(&full)
        .arg("--out")
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?
        .success();
    let _ = std::fs::remove_file(&full);
    if ok {
        Ok(out.to_string_lossy().into_owned())
    } else {
        Err("crop failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANE: Pane = Pane {
        x: 240.0,
        y: 80.0,
        w: 900.0,
        h: 600.0,
    };

    #[test]
    fn only_http_urls() {
        assert!(parse_url("http://localhost:3000/x").is_ok());
        assert!(parse_url("https://example.com").is_ok());
        assert!(parse_url("file:///etc/passwd").is_err());
        assert!(parse_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn label_roundtrips_uuid_tab_ids() {
        let id = "3f2b8c1e-9d4a-4b7e-8a11-0c5d6e7f8a90";
        assert_eq!(tab_of(&label(id)), id);
    }

    #[test]
    fn open_twice_creates_once_then_reuses() {
        let mut r = Registry::default();
        let (plan, at) = r.open("t", false, PANE, true);
        assert_eq!((plan, at), (OpenPlan::Create, Placement::Over(PANE)));
        let moved = Pane { x: 0.0, ..PANE };
        let (plan, at) = r.open("t", true, moved, true);
        assert_eq!((plan, at), (OpenPlan::Reuse, Placement::Over(moved)));
        assert_eq!(r.tabs.len(), 1);
    }

    #[test]
    fn stale_entry_whose_view_is_gone_is_recreated() {
        let mut r = Registry::default();
        r.open("t", false, PANE, true);
        // The native view vanished (window recreated): create again, not reuse.
        let (plan, _) = r.open("t", false, PANE, true);
        assert_eq!(plan, OpenPlan::Create);
        assert_eq!(r.tabs.len(), 1);
    }

    #[test]
    fn background_tab_is_born_parked() {
        let mut r = Registry::default();
        let (_, at) = r.open("t", false, PANE, false);
        assert_eq!(at, Placement::Parked);
        // …but remembers its size so it unparks without a 1×1 flash.
        assert_eq!(r.tabs.get("t").unwrap().last, Some(PANE));
    }

    #[test]
    fn degenerate_rects_park_and_never_overwrite_the_last_good_size() {
        let mut r = Registry::default();
        r.place("t", PANE, true);
        for bad in [
            Pane {
                w: 0.0,
                h: 0.0,
                ..PANE
            },
            Pane {
                w: 1.0,
                h: 1.0,
                ..PANE
            },
            Pane { w: 3.9, ..PANE },
            Pane { h: 3.9, ..PANE },
            Pane {
                x: f64::NAN,
                ..PANE
            },
            Pane {
                w: f64::INFINITY,
                ..PANE
            },
        ] {
            assert_eq!(r.place("t", bad, true), Placement::Parked, "{bad:?}");
            assert_eq!(r.tabs.get("t").unwrap().last, Some(PANE));
        }
        // The next real measurement un-parks.
        assert_eq!(r.place("t", PANE, true), Placement::Over(PANE));
    }

    #[test]
    fn invisible_parks_and_visible_unparks() {
        let mut r = Registry::default();
        assert_eq!(r.place("t", PANE, false), Placement::Parked);
        assert_eq!(r.place("t", PANE, true), Placement::Over(PANE));
    }

    #[test]
    fn park_keeps_size_and_ignores_unknown_tabs() {
        let mut r = Registry::default();
        assert!(r.park("nope").is_none());
        r.place("t", PANE, true);
        let p = r.park("t").unwrap();
        assert_eq!((p.placement, p.last), (Placement::Parked, Some(PANE)));
    }

    #[test]
    fn close_missing_is_a_noop_and_close_forgets() {
        let mut r = Registry::default();
        assert!(!r.close("nope"));
        r.place("t", PANE, true);
        assert!(r.close("t"));
        assert!(r.tabs.is_empty());
        assert!(!r.close("t"));
    }

    #[test]
    fn parked_bounds_sit_offscreen_at_the_last_size() {
        let b = Placement::Parked.bounds(Some(PANE));
        let (p, s): (LogicalPosition<f64>, LogicalSize<f64>) =
            (b.position.to_logical(1.0), b.size.to_logical(1.0));
        assert_eq!((p.x, p.y), (OFFSCREEN, OFFSCREEN));
        assert_eq!((s.width, s.height), (PANE.w, PANE.h));
        let b = Placement::Parked.bounds(None);
        let s: LogicalSize<f64> = b.size.to_logical(1.0);
        assert_eq!((s.width, s.height), (MIN_SIDE, MIN_SIDE));
    }
}
