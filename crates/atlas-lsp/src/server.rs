//! One running language-server process: spawn, `initialize`, and the handler for everything
//! it sends us (diagnostics, progress, configuration, applyEdit, showMessage).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

use crate::caps;
use crate::install::Launch;
use crate::rpc::{Handler, Peer, RpcError};

/// What the manager learns from a server, delivered over a bounded channel.
#[derive(Debug)]
pub enum Event {
    Diagnostics {
        path: String,
        version: Option<i64>,
        diagnostics: Value,
    },
    /// Progress text or a window/showMessage error changed.
    Status,
    ApplyEdit {
        id: String,
        edit: Value,
        reply: oneshot::Sender<bool>,
    },
    /// The process exited or the connection broke.
    Exited,
}

/// How long `shutdown` gets before the process group is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);
/// A client that never acknowledges `lsp_apply_edit` must not hold the server forever.
pub const APPLY_EDIT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Live {
    pub peer: Peer,
    pub capabilities: Value,
    pub child: Arc<tokio::sync::Mutex<Child>>,
    pub pid: Option<u32>,
    /// Paths for which `didOpen` was sent on this connection (a restart gets a fresh set).
    pub opened: Mutex<std::collections::HashSet<String>>,
}

/// State shared with the RPC handler (progress, last error message).
pub struct Shared {
    pub name: &'static str,
    pub root: String,
    pub progress: Mutex<HashMap<String, (String, String)>>,
    pub message: Mutex<Option<String>>,
    /// The user's `settings` for this server (merged over Atlas's defaults when answering
    /// `workspace/configuration`).
    pub settings: Option<Value>,
    pub events: tokio::sync::mpsc::Sender<(u64, Event)>,
    /// Generation of this process, so the manager ignores events from a replaced one.
    pub gen_id: u64,
    pub next_edit: std::sync::atomic::AtomicU64,
}

impl Shared {
    /// Progress text of one active token (stable choice: the greatest token), `title · detail`.
    pub fn progress_text(&self) -> Option<String> {
        let p = self.progress.lock();
        let (_, (title, detail)) = p.iter().max_by(|a, b| a.0.cmp(b.0))?;
        Some(match (title.is_empty(), detail.is_empty()) {
            (_, true) => title.clone(),
            (true, false) => detail.clone(),
            (false, false) => format!("{title} · {detail}"),
        })
    }

    fn send(&self, ev: Event) {
        // Bounded: if the manager is badly behind, drop status/diagnostic bursts rather than
        // grow memory (diagnostics are re-published by servers on the next change).
        let _ = self.events.try_send((self.gen_id, ev));
    }
}

pub struct ServerHandler {
    pub shared: Arc<Shared>,
}

type Fut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl Handler for ServerHandler {
    fn notification(&self, method: &str, params: Value) {
        let sh = &self.shared;
        match method {
            "textDocument/publishDiagnostics" => {
                let Some(path) = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .and_then(caps::path_from_uri)
                else {
                    return;
                };
                sh.send(Event::Diagnostics {
                    path,
                    version: params.get("version").and_then(Value::as_i64),
                    diagnostics: params.get("diagnostics").cloned().unwrap_or(json!([])),
                });
            }
            "$/progress" => {
                let token = params
                    .get("token")
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                let v = &params["value"];
                match v.get("kind").and_then(Value::as_str) {
                    Some("end") => {
                        sh.progress.lock().remove(&token);
                    }
                    Some(kind) => {
                        let mut map = sh.progress.lock();
                        let slot = map.entry(token).or_default();
                        if kind == "begin" || slot.0.is_empty() {
                            slot.0 = v.get("title").and_then(Value::as_str).unwrap_or("").into();
                        }
                        let msg = v.get("message").and_then(Value::as_str).unwrap_or("");
                        let pct = v.get("percentage").and_then(Value::as_u64);
                        slot.1 = match pct {
                            Some(p) if msg.is_empty() => format!("{p}%"),
                            Some(p) => format!("{msg} {p}%"),
                            None => msg.to_string(),
                        };
                    }
                    None => return,
                }
                sh.send(Event::Status);
            }
            // type 1 = Error: surface it; lower severities are noise for a status line.
            "window/showMessage" if params.get("type").and_then(Value::as_i64) == Some(1) => {
                *sh.message.lock() = params
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                sh.send(Event::Status);
            }
            _ => {}
        }
    }

    fn request(&self, method: String, params: Value) -> Fut<Result<Value, RpcError>> {
        let sh = self.shared.clone();
        Box::pin(async move {
            match method.as_str() {
                "workspace/configuration" => {
                    let items = params["items"].as_array().cloned().unwrap_or_default();
                    Ok(Value::Array(
                        items
                            .iter()
                            .map(|i| {
                                caps::configuration_with(
                                    sh.name,
                                    i["section"].as_str(),
                                    sh.settings.as_ref(),
                                )
                            })
                            .collect(),
                    ))
                }
                "window/workDoneProgress/create"
                | "client/registerCapability"
                | "client/unregisterCapability"
                | "workspace/diagnostic/refresh"
                | "workspace/inlayHint/refresh"
                | "workspace/semanticTokens/refresh"
                | "workspace/codeLens/refresh" => Ok(Value::Null),
                "workspace/workspaceFolders" => Ok(json!([{
                    "uri": caps::uri_from_path(&sh.root),
                    "name": Path::new(&sh.root).file_name().and_then(|n| n.to_str()).unwrap_or("root"),
                }])),
                "workspace/applyEdit" => {
                    let id = format!(
                        "{}-{}",
                        sh.gen_id,
                        sh.next_edit
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    let (tx, rx) = oneshot::channel();
                    let edit = params.get("edit").cloned().unwrap_or(Value::Null);
                    let queued = sh
                        .events
                        .try_send((
                            sh.gen_id,
                            Event::ApplyEdit {
                                id,
                                edit,
                                reply: tx,
                            },
                        ))
                        .is_ok();
                    let applied = queued
                        && tokio::time::timeout(APPLY_EDIT_TIMEOUT, rx)
                            .await
                            .is_ok_and(|r| r.unwrap_or(false));
                    Ok(json!({"applied": applied}))
                }
                "window/showMessageRequest" => Ok(Value::Null),
                "window/showDocument" => Ok(json!({"success": false})),
                _ => Err(RpcError::new(-32601, format!("method not found: {method}"))),
            }
        })
    }

    fn closed(&self) {
        self.shared.send(Event::Exited);
    }
}

/// Spawn `launch` in `root` with its own session (so it never gets our terminal's signals
/// and can be killed as a group), stdin/stdout piped, stderr appended to `log`.
pub fn spawn_process(launch: &Launch, root: &Path, log: &Path) -> Result<Child> {
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)?;
    }
    rotate_log(log);
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .with_context(|| format!("open {}", log.display()))?;
    let mut cmd = Command::new(&launch.program);
    cmd.args(&launch.args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);
    if !launch.path_prepend.is_empty() {
        cmd.env("PATH", crate::install::prepend_path(&launch.path_prepend));
    }
    // SAFETY: setsid is async-signal-safe and runs in the forked child before exec.
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    // CREATE_NO_WINDOW: a console server started from the GUI app must not flash a window.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd.spawn()
        .with_context(|| format!("spawn {}", launch.program.display()))
}

