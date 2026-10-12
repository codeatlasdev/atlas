//! GPU terminal: one native view per tab laid over the pane, drawn by `atlas-term`.
//!
//! Per terminal, as in Ghostty: a reader task (hostd `pty_attach_raw` → `Vt`), a render
//! thread (snapshot → wgpu, woken only by new bytes or a resize), and writes (keys,
//! paste, VT replies) through the hostd connection. Clicks pass through the view to the
//! webview (`hitTest:` → nil), so focus and keyboard stay in the frontend for now.
//! Local hostd only: the raw stream is a dedicated socket connection. macOS only (AppKit view).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use atlas_term::colors::{Palette, Rgb as TermRgb, parse_hex};
use atlas_term::input::{self, Mods};
use atlas_term::{Frame, Renderer, SelectionType, Side, Stats, Vt, VtEvent};
use base64::Engine;
use parking_lot::{Condvar, Mutex};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;

use crate::host::HostHub;

/// `terminal.*` settings for a GPU terminal (defaults when absent).
#[derive(serde::Deserialize)]
#[serde(default)]
pub struct TermPrefs {
    font_size: f32,
    scrollback: usize,
    /// `block` | `bar` | `underline` | `hollow`.
    cursor_shape: String,
    cursor_blink: bool,
    /// OSC 52 (program writes the clipboard): `allow` | `ask` | `deny`.
    osc52: String,
    /// Programming ligatures (calt/liga).
    ligatures: bool,
}

impl Default for TermPrefs {
    fn default() -> Self {
        Self {
            font_size: 12.5,
            scrollback: 10_000,
            cursor_shape: "block".into(),
            cursor_blink: false,
            // Ghostty's default: a program may not overwrite the clipboard unasked.
            osc52: "ask".into(),
            ligatures: true,
        }
    }
}
const OFFSCREEN: f64 = -20_000.0;

struct State {
    /// Pending physical size for the render thread.
    size: Option<(u32, u32)>,
    dirty: bool,
    reset: bool,
    closed: bool,
    stats: Stats,
    /// Cell size in physical pixels, from the renderer.
    cell: [f32; 2],
    /// Keyboard focus is in this terminal's input sink.
    focused: bool,
    /// Visual bell: tint the frame until then.
    bell_until: Option<Instant>,
    /// Theme colors; `term_recolor` swaps them and sets `pal_new` for the render thread.
    pal: Palette,
    pal_new: bool,
    /// Pending font size (pt) for the render thread (`term_set_prefs`).
    font_pt: Option<f32>,
    /// Pending ligature setting for the renderer.
    ligatures: Option<bool>,
    /// Font size (pt) the terminal is at, or will be once `font_pt` is applied.
    font_cur: f32,
}

const BLINK: Duration = Duration::from_millis(530);
const BELL: Duration = Duration::from_millis(120);

struct Shared {
    vt: Vt,
    state: Mutex<State>,
    /// OSC 52 policy (`allow` | `ask` | `deny`), live-updatable.
    osc52: Mutex<String>,
    wake: Condvar,
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.lock());
        self.wake.notify_one();
    }
}

struct Term {
    pty: String,
    /// Retained `NSView*`; touched only on the main thread.
    view: usize,
    shared: Arc<Shared>,
    render: Option<JoinHandle<()>>,
    reader: tauri::async_runtime::JoinHandle<()>,
}

static TERMS: LazyLock<Mutex<HashMap<String, Term>>> = LazyLock::new(Default::default);
/// `NSView*` → tab, to resolve native events to a terminal.
static VIEW_TO_TAB: LazyLock<Mutex<HashMap<usize, String>>> = LazyLock::new(Default::default);
/// Set at first `term_open`; lets non-command paths (VT callbacks) reach the app.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Selection highlight: #264F78 at ~67% opacity.
const SELECTION: [u8; 4] = [38, 79, 120, 170];
/// Search matches (dim amber) and the current match (strong orange), alpha-blended.
const SEARCH: [u8; 4] = [255, 200, 0, 90];
const SEARCH_CURRENT: [u8; 4] = [255, 140, 0, 200];

