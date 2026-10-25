//! Opening, moving and closing a terminal: the AppKit view, the render thread and the
//! reader task that feeds the `Vt` from hostd.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use atlas_term::colors::{Palette, Rgb as TermRgb, parse_hex};
use atlas_term::{Renderer, Stats, Vt, VtEvent};
use parking_lot::{Condvar, Mutex};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;

use super::mac;
use super::render_loop::{Hooks, render_loop};
use super::shared::{BELL, Placement, Shared, State};
use super::watchdog::Why;
use super::{APP, TERMS, Term, TermPrefs, on_main, pty_writer, with_term, write_clipboard};
use crate::host::HostHub;

/// Where a hidden terminal's view is parked.
const OFFSCREEN: f64 = -20_000.0;

/// Optional `#rrggbb` theme colors beyond fg/bg (Ghostty `cursor-color`, `cursor-text`,
/// `selection-background`, `selection-foreground`). Empty strings count as unset.
#[derive(Default)]
pub(super) struct ExtraColors {
    pub(super) cursor: Option<String>,
    pub(super) cursor_text: Option<String>,
    pub(super) selection_bg: Option<String>,
    pub(super) selection_fg: Option<String>,
}

fn opt_hex(name: &str, v: Option<&str>) -> Result<Option<TermRgb>, String> {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => parse_hex(s)
            .map(Some)
            .ok_or_else(|| format!("{name}: expected #rrggbb")),
    }
}

/// `#rrggbb` fg/bg + optional 16 ANSI overrides (`null` slots keep the default) and the
/// optional cursor/selection colors.
pub(super) fn build_palette(
    fg: &str,
    bg: &str,
    ansi16: Option<&[Option<String>]>,
    min_contrast: Option<f32>,
    extra: &ExtraColors,
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
    )
    .with_cursor(opt_hex("cursor", extra.cursor.as_deref())?);
    pal.cursor_text = opt_hex("cursor_text", extra.cursor_text.as_deref())?;
    pal.selection_bg = opt_hex("selection_bg", extra.selection_bg.as_deref())?;
    pal.selection_fg = opt_hex("selection_fg", extra.selection_fg.as_deref())?;
    pal.min_contrast = min_contrast.unwrap_or(0.0);
    Ok(pal)
}

