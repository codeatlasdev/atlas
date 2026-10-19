//! Debug-build-only bridge so an agent (or a human in a terminal) can inspect the
//! running app without touching the user's mouse/keyboard: console + error + CSP
//! capture, JS evaluation with a result, and window screenshots.
//!
//! Protocol: one JSON object per line over the unix socket at [`socket_path`],
//! one JSON reply per request. Client: `atlas desktop debug …`.
//! Never compiled into release builds (`#[cfg(debug_assertions)]` in main.rs).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

const MAX_LOGS: usize = 2000;
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs before any page script, so load-time errors and CSP violations are kept.
/// Calls queue until Tauri's IPC is ready.
const INIT_JS: &str = r#"
(() => {
  if (window.__ATLAS_DEBUG__) return;
  window.__ATLAS_DEBUG__ = true;
  const queue = [];
  const fmt = (a) => {
    if (a instanceof Error) return `${a}\n${a.stack ?? ""}`.trim();
    if (typeof a === "string") return a;
    try { return JSON.stringify(a); } catch { return String(a); }
  };
  const send = (level, args) => {
    queue.push({ level, msg: args.map(fmt).join(" ") });
    const ipc = window.__TAURI_INTERNALS__;
    if (!ipc) return;
    while (queue.length) ipc.invoke("debug_log", queue.shift()).catch(() => {});
  };
  for (const l of ["log", "info", "warn", "error", "debug"]) {
    const orig = console[l].bind(console);
    console[l] = (...a) => { orig(...a); send(l, a); };
  }
  addEventListener("error", (e) => send("error", [`${e.message} @ ${e.filename}:${e.lineno}`]));
  addEventListener("unhandledrejection", (e) => send("error", ["unhandled rejection:", e.reason]));
  document.addEventListener("securitypolicyviolation", (e) =>
    send("csp", [`${e.violatedDirective} blocked ${e.blockedURI || "inline"} @ ${e.sourceFile}:${e.lineNumber}`]));
  addEventListener("DOMContentLoaded", () => send("info", [`loaded ${location.href}`]));
})();
"#;

#[derive(Default)]
pub struct DebugState {
    logs: Mutex<VecDeque<Value>>,
    seq: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
}

pub fn socket_path() -> PathBuf {
    atlas_core::paths::desktop_debug_socket()
}

/// The init script: the console/error/CSP capture, plus `window.__ATLAS_AGENT__` in an agent instance. That window is
/// never focused (it must not steal focus), and the page pauses animation when unfocused; the flag tells it that
/// nobody is working above it, so its animations run and can be shot and recorded.
fn init_script(agent: bool) -> String {
    if agent {
        format!("window.__ATLAS_AGENT__ = true;\n{INIT_JS}")
    } else {
        INIT_JS.to_string()
    }
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("atlas-debug")
        .js_init_script(init_script(
            crate::agent_mode::Mode::current() == crate::agent_mode::Mode::Agent,
        ))
        .setup(|app, _| {
            app.manage(DebugState::default());
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = serve(app).await {
                    eprintln!("atlas-debug: {e}");
                }
            });
            Ok(())
        })
        .build()
}

#[tauri::command]
pub fn debug_log(state: State<'_, DebugState>, level: String, msg: String) {
    let seq = state.seq.fetch_add(1, Ordering::Relaxed);
    eprintln!("[webview {level}] {msg}");
    let mut logs = state.logs.lock();
    if logs.len() == MAX_LOGS {
        logs.pop_front();
    }
    let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    logs.push_back(json!({ "seq": seq, "ts": ts, "level": level, "msg": msg }));
}

#[tauri::command]
pub fn debug_reply(state: State<'_, DebugState>, id: u64, ok: bool, value: String) {
    if let Some(tx) = state.pending.lock().remove(&id) {
        let _ = tx.send(if ok {
            Ok(serde_json::from_str(&value).unwrap_or(Value::String(value)))
        } else {
            Err(value)
        });
    }
}

/// Binds the bridge socket (0600: `eval` runs arbitrary JS in the app, only this user may connect).
fn bind(path: &std::path::Path) -> std::io::Result<atlas_core::ipc::Listener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = atlas_core::ipc::remove_stale(path);
    let listener = atlas_core::ipc::Listener::bind(path)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(listener)
}