/// `#rrggbb` fg/bg + optional 16 ANSI overrides (`null` slots keep the default).
fn build_palette(
    fg: &str,
    bg: &str,
    ansi16: Option<&[Option<String>]>,
    min_contrast: Option<f32>,
) -> Result<Palette, String> {
    let ansi: Option<[Option<TermRgb>; 16]> = ansi16.filter(|v| v.len() >= 16).map(|v| {
        let mut a = [None; 16];
        for (slot, s) in a.iter_mut().zip(v) {
            *slot = s.as_deref().and_then(parse_hex);
        }
        a
    });
    let mut pal = Palette::new(
        parse_hex(fg).ok_or("fg: expected #rrggbb")?,
        parse_hex(bg).ok_or("bg: expected #rrggbb")?,
        ansi.as_ref(),
    );
    pal.min_contrast = min_contrast.unwrap_or(0.0);
    Ok(pal)
}

/// Text → the system pasteboard (⌘C, OSC 52).
fn write_clipboard(text: &str) {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    // SAFETY: NSPasteboard general pasteboard is safe off the main thread for plain writes;
    // `NSPasteboardTypeString` is a static NSString constant.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
}

fn write_pty(app: &AppHandle, pty: &str, bytes: &[u8]) {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let args = json!({
        "pty": pty,
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    tauri::async_runtime::spawn(async move {
        let _ = hub.call("pty_write", args).await;
    });
}

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSResponder, NSView, NSWindow};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    define_class!(
        // SAFETY: NSView has no subclassing requirements; no Drop impl.
        #[unsafe(super(NSView, NSResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "AtlasTermView"]
        struct TermView;

        impl TermView {
            /// Transparent to the mouse: clicks reach the webview (focus, selection later).
            #[unsafe(method(hitTest:))]
            fn hit_test(&self, _p: NSPoint) -> *mut NSView {
                std::ptr::null_mut()
            }
        }
    );

    /// Top-left logical rect -> AppKit frame in the content view.
    fn frame(content: &NSView, x: f64, y: f64, w: f64, h: f64) -> NSRect {
        let y = if content.isFlipped() {
            y
        } else {
            content.bounds().size.height - y - h
        };
        NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(1.0), h.max(1.0)))
    }

    /// New view in the window's content view; returns the retained `NSView*`.
    pub fn create(ns_window: usize, x: f64, y: f64, w: f64, h: f64) -> Result<usize, String> {
        let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
        // SAFETY: Tauri's live main NSWindow.
        let win = unsafe { &*(ns_window as *const NSWindow) };
        let content = win.contentView().ok_or("no content view")?;
        // SAFETY: plain NSView designated initializer.
        let view: Retained<TermView> =
            unsafe { msg_send![TermView::alloc(mtm), initWithFrame: frame(&content, x, y, w, h)] };
        view.setWantsLayer(true);
        content.addSubview(&view);
        Ok(Retained::into_raw(view) as usize)
    }

    pub fn set_frame(view: usize, x: f64, y: f64, w: f64, h: f64) {
        // SAFETY: retained in `create`, released only in `destroy`.
        let view = unsafe { &*(view as *const NSView) };
        if let Some(sup) = unsafe { view.superview() } {
            view.setFrame(frame(&sup, x, y, w, h));
        }
    }

    pub fn destroy(view: usize) {
        // SAFETY: balances `Retained::into_raw` in `create`.
        if let Some(view) = unsafe { Retained::from_raw(view as *mut NSView) } {
            view.removeFromSuperview();
        }
    }
}