/// `term://font` `{tab, requested, resolved}`: only when a requested family could not be
/// used as asked (none installed, or a later candidate won), once per font change.
pub(super) fn emit_font_event(
    app: &AppHandle,
    tab: &str,
    spec: &atlas_term::FontSpec,
    res: &atlas_term::FontResolution,
) {
    if spec.families.is_empty() || res.matched == Some(0) {
        return;
    }
    let _ = app.emit(
        "term://font",
        json!({"tab": tab, "requested": spec.families, "resolved": res.family}),
    );
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
    cursor: Option<String>,
    cursor_text: Option<String>,
    selection_bg: Option<String>,
    selection_fg: Option<String>,
) -> Result<(), String> {
    // A terminal whose render thread died (a panic in wgpu, say) keeps its registry entry but
    // never draws again: drop it and open a fresh one (the pty and its output live in hostd).
    if dropped(&tab) {
        let _ = term_close(app.clone(), tab.clone()).await;
    }
    if TERMS.lock().contains_key(&tab) {
        return term_bounds(app, tab, x, y, w, h, true).await;
    }
    let raw = atlas_host::client::attach_raw(&atlas_host::paths::base(), &pty, true)
        .await
        .map_err(|e| format!("GPU terminal (local terminals only): {e:#}"))?;
    let win = app.get_window("main").ok_or("no main window")?;
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let ns_win = win.ns_window().map_err(|e| e.to_string())? as usize;
    let extra = ExtraColors {
        cursor,
        cursor_text,
        selection_bg,
        selection_fg,
    };
    let pal = build_palette(&fg, &bg, ansi16.as_deref(), min_contrast, &extra)?;
    let (cols, rows) = (
        raw.info["cols"].as_u64().unwrap_or(80) as u16,
        raw.info["rows"].as_u64().unwrap_or(24) as u16,
    );
    let prefs = prefs.unwrap_or_default();
    let font_pt = prefs.font_size.clamp(6.0, 48.0);
    let spec = prefs.font_spec();
    let (a, t) = (app.clone(), tab.clone());
    let (writer_tx, writer) = pty_writer(&app, &pty);
    let reply = writer_tx.clone();
    let shared = Arc::new_cyclic(|weak: &std::sync::Weak<Shared>| {
        let weak = weak.clone();
        Shared {
            vt: Vt::new(cols, rows, move |e| match e {
                VtEvent::Write(s) => {
                    let _ = reply.send(s.into_bytes());
                }
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
                    // Off the VT lock: AppKit calls must not run under `feed`.
                    let a = a.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(win) = a.get_window("main")
                            && crate::agent_mode::Mode::current().focus_allowed()
                            && !win.is_focused().unwrap_or(true)
                        {
                            let _ = win.request_user_attention(Some(
                                tauri::UserAttentionType::Informational,
                            ));
                        }
                    });
                }
                VtEvent::Cwd(cwd) => {
                    let _ = a.emit("term://cwd", json!({"tab": t, "cwd": cwd}));
                }
                VtEvent::CommandFinished(exit) => {
                    let _ = a.emit("term://command", json!({"tab": t, "exit": exit}));
                }
                VtEvent::Notify { title, body } => {
                    let _ = a.emit(
                        "term://notify",
                        json!({"tab": t, "title": title, "body": body}),
                    );
                }
                // OSC 0/2 titles come from hostd (`pty_title`).
                VtEvent::Title(_) => {}
            }),
            state: Mutex::new(State {
                size: Some(((w * scale) as u32, (h * scale) as u32)),
                pal: pal.clone(),
                pal_new: false,
                font_pt: None,
                ligatures: Some(prefs.ligatures),
                font_cur: font_pt,
                font_new: None,
                font_spec: spec.clone(),
                dirty: false,
                rebuild: None,
                reset: false,
                closed: false,
                stats: Stats::default(),
                cell: [0.0; 2],
                focused: false,
                bell_until: None,
            }),
            osc52: Mutex::new(prefs.osc52.clone()),
            wake: Condvar::new(),
            placement: Mutex::new(Placement::default()),
            view: std::sync::atomic::AtomicUsize::new(0),
            writer: writer_tx,
        }
    });
    shared
        .vt
        .set_options(prefs.scrollback, &prefs.cursor_shape, prefs.cursor_blink);
    shared.vt.set_palette(&pal);

    // Surface creation touches the NSView/CAMetalLayer: main thread only.
    let spec_open = spec.clone();
    let (view, renderer, resolution) = on_main(&app, move || {
        // Parked: it is moved over the pane when the first frame is drawn (`Placement`).
        new_surface(
            ns_win,
            (w, h),
            ((w * scale) as u32, (h * scale) as u32),
            font_pt,
            scale as f32,
            spec_open,
        )
    })?;
    shared.set_view(view);
    emit_font_event(&app, &tab, &spec, &resolution);
    shared.placement.lock().request((x, y, w, h), true);
    let render = spawn_render(&app, &tab, &pty, &shared, renderer, ns_win, scale)?;
    let reader = spawn_reader(shared.clone(), raw);
    let _ = APP.set(app.clone());
    TERMS.lock().insert(
        tab,
        Term {
            pty,
            shared,
            render: Some(render),
            reader,
            writer,
        },
    );
    Ok(())
}

/// A parked native view and a `Renderer` drawing into it. Main thread only. `logical` is the
/// view's size in points, `pixels` the surface's physical size.
fn new_surface(
    ns_win: usize,
    logical: (f64, f64),
    pixels: (u32, u32),
    font_pt: f32,
    scale: f32,
    spec: atlas_term::FontSpec,
) -> Result<(usize, Renderer, atlas_term::FontResolution), String> {
    let view = mac::create(ns_win, OFFSCREEN, OFFSCREEN, logical.0, logical.1)?;
    let ptr = std::ptr::NonNull::new(view as *mut std::ffi::c_void).ok_or("null view")?;
    // SAFETY: the view stays retained until its renderer is dropped (`term_close` joins the
    // render thread first; a rebuild drops the old renderer before destroying the old view).
    match unsafe { Renderer::new(ptr, pixels.0, pixels.1, font_pt, scale, spec) } {
        Ok((r, res)) => Ok((view, r, res)),
        Err(e) => {
            mac::destroy(view);
            Err(e)
        }
    }
}

