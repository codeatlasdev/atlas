//! Async client for hostd, used by Atlas Desktop and the CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use atlas_core::ipc;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::paths;

/// A pushed event: `{"ev": "...", ...}`.
pub type HostEvent = Arc<Value>;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Clone)]
pub struct HostClient {
    tx: mpsc::UnboundedSender<String>,
    pending: Pending,
    next: Arc<AtomicU64>,
    events: broadcast::Sender<HostEvent>,
    closed: Arc<tokio::sync::Notify>,
}

impl HostClient {
    pub async fn connect(socket: &Path, token: &str, client: &str) -> Result<Self> {
        let stream = ipc::connect(socket)
            .await
            .with_context(|| format!("connect {}", socket.display()))?;
        let (r, mut w) = ipc::split(stream);
        let mut lines = BufReader::new(r).lines();
        let hello = crate::protocol::hello_line(token, client);
        w.write_all(format!("{hello}\n").as_bytes()).await?;
        let first = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow!("hostd closed the connection"))?;
        let v: Value = serde_json::from_str(&first)?;
        if v["ok"] != true {
            return Err(anyhow!(
                "hostd: {}",
                v["error"].as_str().unwrap_or("handshake failed")
            ));
        }

        let (tx, rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(crate::protocol::write_lines(rx, w));
        let pending: Pending = Arc::default();
        let (events, _) = broadcast::channel(4096);
        let closed = Arc::new(tokio::sync::Notify::new());
        let (p, ev, cl) = (pending.clone(), events.clone(), closed.clone());
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = p.lock().remove(&id) {
                        let res = if v["ok"] == true {
                            Ok(v.get("data").cloned().unwrap_or(Value::Null))
                        } else {
                            Err(v["error"].as_str().unwrap_or("error").to_string())
                        };
                        let _ = tx.send(res);
                    }
                } else if v.get("ev").is_some() {
                    let _ = ev.send(Arc::new(v));
                }
            }
            p.lock().clear();
            cl.notify_waiters();
            let _ = ev.send(Arc::new(json!({"ev": "disconnected"})));
        });
        Ok(Self {
            tx,
            pending,
            next: Arc::new(AtomicU64::new(1)),
            events,
            closed,
        })
    }

    /// Send a command and wait for its reply.
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (otx, orx) = oneshot::channel();
        self.pending.lock().insert(id, otx);
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string();
        if self.tx.send(line).is_err() {
            self.pending.lock().remove(&id);
            return Err(anyhow!("hostd disconnected"));
        }
        match orx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(anyhow!(e)),
            Err(_) => Err(anyhow!("hostd disconnected")),
        }
    }

    pub fn events(&self) -> broadcast::Receiver<HostEvent> {
        self.events.subscribe()
    }

    /// Resolves when the connection drops.
    pub async fn closed(&self) {
        if self.tx.is_closed() {
            return;
        }
        self.closed.notified().await
    }

    /// Connect to the hostd at `dir` (default `~/.atlas`).
    pub async fn connect_dir(dir: &Path, client: &str) -> Result<Self> {
        let token = paths::read_token(dir).context("hostd token")?;
        Self::connect(&paths::socket(dir), &token, client).await
    }
}

/// The `atlas` binary used to start hostd.
pub fn atlas_bin() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ATLAS_BIN") {
        return Ok(p.into());
    }
    let home = dirs::home_dir()
        .unwrap_or_default()
        .join(".atlas/bin")
        .join(atlas_core::paths::exe("atlas"));
    if home.exists() {
        return Ok(home);
    }
    if let Some(cli) = atlas_core::paths::bundled_cli() {
        return Ok(cli);
    }
    let names = atlas_core::paths::exe_names("atlas");
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .flat_map(|d| names.iter().map(move |n| d.join(n)))
                .find(|p| p.is_file())
        })
        .ok_or_else(|| anyhow!("atlas CLI not found; run atlas desktop --install"))
}

/// Connect to the local hostd, starting it (detached, own session) if needed.
pub async fn ensure_local(atlas: &Path, client: &str) -> Result<HostClient> {
    ensure_in(&paths::base(), atlas, client).await
}

/// [`ensure_local`] for the hostd at `dir`.
///
/// A daemon that holds the singleton lock but cannot be talked to (its token file was replaced,
/// it is wedged) would make every spawn below die with "another atlas hostd is running", so
/// the caller never connected (1.5.0-alpha.5). [`retire_unusable`] ends exactly that daemon first.
pub async fn ensure_in(dir: &Path, atlas: &Path, client: &str) -> Result<HostClient> {
    if let Ok(c) = HostClient::connect_dir(dir, client).await {
        return Ok(c);
    }
    retire_unusable(dir).await;
    std::fs::create_dir_all(dir)?;
    let log = paths::open_log(dir)?;
    let mut cmd = std::process::Command::new(atlas);
    cmd.arg("hostd")
        .env(paths::DIR_ENV, dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // Detach hostd from our session so it survives the app quitting.
    let mut child = atlas_core::sys::spawn_detached(&mut cmd)
        .with_context(|| format!("spawn {}", atlas.display()))?;
    std::thread::spawn(move || child.wait());
    let mut last = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match HostClient::connect_dir(dir, client).await {
            Ok(c) => return Ok(c),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("hostd did not start")))
        .with_context(|| format!("see {}", paths::log(dir).display()))
}