fn render_loop(mut r: Renderer, sh: Arc<Shared>, on_resize: impl Fn(u16, u16)) {
    let mut pal = sh.state.lock().pal.clone();
    let mut frame = Frame::default();
    let mut blink_at = Instant::now() + BLINK;
    let mut blink_on = true;
    loop {
        let (size, font_pt, ligatures, focused, bell) = {
            let mut s = sh.state.lock();
            loop {
                if s.closed || s.size.is_some() || s.dirty {
                    // New output or input: restart the blink cycle visible.
                    blink_on = true;
                    blink_at = Instant::now() + BLINK;
                    break;
                }
                let blink = frame.cursor_blinks.then_some(blink_at);
                let next = match (blink, s.bell_until) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                let Some(next) = next else {
                    sh.wake.wait(&mut s);
                    continue;
                };
                if !sh.wake.wait_until(&mut s, next).timed_out() {
                    continue;
                }
                let now = Instant::now();
                if s.bell_until.is_some_and(|b| b <= now) {
                    s.bell_until = None;
                }
                if frame.cursor_blinks && blink_at <= now {
                    blink_on = !blink_on;
                    blink_at = now + BLINK;
                }
                break;
            }
            if s.closed {
                return;
            }
            if std::mem::take(&mut s.reset) {
                r.reset_stats();
            }
            s.dirty = false;
            if std::mem::take(&mut s.pal_new) {
                pal = s.pal.clone();
            }
            (
                s.size.take(),
                s.font_pt.take(),
                s.ligatures.take(),
                s.focused,
                s.bell_until.is_some(),
            )
        };
        if let Some(on) = ligatures {
            r.set_ligatures(on);
        }
        // A new font size changes the cell size, so the grid is re-fitted (and the pty
        // told) exactly like a resize.
        let fitted = match (font_pt, size) {
            (Some(pt), size) => {
                let grid = r.set_font_size(pt);
                Some(size.map_or(grid, |(w, h)| r.resize(w, h)))
            }
            (None, Some((w, h))) => Some(r.resize(w, h)),
            (None, None) => None,
        };
        if let Some((cols, rows)) = fitted {
            let [cw, ch] = r.cell_px();
            sh.vt.set_cell_px(cw, ch);
            if sh.vt.size() != (cols, rows) {
                sh.vt.resize(cols, rows);
                on_resize(cols, rows);
            }
        }
        frame.selection_color = SELECTION;
        frame.search_color = SEARCH;
        frame.search_current_color = SEARCH_CURRENT;
        frame.focused = focused;
        frame.blink_on = blink_on;
        sh.vt.snapshot(&pal, &mut frame);
        if bell {
            let tint = |c: [u8; 4]| {
                [
                    c[0].saturating_add(40),
                    c[1].saturating_add(40),
                    c[2].saturating_add(40),
                    c[3],
                ]
            };
            frame.bg.iter_mut().for_each(|c| *c = tint(*c));
            frame.clear = tint(frame.clear);
        }
        if let Err(e) = r.render(&frame) {
            eprintln!("atlas-term render: {e}");
        }
        {
            let mut s = sh.state.lock();
            s.stats = r.stats();
            s.cell = r.cell_px();
        }
    }
}