/// The render thread of `tab`: draws `renderer` and, when asked or stalled, replaces its surface.
fn spawn_render(
    app: &AppHandle,
    tab: &str,
    pty: &str,
    shared: &Arc<Shared>,
    renderer: Renderer,
    ns_win: usize,
    scale: f64,
) -> Result<thread::JoinHandle<()>, String> {
    let (sh, a, p, tab) = (shared.clone(), app.clone(), pty.to_string(), tab.to_string());
    thread::Builder::new()
        .name("atlas-term-render".into())
        .spawn(move || {
            let (resize_app, font_app, first_app, rebuild_app) =
                (a.clone(), a.clone(), a.clone(), a);
            let (font_tab, incident_tab) = (tab.clone(), tab);
            let (first_sh, rebuild_sh) = (sh.clone(), sh.clone());
            render_loop(
                renderer,
                sh,
                Hooks {
                    on_resize: Box::new(move |cols, rows| {
                        let hub = resize_app.state::<Arc<HostHub>>().inner().clone();
                        let args = json!({"pty": p, "cols": cols, "rows": rows});
                        tauri::async_runtime::spawn(async move {
                            let _ = hub.call("pty_resize", args).await;
                        });
                    }),
                    on_font: Box::new(move |spec, res| {
                        emit_font_event(&font_app, &font_tab, spec, res)
                    }),
                    on_first_frame: Box::new(move || {
                        let view = first_sh.view();
                        let frame = first_sh.placement.lock().drawn();
                        if let Some((x, y, w, h)) = frame {
                            let _ = on_main(&first_app, move || {
                                mac::set_frame(view, x, y, w, h);
                                Ok(())
                            });
                        }
                    }),
                    rebuild: Box::new(move |old: Renderer, pixels, _why| {
                        rebuild_surface(&rebuild_app, &rebuild_sh, ns_win, scale, old, pixels)
                    }),
                    incident: Box::new(move |kind, detail| {
                        crate::diag::incident(kind, &format!("tab {incident_tab}: {detail}"))
                    }),
                },
            )
        })
        .map_err(|e| e.to_string())
}

/// Replace `old`'s view and surface with new ones at `pixels` (the screen and scrollback live in
/// the `Vt` and are untouched). Runs on the render thread; the AppKit work hops to the main thread.
/// The new view is parked until its first frame, like a new terminal's (`Placement`). `Err(old)`:
/// the new surface could not be made, the old one stays.
fn rebuild_surface(
    app: &AppHandle,
    sh: &Arc<Shared>,
    ns_win: usize,
    scale: f64,
    old: Renderer,
    pixels: (u32, u32),
) -> Result<Renderer, Renderer> {
    let (font_pt, spec) = {
        let s = sh.state.lock();
        (s.font_cur, s.font_spec.clone())
    };
    let ligatures = old.ligatures();
    let logical = (pixels.0 as f64 / scale, pixels.1 as f64 / scale);
    let made = on_main(app, move || {
        new_surface(ns_win, logical, pixels, font_pt, scale as f32, spec)
    });
    let (view, mut renderer, _) = match made {
        Ok(v) => v,
        Err(e) => {
            crate::diag::incident("terminal-rebuild-failed", &e);
            return Err(old);
        }
    };
    renderer.set_ligatures(ligatures);
    // Parked again: `on_first_frame` shows the new view over the pane once it has drawn.
    sh.placement.lock().parked();
    let old_view = sh.set_view(view);
    // The old surface must be gone before its view is released.
    drop(old);
    let _ = on_main(app, move || {
        mac::destroy(old_view);
        Ok(())
    });
    Ok(renderer)
}

/// What `term_bounds` answers for a terminal whose view is gone (its render thread ended without
/// `term_close`); the frontend re-runs `term_open` on it (`nativeView.ts`).
pub const VIEW_DROPPED: &str = "terminal view dropped";

/// Has `tab`'s render thread ended while the tab is still open?
fn dropped(tab: &str) -> bool {
    TERMS
        .lock()
        .get(tab)
        .is_some_and(|t| t.render.as_ref().is_some_and(|r| r.is_finished()))
}

/// Move/resize over the pane; `visible: false` parks it offscreen. Errors with [`VIEW_DROPPED`]
/// when the view is gone, so the frontend opens it again.
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
    if dropped(&tab) {
        crate::diag::incident("terminal-dropped", &format!("tab {tab}: render thread ended"));
        return Err(VIEW_DROPPED.into());
    }
    let shared = with_term(&tab)?;
    // Before the first frame is drawn the view stays parked; the rect is applied when it is (`Placement`).
    let (vx, vy) = match shared.placement.lock().request((x, y, w, h), visible) {
        Some((x, y, _, _)) => (x, y),
        None => (OFFSCREEN, OFFSCREEN),
    };
    // Read after the placement is recorded: a rebuild swaps the view, and one that lands between
    // here and the move still shows at the right place (`Placement` is what its first frame uses).
    let view = shared.view();
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

/// "Reload terminal": replace `tab`'s surface and renderer by hand, as the watchdog does when a tab
/// stalls. The pty, its screen and its scrollback are untouched (nothing is reattached or
/// replayed). Rebuilds on the tab's render thread, so a stuck renderer does not block this call.
#[tauri::command]
pub fn term_reload(tab: String) -> Result<(), String> {
    let sh = with_term(&tab)?;
    sh.rebuild(Why::Manual);
    Ok(())
}

