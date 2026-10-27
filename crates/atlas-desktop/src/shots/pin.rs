//! Pinned screenshots: a capture floating above every window, borderless, on every Space, that you can drag, zoom,
//! fade and close. Each pin is its own window (`shots-pin-<n>`, [`Surface::Pin`]), built hidden through
//! `surface::build` and ordered in through `surface::show`, so it is a non-activating panel (clicking it never takes
//! the keyboard from the app you are working in) and an agent instance shows it only through the guarded virtual
//! display path like every other capture window.
//!
//! The page (`routes/shots/pin`) owns zoom, opacity and the keys (`lib/shots/pin.ts`, pure and tested); this file
//! owns the windows. Zooming resizes the window around its centre, which only Rust may do (`shots_pin_resize`), and
//! the page's `path` argument is untrusted: only a screenshot of the capture history can be pinned.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use super::geom::Rect;
use super::naming::Kind;
use super::quick::known;
use super::surface::{self, Surface};
use super::{Shots, editor};

/// Pin numbers are never reused, so a label always names one window.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// Smallest side a pin window may be resized to (the page's 25% floor on a small image still leaves a handle).
pub const MIN_SIDE: f64 = 48.0;

/// The size a zoom request becomes: both sides clamped to `[MIN_SIDE, display]`, the aspect left to the page (it
/// sends sizes of one aspect; a degenerate request must not make an unusable window or one bigger than the display).
pub fn clamp_size(want: (f64, f64), display: Rect) -> (f64, f64) {
    let fix = |v: f64, max: f64| {
        if v.is_finite() {
            v.clamp(MIN_SIDE.min(max), max.max(1.0))
        } else {
            MIN_SIDE.min(max)
        }
    };
    (fix(want.0, display.w), fix(want.1, display.h))
}

/// The top-left that keeps the window's centre where it was when its size changes from `old` to `new`, moved back
/// onto `display` when growing would push it off.
pub fn recentre(at: (f64, f64), old: (f64, f64), new: (f64, f64), display: Rect) -> (f64, f64) {
    let cx = at.0 + old.0 / 2.0;
    let cy = at.1 + old.1 / 2.0;
    (
        (cx - new.0 / 2.0)
            .min(display.right() - new.0)
            .max(display.x),
        (cy - new.1 / 2.0)
            .min(display.bottom() - new.1)
            .max(display.y),
    )
}

/// The query of a pin's page: the capture's path (percent-encoded) and the pin's number.
pub fn query(path: &Path, n: u32) -> String {
    let mut enc = String::new();
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    format!("path={enc}&n={n}")
}

/// What the page loads: the picture as a data URL with its pixel size (same shape as the editor's document).
pub type PinDoc = editor::EditorDoc;

/// The display the pointer is on, or the first one: its frame (global points) and backing scale.
#[cfg(target_os = "macos")]
fn display_under_pointer() -> Result<(Rect, f64), String> {
    let cur = super::runner::cursor();
    let content = super::sck::Content::fetch()?;
    content
        .displays
        .iter()
        .find(|d| d.frame.contains(cur.x, cur.y))
        .or(content.displays.first())
        .map(|d| (d.frame, d.scale.max(1.0)))
        .ok_or_else(|| "No display found.".to_string())
}

/// Open a pin of the screenshot at `path` (a capture of the history).
pub fn open(app: &AppHandle, state: &Shots, path: &Path) -> Result<(), String> {
    let entry = known(state, path)?;
    if entry.kind != Kind::Screenshot {
        return Err("Only screenshots can be pinned.".into());
    }
    // Placement first: a display lookup that fails must not leave a hidden window behind.
    #[cfg(target_os = "macos")]
    let (display, scale) = display_under_pointer()?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let win = surface::build(app, Surface::Pin, n, &query(path, n))?;
    #[cfg(target_os = "macos")]
    {
        // 100% is the image's size in points: a retina capture's pixels over the display's scale.
        let image = (
            f64::from(entry.width) / scale,
            f64::from(entry.height) / scale,
        );
        let size = surface::pin_fit(image, display);
        let (x, y) = surface::pin_origin(display, size, n - 1);
        surface::place(app, &win, Rect::new(x, y, size.0, size.1));
    }
    surface::show(app, &win);
    Ok(())
}