/// Open (or move) the GPU terminal for `tab` over the pane rect; `fg`/`bg` are `#rrggbb`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn term_open(
    app: AppHandle,
    tab: String,
    pty: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fg: String,
    bg: String,
    ansi16: Option<Vec<Option<String>>>,
    min_contrast: Option<f32>,
    prefs: Option<TermPrefs>,
) -> Result<(), String> {
    if TERMS.lock().contains_key(&tab) {
        return term_bounds(app, tab, x, y, w, h, true).await;
    }
    let raw = atlas_host::client::attach_raw(&atlas_host::paths::base(), &pty, true)
        .await
        .map_err(|e| format!("GPU terminal (local terminals only): {e:#}"))?;
    let win = app.get_window("main").ok_or("no main window")?;
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let ns_win = win.ns_window().map_err(|e| e.to_string())? as usize;
    let pal = build_palette(&fg, &bg, ansi16.as_deref(), min_contrast)?;
    let (cols, rows) = (
        raw.info["cols"].as_u64().unwrap_or(80) as u16,
        raw.info["rows"].as_u64().unwrap_or(24) as u16,
    );
    let prefs = prefs.unwrap_or_default();
    let font_pt = prefs.font_size.clamp(6.0, 48.0);
    let (a, p, t) = (app.clone(), pty.clone(), tab.clone());
    let shared = Arc::new_cyclic(|weak: &std::sync::Weak<Shared>| {
        let weak = weak.clone();
        Shared {
            vt: Vt::new(cols, rows, move |e| match e {
                VtEvent::Write(s) => write_pty(&a, &p, s.as_bytes()),
                VtEvent::ClipboardWrite(text) => {
                    let policy = weak.upgrade().map(|s| s.osc52.lock().clone());
                    match policy.as_deref().unwrap_or("ask") {
                        "deny" => {}
                        "ask" => {
                            let _ = a.emit("term://osc52", json!({"tab": t, "text": text}));
                        }
                        _ => write_clipboard(&text),
                    }
                }
                VtEvent::Bell => {
                    if let Some(sh) = weak.upgrade() {
                        sh.update(|s| s.bell_until = Some(Instant::now() + BELL));
                    }
                    if let Some(win) = a.get_window("main")
                        && !win.is_focused().unwrap_or(true)
                    {
                        let _ = win
                            .request_user_attention(Some(tauri::UserAttentionType::Informational));
                    }
                }
                VtEvent::Cwd(cwd) => {
                    let _ = a.emit("term://cwd", json!({"tab": t, "cwd": cwd}));
                }
                VtEvent::CommandFinished(exit) => {
                    let _ = a.emit("term://command", json!({"tab": t, "exit": exit}));
                }
                // OSC 0/2 titles come from hostd (`pty_title`).
                VtEvent::Title(_) => {}
            }),
            state: Mutex::new(State {
                size: Some(((w * scale) as u32, (h * scale) as u32)),
                pal,
                pal_new: false,
                font_pt: None,
                ligatures: Some(prefs.ligatures),
                font_cur: font_pt,
                dirty: false,
                reset: false,
                closed: false,
                stats: Stats::default(),
                cell: [0.0; 2],
                focused: false,
                bell_until: None,
            }),
            osc52: Mutex::new(prefs.osc52.clone()),
            wake: Condvar::new(),
        }
    });
    shared
        .vt
        .set_options(prefs.scrollback, &prefs.cursor_shape, prefs.cursor_blink);

    // Surface creation touches the NSView/CAMetalLayer: main thread only.
    let (view, renderer) = on_main(&app, move || {
        let view = mac::create(ns_win, x, y, w, h)?;
        let ptr = std::ptr::NonNull::new(view as *mut std::ffi::c_void).ok_or("null view")?;
        let (pw, ph) = ((w * scale) as u32, (h * scale) as u32);
        // SAFETY: the view stays retained until `term_close` joins the render thread.
        match unsafe { Renderer::new(ptr, pw, ph, font_pt, scale as f32) } {
            Ok(r) => Ok((view, r)),
            Err(e) => {
                mac::destroy(view);
                Err(e)
            }
        }
    })?;
    let (sh, a, p) = (shared.clone(), app.clone(), pty.clone());
    let render = thread::Builder::new()
        .name("atlas-term-render".into())
        .spawn(move || {
            render_loop(renderer, sh, |cols, rows| {
                let hub = a.state::<Arc<HostHub>>().inner().clone();
                let args = json!({"pty": p, "cols": cols, "rows": rows});
                tauri::async_runtime::spawn(async move {
                    let _ = hub.call("pty_resize", args).await;
                });
            })
        })
        .map_err(|e| e.to_string())?;
    let sh = shared.clone();
    let reader = tauri::async_runtime::spawn(async move {
        // Bind the whole `RawPty`: precise capture would move only `reader` and drop
        // the write half here, which hostd reads as a hang-up.
        let mut raw = raw;
        let mut buf = vec![0u8; 64 * 1024];
        // EOF = the pty exited (or hostd went away); the last screen stays up.
        // `feed` follows output to the bottom of the scrollback.
        // A synchronized update (mode 2026) that never ends is flushed at its deadline.
        loop {
            let read = raw.reader.read(&mut buf);
            let n = match sh.vt.sync_deadline() {
                Some(d) => match tokio::time::timeout_at(d.into(), read).await {
                    Ok(r) => r,
                    Err(_) => {
                        sh.vt.flush_sync();
                        sh.update(|s| s.dirty = true);
                        continue;
                    }
                },
                None => read.await,
            };
            let Ok(n @ 1..) = n else { break };
            sh.vt.feed(&buf[..n]);
            sh.update(|s| s.dirty = true);
        }
    });
    let _ = APP.set(app.clone());
    VIEW_TO_TAB.lock().insert(view, tab.clone());
    TERMS.lock().insert(
        tab,
        Term {
            pty,
            view,
            shared,
            render: Some(render),
            reader,
        },
    );
    Ok(())
}

