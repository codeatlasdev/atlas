//! Control socket for `atlas wt|term|browser|file` (all builds).
//!
//! `~/.atlas/desktop.sock` (0600) + `~/.atlas/desktop.token` (0600, new per run).
//! One request per connection: first line `{"token"}`, second line
//! `{"cmd","args"}`, reply `{"ok","error"?,"data"?}` (same shape as
//! atlas-tui's control.rs). The app's model (worktrees, tabs, agents) lives in
//! the frontend, so every command is forwarded as `ctl://request {id, cmd, args}`
//! and answered through [`ctl_reply`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use atlas_core::ipc;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

const MAX_LINE: usize = 1024 * 1024;
/// Upper bound per request; `terminal_wait` / `worktree_create` pass their own, capped here.
const MAX_WAIT: Duration = Duration::from_secs(3600);
const DEFAULT_WAIT: Duration = Duration::from_secs(60);

pub fn socket_path() -> PathBuf {
    atlas_core::paths::desktop_socket()
}

pub fn token_path() -> PathBuf {
    atlas_core::paths::desktop_token()
}

#[derive(Default)]
pub struct Ctl {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
}

#[tauri::command]
pub fn ctl_reply(state: tauri::State<'_, Arc<Ctl>>, id: u64, ok: bool, value: Value) {
    if let Some(tx) = state.pending.lock().remove(&id) {
        let _ = tx.send(if ok {
            Ok(value)
        } else {
            Err(value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string()))
        });
    }
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            eprintln!("atlas-desktop: control socket: {e}");
        }
    });
}

async fn serve(app: AppHandle) -> std::io::Result<()> {
    let path = socket_path();
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    if ipc::connect(&path).await.is_ok() {
        return Err(std::io::Error::other(
            "another Atlas Desktop owns the control socket",
        ));
    }
    let _ = ipc::remove_stale(&path);
    let token = atlas_host::paths::random_hex(24)?;
    let mut f = atlas_core::sys::owner_only(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .open(token_path())?;
    std::io::Write::write_all(&mut f, token.as_bytes())?;
    let listener = ipc::Listener::bind(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    loop {
        let stream = listener.accept().await?;
        let (app, token) = (app.clone(), token.clone());
        tauri::async_runtime::spawn(async move {
            let _ = conn(stream, app, &token).await;
        });
    }
}

async fn read_json(r: &mut BufReader<ipc::ReadHalf>) -> Option<Value> {
    let mut line = String::new();
    let n = r.read_line(&mut line).await.ok()?;
    if n == 0 || n > MAX_LINE {
        return None;
    }
    serde_json::from_str(&line).ok()
}

async fn conn(stream: ipc::Stream, app: AppHandle, token: &str) -> std::io::Result<()> {
    let (r, mut w) = ipc::split(stream);
    let mut r = BufReader::new(r);
    let reply = match read_json(&mut r).await {
        Some(h) if h["token"] == token => match read_json(&mut r).await {
            Some(req) => dispatch(&app, req).await,
            None => Err("bad request".into()),
        },
        _ => Err("unauthorized".into()),
    };
    let v = match reply {
        Ok(data) => json!({"ok": true, "data": data}),
        Err(e) => json!({"ok": false, "error": e}),
    };
    w.write_all(format!("{v}\n").as_bytes()).await
}

async fn dispatch(app: &AppHandle, req: Value) -> Result<Value, String> {
    let cmd = req["cmd"].as_str().ok_or("missing cmd")?.to_string();
    let args = req.get("args").cloned().unwrap_or(json!({}));
    // Native capture is answered here; scripted recordings (`capture_run`) go to the frontend.
    if crate::capture::handles(&cmd) {
        return crate::capture::handle(app, &cmd, &args).await;
    }
    let wait = args["timeout_s"]
        .as_u64()
        .map(|s| Duration::from_secs(s + 5).min(MAX_WAIT))
        .unwrap_or(if cmd == "worktree_create" {
            Duration::from_secs(900)
        } else {
            DEFAULT_WAIT
        });
    let ctl = app.state::<Arc<Ctl>>().inner().clone();
    let id = ctl.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    ctl.pending.lock().insert(id, tx);
    app.emit("ctl://request", json!({"id": id, "cmd": cmd, "args": args}))
        .map_err(|e| e.to_string())?;
    let res = tokio::time::timeout(wait, rx).await;
    ctl.pending.lock().remove(&id);
    match res {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => Err("request dropped".into()),
        Err(_) => Err(format!("{cmd}: timed out (is the Atlas window loaded?)")),
    }
}
