//! The daemon: singleton lock, socket, per-connection dispatch.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use notify::{RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};

use crate::paths;
use crate::protocol::{self, Hello, HostCmd, MAX_LINE_BYTES, Request};
use crate::pty::{Out, Session};

pub(crate) struct Hub {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    scroll_dir: PathBuf,
    pub(crate) dir: PathBuf,
    started: Instant,
    token: String,
    next_conn: AtomicU64,
    stop: Notify,
    /// Signalled once the successor has taken the ptys: `serve` returns without killing.
    handed_off: Arc<Notify>,
    /// The locked pid file; its fd travels to the successor on upgrade.
    lock: std::fs::File,
    /// Language servers (lazy: nothing runs until a document is opened).
    pub(crate) lsp: atlas_lsp::Manager,
    /// Live connections' outgoing queues: LSP pushes are broadcast to all of them.
    subscribers: Mutex<HashMap<u64, Out>>,
    /// Open LSP documents → the connection that opened them last. A connection that drops
    /// without `lsp_close` has its documents closed, so idle servers can shut down; a document
    /// re-opened by a reconnected client now belongs to the new connection.
    pub(crate) lsp_docs: Mutex<HashMap<String, u64>>,
}

impl Hub {
    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown pty {id}"))
    }

    fn info(&self) -> Value {
        let all = self.sessions.lock();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "proto": protocol::PROTO,
            "caps": protocol::CAPS,
            "pid": std::process::id(),
            "uptime_s": self.started.elapsed().as_secs(),
            "ptys": all.len(),
            "live_ptys": all.values().filter(|s| s.exited().is_none()).count(),
            "hostname": hostname(),
        })
    }

    pub(crate) fn lock_fd(&self) -> std::os::fd::RawFd {
        self.lock.as_raw_fd()
    }

    /// (running ptys, connected clients) for the idle-exit check. A raw-attach connection is
    /// not a subscriber but implies a running pty, so it is covered by the first number.
    fn activity(&self) -> (usize, usize) {
        let live = self
            .sessions
            .lock()
            .values()
            .filter(|s| s.exited().is_none())
            .count();
        (live, self.subscribers.lock().len())
    }

    pub(crate) fn sessions_snapshot(&self) -> Vec<Arc<Session>> {
        self.sessions.lock().values().cloned().collect()
    }

    /// Stop serving after `delay` (the upgrade reply needs to flush first); ptys stay alive.
    pub(crate) fn handed_off_later(&self, delay: std::time::Duration) {
        let n = self.handed_off.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            n.notify_one();
        });
    }

    /// Stop every language server (graceful `shutdown`, then group kill). Called before an
    /// `upgrade` exit, which skips destructors; clients re-open their documents.
    pub(crate) async fn stop_lsp(&self) {
        self.lsp.shutdown().await;
    }

    pub(crate) fn broadcast(&self, line: String) {
        for tx in self.subscribers.lock().values() {
            let _ = tx.send(line.clone());
        }
    }

    async fn kill_all(&self) {
        self.lsp.shutdown().await;
        let mut set = tokio::task::JoinSet::new();
        let all: Vec<_> = self.sessions.lock().values().cloned().collect();
        for s in all {
            set.spawn(async move { s.kill().await });
        }
        set.join_all().await;
    }
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: buffer is valid for its length; gethostname NUL-terminates on success.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return String::new();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// Hold an exclusive `flock` on the pid file for the daemon's lifetime.
///
/// The lock can outlive its daemon: a process that inherited the fd (before received fds were
/// close-on-exec, every child of a daemon that had taken over) keeps it after the daemon dies.
/// Then no hostd is behind the lock and it is orphaned: rather than refusing forever
/// (1.5.0-alpha.8, "another atlas hostd is running" on every start), we move that pid file aside
/// and lock a fresh one. A live atlas hostd named by the pid file is never bypassed.
pub(crate) fn lock_pid(dir: &Path) -> Result<std::fs::File> {
    let path = paths::pid(dir);
    if let Some(f) = try_lock(&path)? {
        return write_pid(f);
    }
    if crate::client::live_hostd_pid(dir).is_some() {
        return Err(another_running(dir));
    }
    // Serialize recoveries: two starters must not both move the file aside and lock a copy each.
    let guard = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("hostd.pid.recover"))?;
    // SAFETY: valid fd owned by `guard`; blocking until the other recovery is done.
    unsafe { libc::flock(guard.as_raw_fd(), libc::LOCK_EX) };
    // Re-check under the guard: an earlier recovery may already hold a fresh pid file.
    if let Some(f) = try_lock(&path)? {
        return write_pid(f);
    }
    if crate::client::live_hostd_pid(dir).is_some() {
        return Err(another_running(dir));
    }
    std::fs::rename(&path, dir.join("hostd.pid.orphaned"))?;
    match try_lock(&path)? {
        Some(f) => write_pid(f),
        None => Err(another_running(dir)),
    }
}