/// Move/resize over the pane; `visible: false` parks it offscreen.
#[tauri::command]
pub async fn term_bounds(
    app: AppHandle,
    tab: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    visible: bool,
) -> Result<(), String> {
    let (view, shared) = {
        let t = TERMS.lock();
        let t = t
            .get(&tab)
            .ok_or_else(|| format!("no terminal for tab {tab}"))?;
        (t.view, t.shared.clone())
    };
    let (vx, vy) = if visible {
        (x, y)
    } else {
        (OFFSCREEN, OFFSCREEN)
    };
    on_main(&app, move || {
        mac::set_frame(view, vx, vy, w, h);
        Ok(())
    })?;
    if visible {
        let scale = app
            .get_window("main")
            .and_then(|w| w.scale_factor().ok())
            .unwrap_or(2.0);
        shared.update(|s| s.size = Some(((w * scale) as u32, (h * scale) as u32)));
    }
    Ok(())
}

#[tauri::command]
pub async fn term_close(app: AppHandle, tab: String) -> Result<(), String> {
    let Some(mut t) = TERMS.lock().remove(&tab) else {
        return Ok(());
    };
    t.reader.abort();
    VIEW_TO_TAB.lock().remove(&t.view);
    t.shared.update(|s| s.closed = true);
    // The surface must be gone before its view is released.
    if let Some(r) = t.render.take() {
        let _ = r.join();
    }
    on_main(&app, move || {
        mac::destroy(t.view);
        Ok(())
    })?;
    Ok(())
}

fn with_term(tab: &str) -> Result<(String, Arc<Shared>), String> {
    let t = TERMS.lock();
    let t = t
        .get(tab)
        .ok_or_else(|| format!("no terminal for tab {tab}"))?;
    Ok((t.pty.clone(), t.shared.clone()))
}

/// A key from the webview: `text` for printable input (IME/dead keys already
/// composed), else the DOM `key` name with modifiers. `meta` (⌘) is only sent for the
/// macOS line-editing chords (⌘⌫ ⌘← ⌘→); every other ⌘ chord is an app shortcut.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn term_key(
    app: AppHandle,
    tab: String,
    key: String,
    text: Option<String>,
    shift: bool,
    alt: bool,
    ctrl: bool,
    meta: Option<bool>,
) -> Result<bool, String> {
    let (pty, sh) = with_term(&tab)?;
    // ⇧PgUp / ⇧PgDn page through the scrollback (Terminal.app, iTerm2, xterm); on the
    // alternate screen the program owns the keys and gets `CSI 5;2~` as usual.
    if text.is_none()
        && shift
        && !alt
        && !ctrl
        && let Some(up) = input::shift_page(&key, sh.vt.mode())
    {
        sh.vt.scroll_page(up);
        sh.update(|s| s.dirty = true);
        return Ok(true);
    }
    let bytes = match text {
        Some(t) => input::text(&t, sh.vt.mode()),
        None => {
            let m = Mods {
                shift,
                alt,
                ctrl,
                meta: meta.unwrap_or(false),
            };
            match input::key(&key, m, sh.vt.mode()) {
                Some(b) => b,
                None => return Ok(false),
            }
        }
    };
    write_pty(&app, &pty, &bytes);
    Ok(true)
}

