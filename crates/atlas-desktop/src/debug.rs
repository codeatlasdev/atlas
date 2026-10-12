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
    atlas_core::paths::atlas_home().join("desktop-debug.sock")
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("atlas-debug")
        .js_init_script(INIT_JS.to_string())
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

async fn serve<R: Runtime>(app: AppHandle<R>) -> std::io::Result<()> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path)?;
    // `eval` runs arbitrary JS in the app: only this user may connect.
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let (rd, mut wr) = stream.into_split();
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

async fn handle<R: Runtime>(app: &AppHandle<R>, req: &Value) -> Result<Value, String> {
    let state = app.state::<DebugState>();
    // The main window can host child webviews (embedded browser), so the
    // window and the app's own webview are looked up separately.
    let win = app.get_window("main").ok_or("main window not found")?;
    let view = app.get_webview("main").ok_or("main webview not found")?;
    match req["cmd"].as_str().unwrap_or_default() {
        "info" => Ok(json!({
            "pid": std::process::id(),
            "url": view.url().map(|u| u.to_string()).unwrap_or_default(),
            "visible": win.is_visible().unwrap_or(false),
            "window_id": crate::window_number(&win),
        })),
        "logs" => {
            let since = req["since"].as_u64().unwrap_or(0);
            let logs = state.logs.lock();
            Ok(logs
                .iter()
                .filter(|l| l["seq"].as_u64() >= Some(since))
                .cloned()
                .collect())
        }
        "eval" => {
            let js = req["js"].as_str().ok_or("missing js")?;
            let body = if js.contains("return") {
                js.to_string()
            } else {
                format!("return ({js});")
            };
            let id = state.seq.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = oneshot::channel();
            state.pending.lock().insert(id, tx);
            let script = format!(
                "(async () => {{ const r = (ok, value) => window.__TAURI_INTERNALS__.invoke('debug_reply', {{ id: {id}, ok, value }}); \
                 try {{ const v = await (async () => {{ {body} }})(); r(true, JSON.stringify(v === undefined ? null : v) ?? 'null'); }} \
                 catch (e) {{ r(false, e instanceof Error ? `${{e}}\\n${{e.stack ?? ''}}`.trim() : String(e)); }} }})()"
            );
            view.eval(&script).map_err(|e| e.to_string())?;
            match tokio::time::timeout(EVAL_TIMEOUT, rx).await {
                Ok(Ok(r)) => r,
                _ => {
                    state.pending.lock().remove(&id);
                    Err("eval timed out (page busy, or reloading)".into())
                }
            }
        }
        other => Err(format!(
            "unknown cmd {other:?}; expected info|logs|eval (screenshots: the CLI captures `info.window_id`)"
        )),
    }
}