fn another_running(dir: &Path) -> anyhow::Error {
    anyhow!("another atlas hostd is running ({})", dir.display())
}

/// Open (create) `path` and take a non-blocking exclusive `flock`; `None` when it is held.
fn try_lock(path: &Path) -> Result<Option<std::fs::File>> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    // SAFETY: valid fd owned by `f`.
    let held = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0;
    Ok((!held).then_some(f))
}

fn write_pid(f: std::fs::File) -> Result<std::fs::File> {
    use std::io::Write;
    f.set_len(0)?;
    (&f).write_all(std::process::id().to_string().as_bytes())?;
    Ok(f)
}

/// How `serve` starts: fresh, or as the successor of a running daemon.
#[derive(Debug, Clone, Default)]
pub struct ServeOpts {
    /// The old daemon's handoff socket (`atlas hostd --takeover <sock>`).
    pub takeover: Option<PathBuf>,
}

pub async fn serve(dir: &Path) -> Result<()> {
    serve_with(dir, ServeOpts::default()).await
}

pub async fn serve_with(dir: &Path, opts: ServeOpts) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    // Successor: receive the sessions and the pid lock while the old daemon still serves.
    // The lock fd is the old daemon's open file description, so the singleton lock never
    // has a gap. The old daemon stays authoritative until we confirm below.
    let mut takeover = match &opts.takeover {
        Some(sock) => Some(crate::upgrade::receive(sock).await?),
        None => None,
    };
    let lock = match takeover.as_mut().and_then(|t| t.lock.take()) {
        Some(f) => f,
        None => lock_pid(dir)?,
    };
    if takeover.is_some() {
        // Rewrite the pid for `atlas hostd --status` and the desktop (same lock, new owner).
        use std::io::{Seek, Write};
        let mut f = &lock;
        f.set_len(0)?;
        f.rewind()?;
        f.write_all(std::process::id().to_string().as_bytes())?;
    }
    let token = paths::ensure_token(dir)?;
    let sock = paths::socket(dir);
    // Bind beside the live socket and rename over it: the path is never missing, and the
    // old daemon's listener just stops receiving new connections.
    let tmp_sock = dir.join(format!("hostd.sock.{}", std::process::id()));
    let listener = if takeover.is_some() {
        let _ = std::fs::remove_file(&tmp_sock);
        let l = UnixListener::bind(&tmp_sock)
            .with_context(|| format!("bind {}", tmp_sock.display()))?;
        std::fs::set_permissions(&tmp_sock, std::fs::Permissions::from_mode(0o600))?;
        std::fs::rename(&tmp_sock, &sock)?;
        l
    } else {
        if sock.exists() {
            if UnixStream::connect(&sock).await.is_ok() {
                return Err(anyhow!("socket {} is in use", sock.display()));
            }
            std::fs::remove_file(&sock)?;
        }
        let l = UnixListener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?;
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
        l
    };

    let scroll_dir = paths::scrollback_dir(dir);
    std::fs::create_dir_all(&scroll_dir)?;
    let mut sessions = HashMap::new();
    let mut adopted = 0;
    for (st, fd) in takeover
        .as_mut()
        .map(|t| std::mem::take(&mut t.sessions))
        .into_iter()
        .flatten()
    {
        let id = st.id.clone();
        match Session::adopt(st, fd, &scroll_dir) {
            Ok(s) => {
                adopted += 1;
                sessions.insert(id, s);
            }
            Err(e) => eprintln!("hostd: could not adopt pty {id}: {e:#}"),
        }
    }
    for e in std::fs::read_dir(&scroll_dir)?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "bin")
            && let Some(id) = p.file_stem().and_then(|s| s.to_str())
            && !sessions.contains_key(id)
            && let Some(s) = Session::cold(id, &scroll_dir)
        {
            sessions.insert(id.to_string(), s);
        }
    }
    let (lsp_tx, lsp_rx) = mpsc::channel(512);
    let hub = Arc::new(Hub {
        sessions: Mutex::new(sessions),
        scroll_dir,
        dir: dir.to_path_buf(),
        started: Instant::now(),
        token,
        next_conn: AtomicU64::new(1),
        stop: Notify::new(),
        handed_off: Arc::new(Notify::new()),
        lock,
        lsp: atlas_lsp::Manager::new(dir, atlas_lsp::Config::default(), lsp_tx),
        subscribers: Mutex::new(HashMap::new()),
        lsp_docs: Mutex::new(HashMap::new()),
    });
    tokio::spawn(crate::lsp::pump(Arc::downgrade(&hub), lsp_rx));
    eprintln!(
        "atlas hostd {} listening on {} (pid {}{})",
        env!("CARGO_PKG_VERSION"),
        sock.display(),
        std::process::id(),
        if opts.takeover.is_some() {
            format!(", took over {adopted} pty(s)")
        } else {
            String::new()
        }
    );

    if let Some(t) = takeover {
        // Everything is adopted and bound: the old daemon may exit now.
        t.confirm()?;
    }
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let limit = crate::idle::limit_from_env();
    let mut ticker = tokio::time::interval(crate::idle::tick(limit));
    let mut idle_since: Option<Instant> = None;
    loop {
        tokio::select! {
            acc = listener.accept() => {
                if let Ok((stream, _)) = acc {
                    let hub = hub.clone();
                    tokio::spawn(async move { conn(hub, stream).await });
                }
            }
            _ = hub.stop.notified() => break,
            _ = term.recv() => break,
            _ = ticker.tick() => {
                let (live, clients) = hub.activity();
                if live > 0 || clients > 0 {
                    idle_since = None;
                } else {
                    let since = *idle_since.get_or_insert_with(Instant::now);
                    if crate::idle::idle_expired(live, clients, since.elapsed(), limit) {
                        eprintln!("atlas hostd: idle for {}s, no terminals and no clients: exiting", limit.as_secs());
                        break;
                    }
                }
                if crate::idle::dir_gone(dir) {
                    eprintln!("atlas hostd: {} is gone: exiting", dir.display());
                    break;
                }
            }
            _ = hub.handed_off.notified() => {
                // The successor owns the ptys now: leave them running. The pid lock and
                // socket path are released by exiting (the successor re-binds them). Exit
                // explicitly: open client connections and blocked reader threads must not
                // keep a retired daemon alive, and nothing here may signal the children.
                drop(listener);
                hub.stop_lsp().await;
                std::process::exit(0);
            }
        }
    }
    hub.kill_all().await;
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(paths::pid(dir));
    Ok(())
}