#[tauri::command]
pub fn term_paste(app: AppHandle, tab: String, text: String) -> Result<(), String> {
    let (pty, sh) = with_term(&tab)?;
    write_pty(&app, &pty, &input::paste(&text, sh.vt.mode()));
    Ok(())
}

/// Visible screen text (debug bridge / tests).
#[tauri::command]
pub fn term_text(tab: String) -> Result<String, String> {
    Ok(with_term(&tab)?.1.vt.text())
}

/// Frame timings per open terminal.
#[tauri::command]
pub fn term_stats(app: AppHandle) -> Value {
    let scale = app
        .get_window("main")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(2.0);
    let terms = TERMS.lock();
    json!(
        terms
            .iter()
            .map(|(tab, t)| {
                let (s, cell) = {
                    let st = t.shared.state.lock();
                    (st.stats, st.cell)
                };
                let (cols, rows) = t.shared.vt.size();
                (
                    tab.clone(),
                    json!({
                        "frames": s.frames, "cols": cols, "rows": rows,
                        "cell_w": cell[0] as f64 / scale, "cell_h": cell[1] as f64 / scale,
                        "work_p50_us": s.work_p50_us, "work_p95_us": s.work_p95_us,
                        "work_max_us": s.work_max_us,
                        "interval_p50_us": s.interval_p50_us, "interval_p95_us": s.interval_p95_us,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    )
}

/// Start a fresh measurement window on every open terminal.
#[tauri::command]
pub fn term_stats_reset() {
    for t in TERMS.lock().values() {
        t.shared.update(|s| s.reset = true);
    }
}

/// Scroll the viewport; positive `delta` goes up into history. On the alternate screen
/// (less, vim, htop) there is no scrollback: the wheel becomes arrow keys (Ghostty's
/// mouse-scroll on the alt screen, xterm mode 1007).
#[tauri::command]
pub fn term_scroll(app: AppHandle, tab: String, delta: i32) -> Result<(), String> {
    let (pty, sh) = with_term(&tab)?;
    if let Some(arrows) = input::alt_scroll(delta, sh.vt.mode()) {
        write_pty(&app, &pty, &arrows);
        return Ok(());
    }
    sh.vt.scroll(delta);
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// Where `term_scroll_to` goes (⌘Home / ⌘End / ⌘PgUp / ⌘PgDn, Ghostty's scroll_* actions).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrollTo {
    Top,
    Bottom,
    PageUp,
    PageDown,
}

#[tauri::command]
pub fn term_scroll_to(tab: String, to: ScrollTo) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    match to {
        ScrollTo::Top => sh.vt.scroll_to_top(),
        ScrollTo::Bottom => sh.vt.scroll_to_bottom(),
        ScrollTo::PageUp => sh.vt.scroll_page(true),
        ScrollTo::PageDown => sh.vt.scroll_page(false),
    }
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// ⌘A: select everything (scrollback and screen). The text goes to the clipboard on ⌘C.
#[tauri::command]
pub fn term_select_all(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    sh.vt.select_all();
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// ⌘K (Ghostty `clear_screen`): erase screen and scrollback, then `^L` so the shell redraws
/// its prompt at the top. On the alternate screen the program owns the display: no-op.
#[tauri::command]
pub fn term_clear(app: AppHandle, tab: String) -> Result<bool, String> {
    let (pty, sh) = with_term(&tab)?;
    let cleared = sh.vt.clear_screen();
    if cleared {
        write_pty(&app, &pty, b"\x0c");
        sh.update(|s| s.dirty = true);
    }
    Ok(cleared)
}

/// Click on the prompt line moves the cursor there (Ghostty `cursor-click-to-move`): the
/// delta from the cursor becomes arrow keys. `false` = the click is not on an editable
/// prompt (no OSC 133, a command is running, alternate screen) and is a plain click.
#[tauri::command]
pub fn term_click_move(app: AppHandle, tab: String, col: u16, row: u16) -> Result<bool, String> {
    let (pty, sh) = with_term(&tab)?;
    let Some((dx, dy)) = sh.vt.click_to_move(col as usize, row as i32) else {
        return Ok(false);
    };
    let bytes = input::cursor_moves(dx, dy, sh.vt.mode());
    if bytes.is_empty() {
        return Ok(false);
    }
    write_pty(&app, &pty, &bytes);
    Ok(true)
}

/// Selected text → system clipboard (⌘C). Returns the text.
#[tauri::command]
pub fn term_copy(tab: String) -> Result<Option<String>, String> {
    let text = with_term(&tab)?.1.vt.end_selection();
    if let Some(t) = &text {
        write_clipboard(t);
    }
    Ok(text)
}

/// Which half of the cell a fractional column falls in (selection edge semantics).
fn side(frac: f32) -> Side {
    if frac < 0.5 { Side::Left } else { Side::Right }
}

/// Begin a selection at viewport cell (`col`, `row`); `frac` = x offset within the cell, 0..1.
/// `clicks` 1/2/3 = char/word/line, `block` = rectangular (⌥-drag).
#[tauri::command]
pub fn term_select_start(
    tab: String,
    col: u16,
    row: u16,
    frac: f32,
    clicks: u8,
    block: bool,
) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    let ty = match (clicks, block) {
        (_, true) => SelectionType::Block,
        (2, _) => SelectionType::Semantic,
        (3.., _) => SelectionType::Lines,
        _ => SelectionType::Simple,
    };
    sh.vt
        .start_selection(col as usize, row as i32, ty, side(frac));
    sh.update(|s| s.dirty = true);
    // The view is click-through, but a drag can blur the input sink: hand focus back.
    if let Some(app) = APP.get() {
        let _ = app.emit("term://focus", &tab);
    }
    Ok(())
}

#[tauri::command]
pub fn term_select_update(tab: String, col: u16, row: u16, frac: f32) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    sh.vt.update_selection(col as usize, row as i32, side(frac));
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// Finish a drag: the selection stays highlighted. With `copy` (Ghostty `copy-on-select`,
/// on by default) its text also goes to the clipboard.
#[tauri::command]
pub fn term_select_end(tab: String, copy: Option<bool>) -> Result<Option<String>, String> {
    let text = with_term(&tab)?.1.vt.end_selection();
    if let (Some(t), true) = (&text, copy.unwrap_or(true)) {
        write_clipboard(t);
    }
    Ok(text)
}

#[tauri::command]
pub fn term_clear_selection(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    sh.vt.clear_selection();
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// URL under viewport cell (`col`, `row`) for ⌘-click: an OSC 8 link, else a plain
/// `http(s)://` / `file://` URL in the text.
#[tauri::command]
pub fn term_hyperlink_at(tab: String, col: u16, row: u16) -> Result<Option<String>, String> {
    Ok(with_term(&tab)?.1.vt.url_at(col as usize, row as i32))
}

/// Underline the URL under the pointer (⌘ held); `col`/`row` omitted clears it.
#[tauri::command]
pub fn term_hover(tab: String, col: Option<u16>, row: Option<u16>) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    let at = col.zip(row).map(|(c, r)| (c as usize, r as i32));
    if sh.vt.set_hover(at) {
        sh.update(|s| s.dirty = true);
    }
    Ok(())
}

/// Result of a search step: 1-based `index` of the current match (0 = none) of `total`.
#[derive(serde::Serialize)]
pub struct SearchResult {
    index: usize,
    total: usize,
}

/// Search scrollback + screen: highlights every match, scrolls to the next (`forward`) /
/// previous one (wrapping). A changed query/flags restarts at the top of the viewport.
#[tauri::command]
pub fn term_search(
    tab: String,
    query: String,
    regex: bool,
    case: bool,
    forward: bool,
) -> Result<SearchResult, String> {
    let sh = with_term(&tab)?.1;
    let r = sh.vt.search(&query, regex, case, forward);
    sh.update(|s| s.dirty = true);
    r.map(|s| SearchResult {
        index: s.index,
        total: s.total,
    })
}

#[tauri::command]
pub fn term_search_clear(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?.1;
    sh.vt.clear_search();
    sh.update(|s| s.dirty = true);
    Ok(())
}

/// The terminal of `tab`, or every open one when omitted.
fn targets(tab: Option<&str>) -> Result<Vec<Arc<Shared>>, String> {
    let terms = TERMS.lock();
    match tab {
        Some(t) => terms
            .get(t)
            .map(|t| vec![t.shared.clone()])
            .ok_or_else(|| format!("no terminal for tab {t}")),
        None => Ok(terms.values().map(|t| t.shared.clone()).collect()),
    }
}

/// Live theme update for `tab`, or every open terminal when omitted.
#[tauri::command]
pub fn term_recolor(
    tab: Option<String>,
    fg: String,
    bg: String,
    ansi16: Option<Vec<Option<String>>>,
    min_contrast: Option<f32>,
) -> Result<(), String> {
    let pal = build_palette(&fg, &bg, ansi16.as_deref(), min_contrast)?;
    for sh in targets(tab.as_deref())? {
        let pal = pal.clone();
        sh.update(|s| {
            s.pal = pal;
            s.pal_new = true;
            s.dirty = true;
        });
    }
    Ok(())
}

/// Live `terminal.*` settings for `tab`, or every open terminal when omitted:
/// scrollback, cursor, OSC 52 and font size apply at once (a size change re-fits the grid).
#[tauri::command]
pub fn term_set_prefs(tab: Option<String>, prefs: TermPrefs) -> Result<(), String> {
    let pt = prefs.font_size.clamp(6.0, 48.0);
    for sh in targets(tab.as_deref())? {
        sh.vt
            .set_options(prefs.scrollback, &prefs.cursor_shape, prefs.cursor_blink);
        *sh.osc52.lock() = prefs.osc52.clone();
        sh.update(|s| {
            if s.font_cur != pt {
                s.font_cur = pt;
                s.font_pt = Some(pt);
            }
            s.ligatures = Some(prefs.ligatures);
            s.dirty = true;
        });
    }
    Ok(())
}

/// ⌘+ / ⌘− / ⌘0 (Ghostty `increase_font_size` / `decrease_font_size` / `reset_font_size`):
/// this terminal only, live. `size` is the new absolute size in pt (the frontend steps it
/// from the size it last applied; reset passes the configured one). Returns the applied size.
#[tauri::command]
pub fn term_font(tab: String, size: f32) -> Result<f32, String> {
    let pt = size.clamp(6.0, 48.0);
    with_term(&tab)?.1.update(|s| {
        if s.font_cur != pt {
            s.font_cur = pt;
            s.font_pt = Some(pt);
        }
        s.dirty = true;
    });
    Ok(pt)
}

/// Input sink focus: unfocused terminals draw a hollow, non-blinking cursor.
#[tauri::command]
pub fn term_focus(tab: String, focused: bool) -> Result<(), String> {
    with_term(&tab)?.1.update(|s| {
        s.focused = focused;
        s.dirty = true;
    });
    Ok(())
}

/// Scroll to the previous (`up`) / next shell prompt (OSC 133). `false` = no prompt there.
#[tauri::command]
pub fn term_jump_prompt(tab: String, up: bool) -> Result<bool, String> {
    let sh = with_term(&tab)?.1;
    let moved = sh.vt.jump_to_prompt(up);
    sh.update(|s| s.dirty = true);
    Ok(moved)
}