/// The calling window of a pin command, accepted only when it is a pin (`shots-pin-<n>`). Every command here acts on
/// the *caller* (`set_size`, `destroy`, key status), so the card, the editor or the main window calling one must be
/// refused before the command body runs: taking this type instead of a plain `WebviewWindow` is that wall, and a
/// command cannot forget it.
pub struct PinWindow<R: Runtime = tauri::Wry>(WebviewWindow<R>);

impl<R: Runtime> std::ops::Deref for PinWindow<R> {
    type Target = WebviewWindow<R>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de, R: Runtime> CommandArg<'de, R> for PinWindow<R> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let window = WebviewWindow::<R>::from_command(command)?;
        if surface::is_pin_label(window.label()) {
            Ok(Self(window))
        } else {
            Err(InvokeError::from("only a pin window can call this"))
        }
    }
}

/// The picture a pin page shows.
#[tauri::command]
pub fn shots_pin_load(
    _pin: PinWindow,
    state: State<'_, Shots>,
    path: String,
) -> Result<PinDoc, String> {
    let path = PathBuf::from(path);
    known(&state, &path)?;
    editor::load(&path)
}

/// Resize the calling pin to `width` × `height` points around its centre (zoom) and answer the size it really got
/// (clamped to a handle and to the display), which the page shows as its zoom. Only a pin window may call it.
#[tauri::command]
pub fn shots_pin_resize(
    app: AppHandle,
    window: PinWindow,
    width: f64,
    height: f64,
) -> Result<(f64, f64), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let at = window.outer_position().map_err(|e| e.to_string())?;
    let old = window.outer_size().map_err(|e| e.to_string())?;
    let at = (f64::from(at.x) / scale, f64::from(at.y) / scale);
    let old = (f64::from(old.width) / scale, f64::from(old.height) / scale);
    let display = match window.current_monitor() {
        Ok(Some(m)) => {
            let p = m.position();
            let s = m.size();
            let k = m.scale_factor();
            Rect::new(
                f64::from(p.x) / k,
                f64::from(p.y) / k,
                f64::from(s.width) / k,
                f64::from(s.height) / k,
            )
        }
        _ => return Err("The pin is not on a display.".into()),
    };
    let new = clamp_size((width, height), display);
    let (x, y) = recentre(at, old, new, display);
    surface::place(&app, &window, Rect::new(x, y, new.0, new.1));
    Ok(new)
}

/// Give the calling pin the keyboard (`on`) or take it back. A pin is a non-activating panel: it can only receive
/// Esc, +, -, [ and ] while it is the key window, which the page asks for when it is clicked and gives up when it
/// loses focus. The grant goes through `dock_mac::overlay_keys`, the one place that refuses in an agent instance.
#[tauri::command]
pub fn shots_pin_keys(app: AppHandle, window: PinWindow, on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (a, label) = (app.clone(), window.label().to_string());
        app.run_on_main_thread(move || {
            let win = if on { a.get_window(&label) } else { None };
            crate::dock_mac::overlay_keys(win.as_ref());
        })
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, window, on);
    Ok(())
}