async fn conn(hub: Arc<Hub>, stream: UnixStream) {
    let conn_id = hub.next_conn.fetch_add(1, Ordering::Relaxed);
    let (r, w) = stream.into_split();
    let mut lines = BufReader::new(r);
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    // Returns the write half so `pty_attach_raw` can take the socket over.
    let writer = tokio::spawn(protocol::write_lines(rx, w));

    let mut buf = String::new();
    let authed = match read_line(&mut lines, &mut buf).await {
        Some(l) => serde_json::from_str::<Value>(&l)
            .ok()
            .and_then(|v| serde_json::from_value::<Hello>(v.get("hello")?.clone()).ok())
            .is_some_and(|h| h.token == hub.token),
        None => false,
    };
    if !authed {
        let _ = tx.send(json!({"ok": false, "error": "unauthorized"}).to_string());
        drop(tx);
        let _ = writer.await;
        return;
    }
    let _ = tx.send(json!({"ok": true, "data": hub.info()}).to_string());
    hub.subscribers.lock().insert(conn_id, tx.clone());

    let mut watchers: HashMap<String, notify::RecommendedWatcher> = HashMap::new();
    let mut attached: Vec<Arc<Session>> = Vec::new();
    while let Some(line) = read_line(&mut lines, &mut buf).await {
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let id = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v.get("id")?.as_u64())
                    .unwrap_or(0);
                let _ = tx.send(protocol::reply(id, Err(format!("bad request: {e}"))));
                continue;
            }
        };
        let id = req.id;
        let res = match req.cmd {
            HostCmd::FsWatch { path, id: wid } => watch(&tx, &mut watchers, path, wid),
            HostCmd::FsUnwatch { id: wid } => {
                watchers.remove(&wid);
                Ok(json!({}))
            }
            HostCmd::PtyAttach {
                pty,
                replay,
                output,
            } => hub.get(&pty).map(|s| {
                let info = s.attach(conn_id, tx.clone(), replay, output);
                attached.push(s);
                info
            }),
            HostCmd::PtyAttachRaw { pty, replay } => {
                if !attached.is_empty() || !watchers.is_empty() {
                    Err("pty_attach_raw needs a fresh connection".into())
                } else {
                    match hub.get(&pty) {
                        Ok(s) => {
                            // A raw byte stream cannot carry JSON pushes.
                            hub.subscribers.lock().remove(&conn_id);
                            return raw(s, conn_id, replay, id, tx, writer, lines, buf).await;
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            HostCmd::Shutdown {} => {
                let _ = tx.send(protocol::reply(id, Ok(json!({}))));
                hub.stop.notify_one();
                break;
            }
            cmd => {
                // Slow commands (exec, kill) must not block this connection's other requests.
                let hub = hub.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(protocol::reply(id, handle(&hub, cmd, conn_id).await));
                });
                continue;
            }
        };
        let _ = tx.send(protocol::reply(id, res));
    }
    for s in attached {
        s.detach(conn_id);
    }
    hub.subscribers.lock().remove(&conn_id);
    crate::lsp::connection_closed(&hub, conn_id).await;
    drop(tx);
    let _ = writer.await;
}

