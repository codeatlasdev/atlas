//! Async client for hostd, used by Atlas Desktop and the CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
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
        let stream = UnixStream::connect(socket)
            .await
            .with_context(|| format!("connect {}", socket.display()))?;
        let (r, mut w) = stream.into_split();
        let mut lines = BufReader::new(r).lines();
        let hello =
            json!({"hello": {"token": token, "client": client, "proto": crate::protocol::PROTO}});
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

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(mut l) = rx.recv().await {
                l.push('\n');
                if w.write_all(l.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
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
        .join(".atlas/bin/atlas");
    if home.exists() {
        return Ok(home);
    }
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|d| d.join("atlas"))
                .find(|p| p.is_file())
        })
        .ok_or_else(|| anyhow!("atlas CLI not found; run atlas desktop --install"))
}

/// Connect to the local hostd, starting it (detached, own session) if needed.
pub async fn ensure_local(atlas: &Path, client: &str) -> Result<HostClient> {
    let dir = paths::base();
    if let Ok(c) = HostClient::connect_dir(&dir, client).await {
        return Ok(c);
    }
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log(&dir))?;
    let mut cmd = std::process::Command::new(atlas);
    cmd.arg("hostd")
        .env(paths::DIR_ENV, &dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: setsid is async-signal-safe; detaches hostd from our session so it
    // survives the app quitting.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn {}", atlas.display()))?;
    std::thread::spawn(move || child.wait());
    let mut last = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match HostClient::connect_dir(&dir, client).await {
            Ok(c) => return Ok(c),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("hostd did not start")))
        .with_context(|| format!("see {}", paths::log(&dir).display()))
}

/// A `pty_attach_raw` connection: `read` yields the pty's raw output (replay first);
/// EOF when the pty exits. Dropping it detaches.
pub struct RawPty {
    pub info: Value,
    pub reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    /// Kept open: closing the write half tells hostd to detach.
    _w: tokio::net::unix::OwnedWriteHalf,
}

/// Open a dedicated raw byte stream for `pty` on the hostd at `dir`.
pub async fn attach_raw(dir: &Path, pty: &str, replay: bool) -> Result<RawPty> {
    let token = paths::read_token(dir).context("hostd token")?;
    let stream = UnixStream::connect(paths::socket(dir)).await?;
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);
    let hello =
        json!({"hello": {"token": token, "client": "raw", "proto": crate::protocol::PROTO}});
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
async fn reply_line(r: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Result<Value> {
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
