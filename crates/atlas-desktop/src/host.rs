//! Bridge to `atlas hostd` instances: the local one plus one per SSH host
//! (`ssh:<alias>`, see `remote.rs`). Pushes from every host are re-emitted as
//! Tauri events; pty ids are UUIDs, so they never collide across hosts.
//!
//! Routing (`HostHub::call`): an explicit `host` arg wins; else a `pty` arg goes
//! to the host that created it; else a path arg (`cwd`/`path`/`from`) goes to the
//! host of the registry entry whose root contains it; else `local`.
//!
//! Events: `pty://<id>` `{data, replay}` (base64), `host://event` for
//! `pty_title`/`pty_exit`/`fs_event`, `host://reconnected {host}` after a reconnect
//! (the frontend re-attaches its terminals), `host://status {status, detail}` once the
//! daemon version was reconciled with the app's (`upgraded` = replaced live, terminals kept).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use atlas_host::HostClient;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

pub const LOCAL: &str = "local";

#[derive(Default)]
pub struct HostHub {
    clients: RwLock<HashMap<String, HostClient>>,
    pty_host: Mutex<HashMap<String, String>>,
    /// Outcome of `reconcile` for the local daemon, kept so a webview that registers its
    /// listeners after startup still learns it (`host_status`).
    status: Mutex<Value>,
}

/// Host of the registry entry whose root contains `path` (longest match).
pub fn host_for_path(path: &str) -> String {
    crate::registry::load()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| path == e.root || path.starts_with(&format!("{}/", e.root)))
        .max_by_key(|e| e.root.len())
        .and_then(|e| e.host)
        .unwrap_or_else(|| LOCAL.into())
}