/// The pid in `hostd.pid` when that process is an `atlas ... hostd` (never any other process:
/// a stale file can name a recycled pid). `None` when there is no file or it is not a hostd.
pub fn live_hostd_pid(dir: &Path) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(paths::pid(dir))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let cmd = atlas_core::sys::command_line(pid)?;
    is_hostd_command(&cmd).then_some(pid)
}

/// Is `cmd` (a process's command line) `atlas hostd …`? The program is matched by its file name
/// (`atlas`, `atlas.exe` on Windows, where a path with spaces arrives quoted).
fn is_hostd_command(cmd: &str) -> bool {
    let cmd = cmd.trim_start();
    let (exe, rest) = match cmd.strip_prefix('"') {
        Some(q) => q.split_once('"').unwrap_or((q, "")),
        None => cmd.split_once(char::is_whitespace).unwrap_or((cmd, "")),
    };
    let name = Path::new(exe)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    atlas_core::sys::is_program(&name, "atlas") && rest.split_whitespace().next() == Some("hostd")
}

/// The pid of a live atlas hostd in `dir` that this client cannot use (it holds the lock but
/// `connect_dir` fails): what `ensure_in` is about to retire. `None` when nothing runs or it answers.
pub async fn unusable_hostd_pid(dir: &Path) -> Option<u32> {
    let pid = live_hostd_pid(dir)?;
    HostClient::connect_dir(dir, "probe")
        .await
        .err()
        .map(|_| pid)
}

/// The daemon of `dir` is alive but `connect_dir` fails: ask it to stop with `SIGTERM` (its
/// handler ends its terminals cleanly) and wait for its pid lock to free. Exact pid only, and
/// only after [`live_hostd_pid`] confirmed it is an atlas hostd; never a port, never a name.
async fn retire_unusable(dir: &Path) {
    let Some(pid) = live_hostd_pid(dir) else {
        return;
    };
    // `pid` was just verified to be this dir's atlas hostd. Windows ends it outright: its
    // terminals' pseudo consoles close with it.
    atlas_core::sys::terminate(pid);
    for _ in 0..50 {
        if !atlas_core::sys::pid_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A `pty_attach_raw` connection: `read` yields the pty's raw output (replay first);
/// EOF when the pty exits. Dropping it detaches.
pub struct RawPty {
    pub info: Value,
    pub reader: BufReader<ipc::ReadHalf>,
    /// Kept open: closing the write half tells hostd to detach.
    _w: ipc::WriteHalf,
}

/// Open a dedicated raw byte stream for `pty` on the hostd at `dir`.
pub async fn attach_raw(dir: &Path, pty: &str, replay: bool) -> Result<RawPty> {
    let token = paths::read_token(dir).context("hostd token")?;
    let stream = ipc::connect(&paths::socket(dir)).await?;
    let (r, mut w) = ipc::split(stream);
    let mut reader = BufReader::new(r);
    let hello = crate::protocol::hello_line(&token, "raw");
    let req = json!({"id": 1, "cmd": "pty_attach_raw", "args": {"pty": pty, "replay": replay}});
    w.write_all(format!("{hello}\n{req}\n").as_bytes()).await?;
    let _hello = reply_line(&mut reader).await?;
    Ok(RawPty {
        info: reply_line(&mut reader).await?,
        reader,
        _w: w,
    })
}

/// One `{"ok":..}` line from hostd, as its `data`.
async fn reply_line(r: &mut BufReader<ipc::ReadHalf>) -> Result<Value> {
    let mut line = String::new();
    if r.read_line(&mut line).await? == 0 {
        return Err(anyhow!("hostd closed the connection"));
    }
    let v: Value = serde_json::from_str(&line)?;
    if v["ok"] != true {
        return Err(anyhow!("hostd: {}", v["error"].as_str().unwrap_or("error")));
    }
    Ok(v.get("data").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::is_hostd_command;

    #[test]
    fn only_an_atlas_hostd_command_line_is_recognised() {
        assert!(is_hostd_command("/Users/u/.atlas/bin/atlas hostd"));
        assert!(is_hostd_command("atlas hostd --dir /tmp/x"));
        assert!(!is_hostd_command("/usr/bin/atlas-mcp hostd"));
        assert!(!is_hostd_command("/Users/u/.atlas/bin/atlas mcp"));
        assert!(!is_hostd_command("/bin/zsh -l"));
        assert!(!is_hostd_command(""));
        // Windows: a quoted program path with spaces, and the `.exe` suffix.
        assert_eq!(
            is_hostd_command(r#""C:\Users\Jane Doe\.atlas\bin\atlas.exe" hostd"#),
            cfg!(windows)
        );
        assert_eq!(
            is_hostd_command(r"C:\Users\u\.atlas\bin\ATLAS.EXE hostd"),
            cfg!(windows)
        );
        assert!(!is_hostd_command(r#""C:\Program Files\x\atlas.exe" mcp"#));
    }
}