/// Close the calling pin and give the keyboard back if it held it.
#[tauri::command]
pub fn shots_pin_close(app: AppHandle, window: PinWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(|| crate::dock_mac::overlay_keys(None));
    if let Some(w) = app.get_webview_window(window.label()) {
        w.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);

    /// The page asks for sizes from its zoom; a window must never become unusable (a sliver) or larger than the display.
    #[test]
    fn a_zoom_size_is_kept_between_a_handle_and_the_display() {
        assert_eq!(clamp_size((400.0, 300.0), D), (400.0, 300.0));
        assert_eq!(clamp_size((1.0, 2.0), D), (MIN_SIDE, MIN_SIDE));
        assert_eq!(clamp_size((9000.0, 9000.0), D), (1512.0, 982.0));
        assert_eq!(clamp_size((f64::NAN, f64::INFINITY), D).0, MIN_SIDE);
        assert_eq!(clamp_size((-5.0, 0.0), D), (MIN_SIDE, MIN_SIDE));
        // a display smaller than the floor: the display wins
        let tiny = Rect::new(0.0, 0.0, 30.0, 30.0);
        assert_eq!(clamp_size((500.0, 500.0), tiny), (30.0, 30.0));
        assert_eq!(clamp_size((1.0, 1.0), tiny), (30.0, 30.0));
    }

    /// Zoom grows the window around its centre; at an edge it slides back instead of leaving the display.
    #[test]
    fn zoom_keeps_the_centre_and_stays_on_the_display() {
        // centre (300, 250) kept
        assert_eq!(
            recentre((200.0, 200.0), (200.0, 100.0), (400.0, 200.0), D),
            (100.0, 150.0)
        );
        // shrinking keeps the centre too
        assert_eq!(
            recentre((100.0, 150.0), (400.0, 200.0), (200.0, 100.0), D),
            (200.0, 200.0)
        );
        // growing at the top-left corner would go negative: pinned to the display's origin
        assert_eq!(
            recentre((0.0, 0.0), (100.0, 100.0), (300.0, 300.0), D),
            (0.0, 0.0)
        );
        // growing at the bottom-right corner slides back in
        assert_eq!(
            recentre((1412.0, 882.0), (100.0, 100.0), (300.0, 300.0), D),
            (1212.0, 682.0)
        );
        // a second display with a negative origin
        let up = Rect::new(0.0, -1080.0, 1920.0, 1080.0);
        assert_eq!(
            recentre((10.0, -1070.0), (100.0, 100.0), (400.0, 400.0), up),
            (0.0, -1080.0)
        );
    }

    /// The page reads `path` and `n` from its query; the path is percent-encoded (spaces, `&`, `#`, non-ASCII).
    #[test]
    fn the_query_round_trips_a_path_with_spaces_and_reserved_characters() {
        let p = Path::new("/Users/me/Atlas Captures/a&b #1 é.png");
        let q = query(p, 7);
        assert!(q.ends_with("&n=7"), "{q}");
        let enc = q.strip_prefix("path=").unwrap().split('&').next().unwrap();
        assert!(!enc.contains(' ') && !enc.contains('#') && !enc.contains('&'));
        assert_eq!(
            enc,
            "%2FUsers%2Fme%2FAtlas%20Captures%2Fa%26b%20%231%20%C3%A9.png"
        );
    }

    /// A command that takes a [`PinWindow`], registered on tauri's mock runtime and invoked through the real IPC
    /// pipeline, the way each `shots_pin_*` command receives its caller.
    #[tauri::command]
    fn probe<R: Runtime>(_pin: PinWindow<R>) -> &'static str {
        "ran"
    }

    fn call_from(label: &str) -> Result<String, serde_json::Value> {
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![probe])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let caller = tauri::WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap();
        tauri::test::get_ipc_response(
            &caller,
            tauri::webview::InvokeRequest {
                cmd: "probe".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                // the page's own origin: tauri://localhost, or http://tauri.localhost on Windows
                url: if cfg!(windows) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map(|b| b.deserialize::<String>().unwrap())
    }

    /// Every pin command acts on its caller (`set_size`, `destroy`, key status), so a window that is not a pin must be
    /// refused before the body runs: the card, the editor, the main window, the overlay, a look-alike label.
    #[test]
    fn a_pin_command_refuses_every_caller_that_is_not_a_pin() {
        assert_eq!(call_from("shots-pin-1").as_deref(), Ok("ran"));
        assert_eq!(call_from("shots-pin-42").as_deref(), Ok("ran"));
        for label in [
            "main",
            "shots-quick",
            "shots-editor",
            "shots-hud",
            "shots-overlay-1",
            "shots-pin-",
            "shots-pin-x",
            "shots-pin-1x",
            "shots-pinned-1",
        ] {
            assert!(call_from(label).is_err(), "{label} must be refused");
        }
    }

    /// Pin numbers are unique and never reused: two pins of the same file are two windows.
    #[test]
    fn every_open_gets_a_fresh_number() {
        let a = NEXT.fetch_add(1, Ordering::Relaxed);
        let b = NEXT.fetch_add(1, Ordering::Relaxed);
        assert_ne!(a, b);
        assert_ne!(Surface::Pin.label(a), Surface::Pin.label(b));
    }
}