/// Keep one previous generation: `x.log` → `x.log.1` once it passes 2 MiB.
pub fn rotate_log(log: &Path) {
    const MAX: u64 = 2 * 1024 * 1024;
    if std::fs::metadata(log).is_ok_and(|m| m.len() > MAX) {
        let mut old = log.as_os_str().to_owned();
        old.push(".1");
        let _ = std::fs::rename(log, PathBuf::from(old));
    }
}

/// Spawn, connect and run the `initialize` handshake.
pub async fn start(
    launch: &Launch,
    shared: Arc<Shared>,
    log: &Path,
    init_timeout: Duration,
    initialization_options: Value,
) -> Result<Live> {
    let root = PathBuf::from(&shared.root);
    let mut child = spawn_process(launch, &root, log)?;
    let pid = child.id();
    let stdin = child.stdin.take().context("no stdin")?;
    let stdout = child.stdout.take().context("no stdout")?;
    let peer = Peer::spawn(
        stdout,
        stdin,
        Arc::new(ServerHandler {
            shared: shared.clone(),
        }),
    );
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root")
        .to_string();
    let params = caps::initialize_params(&shared.root, &name, initialization_options);
    let child = Arc::new(tokio::sync::Mutex::new(child));
    let res = tokio::time::timeout(init_timeout, peer.request("initialize", params)).await;
    let init = match res {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            kill_group(pid);
            return Err(anyhow!("initialize failed: {e}"));
        }
        Err(_) => {
            kill_group(pid);
            return Err(anyhow!("initialize timed out after {init_timeout:?}"));
        }
    };
    peer.notify("initialized", json!({})).await;
    // Push configuration up front: servers that ignore pull-style config (typescript, yaml)
    // still get our defaults, and ones that pull get a refresh hint.
    peer.notify(
        "workspace/didChangeConfiguration",
        json!({"settings": caps::configuration_with(shared.name, None, shared.settings.as_ref())}),
    )
    .await;
    Ok(Live {
        peer,
        capabilities: init.get("capabilities").cloned().unwrap_or(Value::Null),
        child,
        pid,
        opened: Mutex::default(),
    })
}

/// SIGKILL the whole process group of `pid` (it is a session leader: pgid == pid). The pid
/// came from our own spawn this instant; never a recycled number from a stale file.
pub fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: plain signal to a group we created with setsid.
        #[cfg(unix)]
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
        // Windows has no groups: end the server and every process it started.
        #[cfg(windows)]
        atlas_core::sys::kill_tree(pid);
    }
}

/// `shutdown` + `exit`, then kill the group if it does not leave in [`SHUTDOWN_GRACE`].
pub async fn shutdown(live: &Live) {
    if !live.peer.is_dead() {
        let _ =
            tokio::time::timeout(SHUTDOWN_GRACE, live.peer.request("shutdown", Value::Null)).await;
        live.peer.notify("exit", Value::Null).await;
    }
    let mut child = live.child.lock().await;
    if tokio::time::timeout(SHUTDOWN_GRACE, child.wait())
        .await
        .is_err()
    {
        kill_group(live.pid);
        let _ = child.wait().await;
    } else {
        // Leave nothing behind: helper processes (tsserver) share the group.
        kill_group(live.pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_rotation_keeps_one_generation() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("s.log");
        std::fs::write(&log, vec![b'x'; 3 * 1024 * 1024]).unwrap();
        rotate_log(&log);
        assert!(!log.exists());
        assert!(t.path().join("s.log.1").is_file());
        std::fs::write(&log, "small").unwrap();
        rotate_log(&log);
        assert!(log.is_file());
    }
}