async fn serve<R: Runtime>(app: AppHandle<R>) -> std::io::Result<()> {
    let listener = bind(&socket_path())?;
    loop {
        let stream = listener.accept().await?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let (rd, mut wr) = atlas_core::ipc::split(stream);
            let mut lines = BufReader::new(rd).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let reply = match serde_json::from_str::<Value>(&line) {
                    Ok(req) => handle(&app, &req).await,
                    Err(e) => Err(format!("bad request: {e}")),
                };
                let out = match reply {
                    Ok(v) => json!({ "ok": true, "result": v }),
                    Err(e) => json!({ "ok": false, "error": e }),
                };
                if wr.write_all(format!("{out}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}

/// Runs `body` (a function body that may `return` and `await`) in the page and returns its JSON result.
async fn page_eval<R: Runtime>(
    app: &AppHandle<R>,
    body: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let state = app.state::<DebugState>();
    let view = app.get_webview("main").ok_or("main webview not found")?;
    let id = state.seq.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    state.pending.lock().insert(id, tx);
    let script = format!(
        "(async () => {{ const r = (ok, value) => window.__TAURI_INTERNALS__.invoke('debug_reply', {{ id: {id}, ok, value }}); \
         try {{ const v = await (async () => {{ {body} }})(); r(true, JSON.stringify(v === undefined ? null : v) ?? 'null'); }} \
         catch (e) {{ r(false, e instanceof Error ? `${{e}}\\n${{e.stack ?? ''}}`.trim() : String(e)); }} }})()"
    );
    view.eval(&script).map_err(|e| e.to_string())?;
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(r)) => r,
        _ => {
            state.pending.lock().remove(&id);
            Err("page did not answer in time (busy, or reloading)".into())
        }
    }
}

/// `window.__atlasDebug.<f>(args…)` in the page (lib/debug/agent.ts); args are JSON, never spliced as code.
async fn page_api<R: Runtime>(
    app: &AppHandle<R>,
    f: &str,
    args: &[Value],
    timeout: Duration,
) -> Result<Value, String> {
    page_eval(app, &api_body(f, args), timeout).await
}

/// The page-side body of an API call. The function name is one of ours; every argument is JSON, so a selector or
/// text from an agent can never become code.
fn api_body(f: &str, args: &[Value]) -> String {
    let args = args
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "const d = window.__atlasDebug; if (!d) throw new Error('the page API is not installed yet (the app is still booting, or this is not a debug build)'); return await d.{f}({args});"
    )
}

/// The page-side body of a root-scoped audit (`a11y`, `contrast`): `window.__atlasDebug.<f>(root)` on the element at
/// the selector (JSON-quoted, never spliced as code) or on the body.
fn scoped_body(f: &str, selector: Option<&str>) -> String {
    let root = selector.map_or("document.body".to_string(), |s| {
        format!("document.querySelector({})", json!(s))
    });
    format!(
        "const d = window.__atlasDebug; const root = {root}; if (!root) throw new Error('no element matches ' + {}); return d.{f}(root);",
        json!(selector)
    )
}

/// The page-side body of `wait`. A selector goes through `waitFor`. An expression is compiled by the bridge's own
/// native eval (as part of this body), never by `new Function` in the page: the bundled app's CSP forbids
/// `unsafe-eval` (measured: every `wait --expr` failed with an EvalError), while `debug eval` has always worked.
fn wait_body(selector: Option<&str>, expr: Option<&str>, timeout_ms: u64) -> String {
    let opts = json!({ "timeoutMs": timeout_ms, "pollMs": 50 });
    let guard = "const d = window.__atlasDebug; if (!d) throw new Error('the page API is not installed yet (the app is still booting, or this is not a debug build)');";
    match expr {
        Some(e) => format!(
            "{guard} return await d.waitFor({{ expr: {} }}, {opts}, undefined, () => ({e}));",
            json!(e)
        ),
        None => format!(
            "{guard} return await d.waitFor({{ selector: {} }}, {opts});",
            json!(selector)
        ),
    }
}

fn eval_body(js: &str) -> String {
    if js.contains("return") {
        js.to_string()
    } else {
        format!("return ({js});")
    }
}

async fn handle<R: Runtime>(app: &AppHandle<R>, raw: &Value) -> Result<Value, String> {
    use crate::debug_req::{Req, parse};
    // Capture commands keep their own validation (capture/request.rs).
    if let Some(c) = raw["cmd"].as_str().filter(|c| crate::capture::handles(c)) {
        // The bridge is generic over the runtime for tests; capture needs the real one.
        let args = raw.get("args").cloned().unwrap_or(Value::Null);
        let real: &dyn std::any::Any = app;
        let real = real
            .downcast_ref::<AppHandle>()
            .ok_or("capture needs the Wry runtime")?;
        return crate::capture::handle(real, c, &args).await;
    }
    let state = app.state::<DebugState>();
    match parse(raw)? {
        Req::Info => {
            let win = app.get_window("main").ok_or("main window not found")?;
            let view = app.get_webview("main").ok_or("main webview not found")?;
            Ok(json!({
                "pid": std::process::id(),
                "url": view.url().map(|u| u.to_string()).unwrap_or_default(),
                "visible": win.is_visible().unwrap_or(false),
                "window_id": crate::window_number(&win),
                "agent": crate::agent_mode::Mode::current() == crate::agent_mode::Mode::Agent,
                "socket": socket_path(),
            }))
        }
        Req::Logs { since } => Ok(state
            .logs
            .lock()
            .iter()
            .filter(|l| l["seq"].as_u64() >= Some(since))
            .cloned()
            .collect()),
        Req::Eval { js } => page_eval(app, &eval_body(&js), EVAL_TIMEOUT).await,
        Req::State => page_api(app, "state", &[], EVAL_TIMEOUT).await,
        Req::Wait {
            selector,
            expr,
            timeout_ms,
        } => {
            let body = wait_body(selector.as_deref(), expr.as_deref(), timeout_ms);
            page_eval(app, &body, Duration::from_millis(timeout_ms) + EVAL_TIMEOUT).await
        }
        Req::Click { selector } => {
            page_api(app, "clickTarget", &[json!(selector)], EVAL_TIMEOUT).await
        }
        Req::Type {
            selector,
            text,
            delay_ms,
        } => {
            let budget =
                Duration::from_millis(delay_ms * text.chars().count() as u64) + EVAL_TIMEOUT;
            let value = page_api(
                app,
                "typeTarget",
                &[json!(selector), json!(text), json!(delay_ms)],
                budget,
            )
            .await?;
            Ok(json!({ "typed": true, "value": value }))
        }
        Req::A11y { selector } => {
            page_eval(
                app,
                &scoped_body("a11yTree", selector.as_deref()),
                EVAL_TIMEOUT,
            )
            .await
        }
        Req::Contrast { selector } => {
            page_eval(
                app,
                &scoped_body("contrastAudit", selector.as_deref()),
                EVAL_TIMEOUT,
            )
            .await
        }
        Req::Perf { secs } => {
            let body = format!(
                "const p = window.__atlasDebug.perf; p.start(); await new Promise((r) => setTimeout(r, {})); return p.stop();",
                secs * 1000
            );
            page_eval(app, &body, Duration::from_secs(secs) + EVAL_TIMEOUT).await
        }
        Req::GestureRecord { out, secs } => gesture_record(app, out, secs).await,
        Req::GestureReplay { trace } => gesture_replay(app, trace).await,
        Req::Shot {
            out,
            target,
            full,
            window,
        } => shot(app, out, target, full, window).await,
        Req::Record { out, stop, fps } => record(app, out, stop, fps).await,
        Req::Resize { w, h } => {
            set_view_size(app, w, h).await?;
            let win = app.get_window("main").ok_or("main window not found")?;
            let scale = win.scale_factor().map_err(|e| e.to_string())?;
            let inner = win.inner_size().map_err(|e| e.to_string())?;
            Ok(
                json!({ "width": f64::from(inner.width) / scale, "height": f64::from(inner.height) / scale }),
            )
        }
    }
}

/// `gesture-record`: arm the swipe monitor's recorder, wait, write what the trackpad did to `out` as a trace the
/// engine tests and the bench replay (`Trace` in `lib/shell/trace.ts`; `expect` is filled in by hand afterwards).
#[cfg(target_os = "macos")]
async fn gesture_record<R: Runtime>(
    app: &AppHandle<R>,
    out: PathBuf,
    secs: u64,
) -> Result<Value, String> {
    let real: &dyn std::any::Any = app;
    let real = real
        .downcast_ref::<AppHandle>()
        .ok_or("needs the Wry runtime")?;
    let (tx, rx) = std::sync::mpsc::channel();
    real.run_on_main_thread(move || {
        let _ = tx.send(crate::swipe_mac::record_start());
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())??;
    tokio::time::sleep(Duration::from_secs(secs)).await;
    let (tx, rx) = std::sync::mpsc::channel();
    real.run_on_main_thread(move || {
        let _ = tx.send(crate::swipe_mac::record_stop());
    })
    .map_err(|e| e.to_string())?;
    let events = rx.recv().map_err(|e| e.to_string())??;
    let trace = json!({
        "name": out.file_stem().and_then(|s| s.to_str()).unwrap_or("recorded"),
        "source": format!("recorded:{} {}s on this Mac's trackpad", chrono::Local::now().format("%Y-%m-%d %H:%M"), secs),
        "expect": { "zone": "space", "steps": 0, "untouched": false },
        "events": events,
    });
    std::fs::write(
        &out,
        serde_json::to_vec_pretty(&trace).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(json!({ "out": out, "events": events.len() }))
}

#[cfg(not(target_os = "macos"))]
async fn gesture_record<R: Runtime>(_: &AppHandle<R>, _: PathBuf, _: u64) -> Result<Value, String> {
    Err("gesture recording is macOS only".into())
}

/// Page-side replay (the gesture bench): the page sends the samples, native feeds them to the engine at their
/// pace and answers when the last settle is at rest. Debug builds only (this file is).
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn gesture_replay_samples(
    app: AppHandle,
    events: Vec<crate::swipe_engine::Sample>,
) -> Result<Value, String> {
    let r = crate::swipe_mac::replay(&app, events).await;
    serde_json::to_value(r).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn gesture_replay_samples(_events: Vec<Value>) -> Result<Value, String> {
    Err("gesture replay is macOS only".into())
}

/// `gesture-replay`: inject a trace through the swipe engine's own entry point at its recorded pace.
#[cfg(target_os = "macos")]
async fn gesture_replay<R: Runtime>(app: &AppHandle<R>, trace: PathBuf) -> Result<Value, String> {
    let real: &dyn std::any::Any = app;
    let real = real
        .downcast_ref::<AppHandle>()
        .ok_or("needs the Wry runtime")?;
    let raw = std::fs::read_to_string(&trace).map_err(|e| format!("{}: {e}", trace.display()))?;
    #[derive(serde::Deserialize)]
    struct T {
        events: Vec<crate::swipe_engine::Sample>,
    }
    let t: T = serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", trace.display()))?;
    let r = crate::swipe_mac::replay(real, t.events).await;
    serde_json::to_value(r).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
async fn gesture_replay<R: Runtime>(_: &AppHandle<R>, _: PathBuf) -> Result<Value, String> {
    Err("gesture replay is macOS only".into())
}

/// `shot`: a page snapshot (whole page, an element, a rect; `full` grows the view to the content first) or, with
/// `window`, the compositor's picture of the whole window. Page snapshots need no permission and work while the
/// window is occluded.
async fn shot<R: Runtime>(
    app: &AppHandle<R>,
    out: PathBuf,
    target: crate::debug_req::Target,
    full: bool,
    window: bool,
) -> Result<Value, String> {
    use crate::debug_req::Target;
    let real: &dyn std::any::Any = app;
    let real = real
        .downcast_ref::<AppHandle>()
        .ok_or("shot needs the Wry runtime")?
        .clone();
    if window {
        let args = json!({ "out": out, "mode": "window" });
        return crate::capture::handle(&real, "capture_shot", &args).await;
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let view: Value = page_api(app, "view", &[], EVAL_TIMEOUT).await?;
    let scale = view["scale"].as_f64().unwrap_or(1.0);
    let (vw, vh) = (
        view["w"].as_f64().unwrap_or(0.0),
        view["h"].as_f64().unwrap_or(0.0),
    );
    // The region, in CSS px of the view.
    let selector = match &target {
        Target::Selector(s) => Some(s.clone()),
        _ => None,
    };
    let mut resize = None;
    let region = match (&target, full) {
        (Target::Rect { x, y, w, h }, _) => Some((*x, *y, *w, *h)),
        (_, true) => {
            let size = page_api(
                app,
                "fullShotSize",
                &[json!(selector), json!({ "w": vw, "h": vh })],
                EVAL_TIMEOUT,
            )
            .await?;
            let (w, h) = (
                size["width"].as_f64().unwrap_or(vw),
                size["height"].as_f64().unwrap_or(vh),
            );
            // The view must be as large as the content for one snapshot to show it; restored afterwards.
            if w > vw || h > vh {
                resize = Some((vw.max(w), vh.max(h), vw, vh));
            }
            Some((0.0, 0.0, w, h))
        }
        (Target::Selector(s), false) => {
            let r = page_api(
                app,
                "elementShotRect",
                &[json!(s), json!({ "w": vw, "h": vh, "scale": scale })],
                EVAL_TIMEOUT,
            )
            .await?;
            Some((
                r["x"].as_f64().unwrap_or(0.0),
                r["y"].as_f64().unwrap_or(0.0),
                r["width"].as_f64().unwrap_or(vw),
                r["height"].as_f64().unwrap_or(vh),
            ))
        }
        (Target::Page, false) => None,
    };
    if let Some((w, h, _, _)) = resize {
        set_view_size(app, w, h).await?;
    }
    let region = region.map(|(x, y, w, h)| crate::capture::snapshot_region(x, y, w, h, scale));
    let out2 = out.clone();
    let app2 = real.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::capture::snapshot_to(&app2, region, &out2)
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some((_, _, w0, h0)) = resize {
        set_view_size(app, w0, h0).await?;
    }
    result?;
    let meta = std::fs::metadata(&out).map_err(|e| e.to_string())?;
    Ok(json!({ "out": out, "bytes": meta.len(), "scale": scale }))
}

/// Resize the window so the page view is `w`×`h` CSS px, then let layout settle.
async fn set_view_size<R: Runtime>(app: &AppHandle<R>, w: f64, h: f64) -> Result<(), String> {
    let win = app.get_window("main").ok_or("main window not found")?;
    win.set_size(tauri::LogicalSize::new(w, h))
        .map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    Ok(())
}

/// `record`: the in-app recorder (capture_start/stop) until the time is up or the selector shows.
async fn record<R: Runtime>(
    app: &AppHandle<R>,
    out: PathBuf,
    stop: crate::debug_req::Stop,
    fps: u32,
) -> Result<Value, String> {
    use crate::debug_req::Stop;
    let real: &dyn std::any::Any = app;
    let real = real
        .downcast_ref::<AppHandle>()
        .ok_or("record needs the Wry runtime")?
        .clone();
    let started =
        crate::capture::handle(&real, "capture_start", &json!({ "out": out, "fps": fps })).await?;
    let waited = match stop {
        Stop::AfterSecs(s) => {
            tokio::time::sleep(Duration::from_secs(s)).await;
            Ok(Value::Null)
        }
        Stop::Selector(sel) => {
            let ms = crate::debug_req::MAX_RECORD_SECS * 1000;
            page_eval(
                app,
                &wait_body(Some(&sel), None, ms),
                Duration::from_millis(ms) + EVAL_TIMEOUT,
            )
            .await
        }
    };
    // Always stop: a recording must not outlive a failed wait.
    let stopped = crate::capture::handle(&real, "capture_stop", &Value::Null).await;
    waited?;
    let mut stats = stopped?;
    stats["start"] = started;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn only_an_agent_instance_tells_the_page_it_is_one() {
        assert!(init_script(true).starts_with("window.__ATLAS_AGENT__ = true;"));
        assert!(!init_script(false).contains("__ATLAS_AGENT__"));
        // the capture of console/errors/CSP is in both
        for a in [true, false] {
            assert!(init_script(a).contains("securitypolicyviolation"));
        }
    }

    #[test]
    fn page_api_arguments_are_json_so_agent_input_cannot_become_code() {
        let evil = r#"x'); window.pwned = 1; ('"#;
        let body = api_body("clickTarget", &[json!(evil)]);
        assert!(
            body.contains(r#"d.clickTarget("x'); window.pwned = 1; ('")"#),
            "{body}"
        );
        // a selector with a double quote and a backslash stays one JSON string literal: it parses back unchanged
        let sel = r#"[data-x="a\b"]"#;
        let body = api_body("clickTarget", &[json!(sel)]);
        let lit = body
            .split("d.clickTarget(")
            .nth(1)
            .and_then(|r| r.strip_suffix(");"))
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(lit).unwrap(), sel);
        assert!(api_body("state", &[]).contains("d.state()"));
        assert!(api_body("state", &[]).contains("not installed yet"));
    }

    #[test]
    fn scoped_audits_use_a_json_quoted_selector_or_the_body() {
        assert!(scoped_body("a11yTree", None).contains("const root = document.body;"));
        let b = scoped_body("a11yTree", Some(r#"a[href="x"]"#));
        assert!(
            b.contains(r#"document.querySelector("a[href=\"x\"]")"#),
            "{b}"
        );
        assert!(b.contains("d.a11yTree(root)"), "{b}");
        let c = scoped_body("contrastAudit", None);
        assert!(c.contains("d.contrastAudit(root)"), "{c}");
        assert!(c.contains("const root = document.body;"), "{c}");
    }

    /// Incident (first agent run): `wait --expr` died with "Refused to evaluate a string as JavaScript" (CSP).
    #[test]
    fn an_expression_wait_is_compiled_by_the_native_eval_not_by_new_function_in_the_page() {
        let b = wait_body(None, Some("window.__atlas.ws.loaded"), 500);
        assert!(b.contains("() => (window.__atlas.ws.loaded)"), "{b}");
        assert!(!b.contains("new Function"), "{b}");
        // the page API still gets the text, JSON-quoted, only for the timeout message
        assert!(b.contains(r#"{ expr: "window.__atlas.ws.loaded" }"#), "{b}");
        assert!(b.contains("\"timeoutMs\":500"), "{b}");
    }

    #[test]
    fn a_selector_wait_quotes_the_selector_as_json() {
        let b = wait_body(Some(r#"[data-intro="done"]"#), None, 10_000);
        assert!(
            b.contains(r#"{ selector: "[data-intro=\"done\"]" }"#),
            "{b}"
        );
        assert!(!b.contains("() =>"), "{b}");
    }

    #[test]
    fn eval_takes_an_expression_or_a_body_with_return() {
        assert_eq!(eval_body("document.title"), "return (document.title);");
        assert_eq!(eval_body("const n = 1; return n"), "const n = 1; return n");
    }

    /// Incident: an isolated instance (fresh HOME, e.g. Studio's capture) logged
    /// `atlas-debug: No such file or directory` and had no bridge, because `~/.atlas` did not exist yet
    /// when the bridge bound its socket (the ctl socket, which creates the directory, races it).
    // Socket files and mode bits: Unix (Windows serves a named pipe).
    #[cfg(unix)]
    #[tokio::test]
    async fn bind_creates_the_missing_socket_directory_and_locks_it_to_the_user() {
        let d = tempfile::tempdir().unwrap();
        let sock = d.path().join("fresh-home/.atlas/desktop-debug.sock");
        let _l = bind(&sock).expect("a fresh HOME must still get a bridge");
        assert_eq!(
            std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    // Socket files and mode bits: Unix (Windows serves a named pipe).
    #[cfg(unix)]
    #[tokio::test]
    async fn bind_replaces_a_stale_socket_file_from_a_crashed_run() {
        let d = tempfile::tempdir().unwrap();
        let sock = d.path().join("s.sock");
        std::fs::write(&sock, b"stale").unwrap();
        bind(&sock).expect("a stale file must not block the bridge");
    }
}