#[tauri::command]
pub async fn term_close(app: AppHandle, tab: String) -> Result<(), String> {
    let Some(mut t) = TERMS.lock().remove(&tab) else {
        return Ok(());
    };
    t.reader.abort();
    t.writer.abort();
    t.shared.update(|s| s.closed = true);
    // The surface must be gone before its view is released.
    if let Some(r) = t.render.take() {
        let _ = r.join();
    }
    let view = t.shared.view();
    on_main(&app, move || {
        mac::destroy(view);
        Ok(())
    })?;
    Ok(())
}

/// Feed a raw pty stream into the terminal until EOF (the pty exited or hostd went away;
/// the last screen stays up, and `super::reattach_all` resumes after a hostd restart).
/// A synchronized update (mode 2026) that never ends is flushed at its deadline.
pub(super) fn spawn_reader(
    sh: Arc<Shared>,
    raw: atlas_host::client::RawPty,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        // Bind the whole `RawPty`: precise capture would move only `reader` and drop
        // the write half here, which hostd reads as a hang-up.
        let mut raw = raw;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let read = raw.reader.read(&mut buf);
            let n = match sh.vt.sync_deadline() {
                Some(d) => match tokio::time::timeout_at(d.into(), read).await {
                    Ok(r) => r,
                    Err(_) => {
                        sh.vt.flush_sync();
                        sh.redraw();
                        continue;
                    }
                },
                None => read.await,
            };
            let Ok(n @ 1..) = n else { break };
            sh.vt.feed(&buf[..n]);
            sh.redraw();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extra(c: Option<&str>, ct: Option<&str>, sb: Option<&str>, sf: Option<&str>) -> ExtraColors {
        let o = |s: Option<&str>| s.map(String::from);
        ExtraColors {
            cursor: o(c),
            cursor_text: o(ct),
            selection_bg: o(sb),
            selection_fg: o(sf),
        }
    }

    #[test]
    fn unset_extra_colors_keep_todays_palette() {
        let p = build_palette("#c8c8c8", "#0a0a0a", None, None, &ExtraColors::default()).unwrap();
        assert_eq!(p.cursor(&Default::default()), parse_hex("#c8c8c8").unwrap());
        assert_eq!(
            (p.cursor_text, p.selection_bg, p.selection_fg),
            (None, None, None)
        );
        // Empty strings (a frontend sending "" for unset) count as unset.
        let p = build_palette(
            "#c8c8c8",
            "#0a0a0a",
            None,
            None,
            &extra(Some(""), Some(" "), Some(""), None),
        )
        .unwrap();
        assert_eq!((p.cursor_text, p.selection_bg), (None, None));
    }

    #[test]
    fn extra_colors_are_applied() {
        let p = build_palette(
            "#c8c8c8",
            "#0a0a0a",
            None,
            Some(4.5),
            &extra(
                Some("#ff0000"),
                Some("#00ff00"),
                Some("#0000ff"),
                Some("#ffffff"),
            ),
        )
        .unwrap();
        assert_eq!(p.cursor(&Default::default()), parse_hex("#ff0000").unwrap());
        assert_eq!(p.cursor_text, parse_hex("#00ff00"));
        assert_eq!(p.selection_bg, parse_hex("#0000ff"));
        assert_eq!(p.selection_fg, parse_hex("#ffffff"));
        assert_eq!(p.min_contrast, 4.5);
    }

    #[test]
    fn invalid_extra_color_names_its_argument() {
        let err = |e: ExtraColors| {
            build_palette("#fff000", "#000000", None, None, &e)
                .err()
                .unwrap()
        };
        assert!(err(extra(Some("red"), None, None, None)).starts_with("cursor:"));
        assert!(err(extra(None, Some("#12"), None, None)).starts_with("cursor_text:"));
        assert!(err(extra(None, None, Some("nope"), None)).starts_with("selection_bg:"));
        assert!(err(extra(None, None, None, Some("#12345g"))).starts_with("selection_fg:"));
        assert!(
            build_palette("zzz", "#000000", None, None, &ExtraColors::default())
                .err()
                .unwrap()
                .starts_with("fg:")
        );
    }

    #[test]
    fn prefs_font_request_comes_from_json() {
        let prefs: TermPrefs = serde_json::from_str(
            r#"{"font_family":[" Fira Code ","","Menlo"],"font_thicken":true,"font_size":14}"#,
        )
        .unwrap();
        let spec = prefs.font_spec();
        assert_eq!(spec.families, ["Fira Code", "Menlo"]);
        assert!(spec.thicken);
        // Absent = the bundled font, no thickening.
        let d = TermPrefs::default().font_spec();
        assert!(d.families.is_empty() && !d.thicken);
    }
}