impl HostHub {
    /// Connected client for `host`; the local one is waited for (startup), remote
    /// ones are connected on demand.
    pub async fn client(&self, host: &str) -> Result<HostClient, String> {
        if host != LOCAL {
            if let Some(c) = self.clients.read().await.get(host).cloned() {
                return Ok(c);
            }
            return Err(format!("{host} is not connected (Settings → SSH)"));
        }
        for _ in 0..100 {
            if let Some(c) = self.clients.read().await.get(LOCAL).cloned() {
                return Ok(c);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("atlas hostd is not reachable (see ~/.atlas/hostd.log)".into())
    }

    pub async fn set(&self, host: &str, c: Option<HostClient>) {
        let mut m = self.clients.write().await;
        match c {
            Some(c) => m.insert(host.to_string(), c),
            None => m.remove(host),
        };
    }

    fn route(&self, args: &Value) -> String {
        if let Some(h) = args["host"].as_str() {
            return h.to_string();
        }
        if let Some(p) = args["pty"].as_str() {
            return self
                .pty_host
                .lock()
                .get(p)
                .cloned()
                .unwrap_or_else(|| LOCAL.into());
        }
        ["cwd", "path", "from"]
            .iter()
            .find_map(|k| args[*k].as_str())
            .map(host_for_path)
            .unwrap_or_else(|| LOCAL.into())
    }

    /// Run a hostd command on the routed host. Local `exec` gets the user's
    /// login-shell env (PATH for git/gh/atlas); remote hosts use their own.
    pub async fn call(&self, cmd: &str, mut args: Value) -> Result<Value, String> {
        let host = self.route(&args);
        if let Some(o) = args.as_object_mut() {
            o.remove("host");
        }
        if cmd == "exec" && host == LOCAL {
            let mut env: serde_json::Map<String, Value> = crate::shell_env::vars_async()
                .await
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect();
            if let Some(own) = args["env"].as_object() {
                env.extend(own.clone());
            }
            args["env"] = Value::Object(env);
        }
        let res = self
            .client(&host)
            .await?
            .call(cmd, args)
            .await
            .map_err(|e| format!("{e:#}"))?;
        if cmd == "pty_create"
            && let Some(p) = res["pty"].as_str()
        {
            self.pty_host.lock().insert(p.to_string(), host);
        }
        Ok(res)
    }

    /// Learn which host owns each existing pty (after (re)connecting).
    pub async fn index_ptys(&self, host: &str) {
        let Ok(c) = self.client(host).await else {
            return;
        };
        if let Ok(v) = c.call("pty_list", json!({})).await {
            let mut m = self.pty_host.lock();
            for p in v["ptys"].as_array().into_iter().flatten() {
                if let Some(id) = p["pty"].as_str() {
                    m.insert(id.to_string(), host.to_string());
                }
            }
        }
    }
}

/// Filesystem helpers over hostd, so local and SSH checkouts share one code path.
impl HostHub {
    pub async fn exists(&self, host: &str, path: &str) -> bool {
        self.call("fs_stat", json!({"host": host, "path": path}))
            .await
            .is_ok()
    }

    pub async fn mkdir(&self, host: &str, path: &str) -> Result<(), String> {
        self.call("fs_mkdir", json!({"host": host, "path": path}))
            .await
            .map(|_| ())
    }

    /// File contents, `None` when missing.
    pub async fn read_text(&self, host: &str, path: &str) -> Option<String> {
        use base64::Engine;
        let r = self
            .call("fs_read", json!({"host": host, "path": path}))
            .await
            .ok()?;
        let b = base64::engine::general_purpose::STANDARD
            .decode(r["data"].as_str()?)
            .ok()?;
        String::from_utf8(b).ok()
    }

    /// `$HOME` of the host's hostd user.
    pub async fn home(&self, host: &str) -> Result<String, String> {
        if host == LOCAL {
            return Ok(dirs::home_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned());
        }
        let r = self
            .call(
                "exec",
                json!({"host": host, "cwd": "/", "argv": ["sh", "-c", "printf %s \"$HOME\""]}),
            )
            .await?;
        use base64::Engine;
        let b = base64::engine::general_purpose::STANDARD
            .decode(r["stdout"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }

    /// atlas.yaml (+ atlas.local.yaml) of a checkout on any host, ports shifted
    /// by `offset`. Remote files are copied to a temp dir and parsed locally.
    pub async fn load_config(
        &self,
        host: &str,
        root: &str,
        offset: u16,
    ) -> Result<atlas_tui::config::ProjectConfig, String> {
        use atlas_tui::config;
        let mut cfg = if host == LOCAL {
            config::load(std::path::Path::new(root)).map_err(|e| format!("{e:#}"))?
        } else {
            let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
            for f in ["atlas.yaml", "atlas.local.yaml"] {
                if let Some(t) = self.read_text(host, &format!("{root}/{f}")).await {
                    std::fs::write(tmp.path().join(f), t).map_err(|e| e.to_string())?;
                }
            }
            config::load(tmp.path()).map_err(|e| format!("{e:#}"))?
        };
        if offset > 0 {
            config::apply_port_offset(&mut cfg, offset);
        }
        Ok(cfg)
    }
}

fn forward(app: &AppHandle, ev: &Value) {
    match ev["ev"].as_str() {
        Some(k @ ("pty_output" | "pty_replay")) => {
            let pty = ev["pty"].as_str().unwrap_or_default();
            let _ = app.emit(
                &format!("pty://{pty}"),
                json!({"data": ev["data"], "replay": k == "pty_replay"}),
            );
        }
        Some("disconnected") | None => {}
        Some(_) => {
            let _ = app.emit("host://event", ev);
        }
    }
}

/// Register a connected client and pump its events until it disconnects.
pub async fn serve_client(app: &AppHandle, host: &str, c: HostClient) {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let mut rx = c.events();
    hub.set(host, Some(c)).await;
    hub.index_ptys(host).await;
    let _ = app.emit("host://reconnected", json!({"host": host}));
    while let Ok(ev) = rx.recv().await {
        if ev["ev"] == "disconnected" {
            break;
        }
        forward(app, &ev);
    }
    hub.set(host, None).await;
}

/// Bring the daemon we connected to up to this app's version. The terminal host outlives
/// app updates, so after one it is usually older. Returns `true` when it was replaced
/// (the caller reconnects).
///
/// - upgradeable daemon: live takeover, every terminal keeps running (`host://upgraded`);
/// - proto 1 daemon with no live terminal: restart it, nothing to lose;
/// - proto 1 daemon with live terminals: keep it and say so (`host://outdated`).
async fn reconcile(app: &AppHandle, c: &HostClient, bin: &std::path::Path) -> bool {
    use atlas_host::upgrade::{Plan, plan};
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let set = |status: &str, detail: Value| {
        let v = json!({"status": status, "detail": detail});
        *hub.status.lock() = v.clone();
        let _ = app.emit("host://status", v);
    };
    let Ok(info) = c.call("info", json!({})).await else {
        return false;
    };
    // proto 1 reports no `live_ptys`: count the unexited ones from the list.
    let live = if info["live_ptys"].is_null() {
        c.call("pty_list", json!({}))
            .await
            .map(|v| {
                v["ptys"]
                    .as_array()
                    .map_or(0, |a| a.iter().filter(|p| p["exited"].is_null()).count())
            })
            .unwrap_or(0)
    } else {
        0
    };
    match plan(&info, live, env!("CARGO_PKG_VERSION")) {
        Plan::Current => {
            set("current", json!({}));
            false
        }
        Plan::Upgrade => match c
            .call("upgrade", json!({"exe": bin.to_string_lossy()}))
            .await
        {
            Ok(r) => {
                set("upgraded", r);
                true
            }
            Err(e) => {
                // The old host keeps running; same notice as a host that cannot hand over.
                set("outdated", json!(format!("{e:#}")));
                false
            }
        },
        Plan::RestartEmpty => {
            let _ = c.call("shutdown", json!({})).await;
            let _ = app.emit(
                "host://upgraded",
                json!({"handed_off": 0, "restarted": true}),
            );
            true
        }
        Plan::Outdated => {
            set("outdated", json!("terminal host predates live upgrade"));
            false
        }
    }
}

/// Connect to the local hostd (starting it if needed) and keep it alive.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        // One replacement per app session: if the installed CLI is itself older than the
        // app, retrying would loop forever.
        let mut reconciled = false;
        loop {
            let bin = match atlas_host::client::atlas_bin() {
                Ok(b) => b,
                Err(e) => {
                    let _ = app.emit("host://error", e.to_string());
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            match atlas_host::client::ensure_local(&bin, "desktop").await {
                Ok(c) => {
                    backoff = Duration::from_secs(1);
                    if !std::mem::replace(&mut reconciled, true) && reconcile(&app, &c, &bin).await
                    {
                        // Replaced: drop this connection and connect to the new daemon
                        // (`serve_client` then emits `host://reconnected`).
                        drop(c);
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        continue;
                    }
                    serve_client(&app, LOCAL, c).await;
                }
                Err(e) => {
                    let _ = app.emit("host://error", format!("{e:#}"));
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(10));
                }
            }
        }
    });
}

/// `pty_list` of every connected host, merged (`host` added to each item).
#[tauri::command]
pub async fn host_pty_list(hub: tauri::State<'_, Arc<HostHub>>) -> Result<Vec<Value>, String> {
    let hosts: Vec<(String, HostClient)> = hub
        .clients
        .read()
        .await
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut out = Vec::new();
    for (h, c) in hosts {
        if let Ok(v) = c.call("pty_list", json!({})).await {
            for mut p in v["ptys"].as_array().cloned().unwrap_or_default() {
                if let Some(id) = p["pty"].as_str() {
                    hub.pty_host.lock().insert(id.to_string(), h.clone());
                }
                p["host"] = json!(h);
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// Last `reconcile` outcome (`{status: current|upgraded|outdated, detail}`), `null` before it ran.
#[tauri::command]
pub fn host_status(hub: tauri::State<'_, Arc<HostHub>>) -> Value {
    hub.status.lock().clone()
}

/// Generic passthrough to hostd (`pty_*`, `exec`, `fs_*`, `info`), routed by host.
#[tauri::command]
pub async fn host_call(
    hub: tauri::State<'_, Arc<HostHub>>,
    cmd: String,
    args: Option<Value>,
) -> Result<Value, String> {
    if cmd == "shutdown" {
        return Err("shutdown is not allowed from the app".into());
    }
    hub.call(&cmd, args.unwrap_or_else(|| json!({}))).await
}