async fn read_line(
    r: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    buf: &mut String,
) -> Option<String> {
    buf.clear();
    match r.read_line(buf).await {
        Ok(0) | Err(_) => None,
        Ok(n) if n > MAX_LINE_BYTES => Some(String::new()),
        Ok(_) => Some(buf.trim_end().to_string()),
    }
}

fn watch(
    tx: &Out,
    watchers: &mut HashMap<String, notify::RecommendedWatcher>,
    path: String,
    wid: String,
) -> Result<Value, String> {
    let (etx, mut erx) = mpsc::unbounded_channel::<(String, String)>();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            use notify::EventKind::*;
            let kind = match ev.kind {
                Create(_) => "create",
                Remove(_) => "remove",
                Modify(_) => "modify",
                _ => return,
            };
            for p in ev.paths {
                let _ = etx.send((p.to_string_lossy().into_owned(), kind.into()));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    w.watch(Path::new(&path), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    let out = tx.clone();
    let id = wid.clone();
    tokio::spawn(async move {
        while let Some(first) = erx.recv().await {
            let mut batch = vec![first];
            tokio::time::sleep(crate::fs::WATCH_DEBOUNCE).await;
            while let Ok(e) = erx.try_recv() {
                batch.push(e);
            }
            let mut by_kind: HashMap<String, Vec<String>> = HashMap::new();
            for (p, k) in batch {
                let v = by_kind.entry(k).or_default();
                if !v.contains(&p) {
                    v.push(p);
                }
            }
            for (kind, paths) in by_kind {
                let line = json!({"ev": "fs_event", "id": id, "paths": paths, "kind": kind});
                if out.send(line.to_string()).is_err() {
                    return;
                }
            }
        }
    });
    watchers.insert(wid, w);
    Ok(json!({}))
}

async fn handle(hub: &Hub, cmd: HostCmd, conn_id: u64) -> Result<Value, String> {
    let e = |e: anyhow::Error| format!("{e:#}");
    match cmd {
        HostCmd::PtyCreate(req) => {
            let s = Session::spawn(req, &hub.scroll_dir).map_err(e)?;
            let id = s.id.clone();
            hub.sessions.lock().insert(id.clone(), s);
            Ok(json!({"pty": id}))
        }
        HostCmd::PtyDetach { pty } => {
            hub.get(&pty)?.detach(conn_id);
            Ok(json!({}))
        }
        HostCmd::PtyWrite { pty, data } => {
            let bytes = B64.decode(data).map_err(|x| x.to_string())?;
            hub.get(&pty)?.write(&bytes).map_err(e)?;
            Ok(json!({}))
        }
        HostCmd::PtyResize { pty, cols, rows } => {
            hub.get(&pty)?.resize(cols, rows).map_err(e)?;
            Ok(json!({}))
        }
        HostCmd::PtySignal { pty, signal } => {
            hub.get(&pty)?.signal(&signal).map_err(e)?;
            Ok(json!({}))
        }
        HostCmd::PtyKill { pty } => {
            let s = hub.get(&pty)?;
            s.kill().await;
            hub.sessions.lock().remove(&pty);
            s.remove_files();
            Ok(json!({}))
        }
        HostCmd::PtyList {} => {
            let mut all: Vec<_> = hub.sessions.lock().values().cloned().collect();
            all.sort_by_key(|s| s.meta.created_at);
            Ok(json!({"ptys": all.iter().map(|s| s.summary()).collect::<Vec<_>>()}))
        }
        HostCmd::PtyInfo { pty } => {
            let s = hub.get(&pty)?;
            let mut v = s.summary();
            v["cwd_now"] = json!(s.cwd_now().await);
            Ok(v)
        }
        HostCmd::PtyRead { pty, lines } => Ok(json!({"text": hub.get(&pty)?.read_text(lines)})),
        HostCmd::PtyClear { pty } => {
            hub.get(&pty)?.clear();
            Ok(json!({}))
        }
        HostCmd::Exec {
            cwd,
            argv,
            env,
            stdin,
            timeout_ms,
        } => crate::exec::run(&cwd, &argv, &env, stdin.as_deref(), timeout_ms).await,
        HostCmd::FsList { path } => crate::fs::list(&path),
        HostCmd::FsRead { path, max, offset } => crate::fs::read(&path, max, offset),
        HostCmd::FsWrite { path, data, mode } => crate::fs::write(&path, &data, mode),
        HostCmd::FsStat { path } => crate::fs::stat(&path),
        HostCmd::FsMkdir { path } => crate::fs::mkdir(&path),
        HostCmd::FsRename { from, to } => crate::fs::rename(&from, &to),
        HostCmd::FsRemove { path, recursive } => crate::fs::remove(&path, recursive),
        HostCmd::Info {} => Ok(hub.info()),
        HostCmd::Upgrade { exe } => crate::upgrade::run(hub, &exe).await,
        lsp @ (HostCmd::LspOpen { .. }
        | HostCmd::LspChange { .. }
        | HostCmd::LspClose { .. }
        | HostCmd::LspSave { .. }
        | HostCmd::LspRequest { .. }
        | HostCmd::LspCancel { .. }
        | HostCmd::LspStatus {}
        | HostCmd::LspCatalog {}
        | HostCmd::LspInstall { .. }
        | HostCmd::LspUninstall { .. }
        | HostCmd::LspRestart { .. }
        | HostCmd::LspLog { .. }
        | HostCmd::LspApplyEditResult { .. }) => crate::lsp::handle(hub, lsp, conn_id)
            .await
            .unwrap_or_else(|| Err("not an lsp command".into())),
        HostCmd::PtyAttach { .. }
        | HostCmd::PtyAttachRaw { .. }
        | HostCmd::FsWatch { .. }
        | HostCmd::FsUnwatch { .. }
        | HostCmd::Shutdown {} => Err("handled by connection".into()),
    }
}

/// Serve a `pty_attach_raw` connection: reply line, then raw bytes until the pty
/// exits or the client hangs up.
#[allow(clippy::too_many_arguments)]
async fn raw(
    s: Arc<Session>,
    conn_id: u64,
    replay: bool,
    id: u64,
    tx: Out,
    writer: tokio::task::JoinHandle<tokio::net::unix::OwnedWriteHalf>,
    mut lines: BufReader<tokio::net::unix::OwnedReadHalf>,
    mut buf: String,
) {
    let (rtx, mut rrx) = mpsc::channel::<Vec<u8>>(crate::pty::RAW_QUEUE);
    // Subscribe before replying: output produced meanwhile queues in `rrx`.
    let info = s.attach_raw(conn_id, rtx, replay);
    let _ = tx.send(protocol::reply(id, Ok(info)));
    drop(tx);
    // In-flight slow commands hold `tx` clones; the writer ends after their replies.
    let Ok(mut w) = writer.await else {
        s.detach(conn_id);
        return;
    };
    loop {
        tokio::select! {
            chunk = rrx.recv() => match chunk {
                Some(c) => if w.write_all(&c).await.is_err() { break },
                None => break,
            },
            // The client sends nothing on a raw connection: any line or EOF ends it.
            _ = read_line(&mut lines, &mut buf) => break,
        }
    }
    s.detach(conn_id);
    let _ = w.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Another open file description holding the `flock`, like a process that inherited the
    /// lock fd from a daemon that is gone.
    fn hold(path: &Path) -> std::fs::File {
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();
        // SAFETY: valid fd owned by `f`.
        assert_eq!(
            unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        f
    }

    fn dead_pid() -> u32 {
        let mut c = std::process::Command::new("true").spawn().unwrap();
        let pid = c.id();
        c.wait().unwrap();
        pid
    }

    /// Regression (1.5.0-alpha.8): a dead hostd's lock still held by a process that inherited
    /// the fd (a language server started after a takeover) made every start fail with "another
    /// atlas hostd is running", forever, and Restart went through the same path. With no live
    /// hostd behind it, the lock is orphaned: the new daemon takes a fresh pid file.
    #[test]
    fn a_lock_held_without_a_live_hostd_is_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let _orphan = hold(&paths::pid(dir.path()));
        std::fs::write(paths::pid(dir.path()), dead_pid().to_string()).unwrap();
        let _lock = lock_pid(dir.path()).expect("orphaned lock is recovered");
        let written = std::fs::read_to_string(paths::pid(dir.path())).unwrap();
        assert_eq!(written, std::process::id().to_string());
    }

    /// The pid file can name a recycled pid that now belongs to an unrelated live process.
    #[test]
    fn a_lock_whose_pid_is_not_a_hostd_is_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let _orphan = hold(&paths::pid(dir.path()));
        std::fs::write(paths::pid(dir.path()), std::process::id().to_string()).unwrap();
        assert!(lock_pid(dir.path()).is_ok());
    }

    /// Recovery never yields two holders: the first recovery's lock stays exclusive.
    #[test]
    fn a_second_start_after_a_recovery_is_refused_while_the_first_holds() {
        let dir = tempfile::tempdir().unwrap();
        let _orphan = hold(&paths::pid(dir.path()));
        std::fs::write(paths::pid(dir.path()), dead_pid().to_string()).unwrap();
        let _first = lock_pid(dir.path()).unwrap();
        // What a second daemon's own flock would do on the pid file now in place.
        let second = hold_attempt(dir.path());
        assert!(second.is_err(), "the recovered lock must stay exclusive");
    }

    /// A plain flock attempt on the current pid file, as a second daemon's would start.
    fn hold_attempt(dir: &Path) -> Result<std::fs::File> {
        let f = std::fs::File::open(paths::pid(dir))?;
        // SAFETY: valid fd owned by `f`.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(anyhow!("held"));
        }
        Ok(f)
    }
}
