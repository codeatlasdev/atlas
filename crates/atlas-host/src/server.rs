//! The daemon: singleton lock, socket, per-connection dispatch.

use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use atlas_core::ipc;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use notify::{RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Notify, mpsc};

use crate::paths;
use crate::protocol::{self, Hello, HostCmd, MAX_LINE_BYTES, Request};
use crate::pty::{Out, Session};

pub(crate) struct Hub {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    scroll_dir: PathBuf,
    /// Read by the Unix live upgrade only.
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) dir: PathBuf,
    started: Instant,
    token: String,
    next_conn: AtomicU64,
    stop: Notify,
    /// Signalled once the successor has taken the ptys: `serve` returns without killing.
    handed_off: Arc<Notify>,
    /// The locked pid file; its fd travels to the successor on upgrade. Held for the lock
    /// alone on Windows.
    #[cfg_attr(windows, allow(dead_code))]
    lock: std::fs::File,
    /// Language servers (lazy: nothing runs until a document is opened).
    pub(crate) lsp: atlas_lsp::Manager,
    /// Live connections' outgoing queues: LSP pushes are broadcast to all of them.
    subscribers: Mutex<HashMap<u64, Out>>,
    /// Open LSP documents → the connection that opened them last. A connection that drops
    /// without `lsp_close` has its documents closed, so idle servers can shut down; a document
    /// re-opened by a reconnected client now belongs to the new connection.
    pub(crate) lsp_docs: Mutex<HashMap<String, u64>>,
    /// Coding-agent state per pane (hook-fed); changes are pushed as `agent_status`.
    pub(crate) agents: Arc<crate::agents::AgentHub>,
    /// Atlas Mobile: identity, paired phones, pairing codes (`crate::mobile`).
    pub(crate) mobile: Arc<crate::mobile::Mobile>,
    /// `Some` when this is the lab lane's shadow daemon (`atlas hostd --lab`): it mirrors and
    /// forwards to the main hostd instead of owning terminals (`crate::lab`).
    pub(crate) lab: Option<Arc<crate::lab::Lab>>,
}

impl Hub {
    /// Where the desktop's project registry lives: this daemon's dir, or, in a lab, the main
    /// daemon's (read only).
    pub(crate) fn data_dir(&self) -> &Path {
        self.lab.as_ref().map_or(&self.dir, |l| l.main_dir())
    }

    pub(crate) fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown pty {id}"))
    }

    /// Pending output per live JSON connection plus how many were dropped for not reading, so a
    /// client that stopped reading shows up here before [`crate::pty::OUT_CAP`] cuts it.
    fn queued(&self) -> Value {
        let mut conns: Vec<(u64, usize)> = self
            .subscribers
            .lock()
            .iter()
            .map(|(id, out)| (*id, out.pending()))
            .collect();
        conns.sort_unstable();
        json!({
            "total_bytes": conns.iter().map(|(_, n)| n).sum::<usize>(),
            "cap_bytes": crate::pty::OUT_CAP,
            "dropped": crate::pty::dropped_total(),
            "connections": conns
                .iter()
                .map(|(id, n)| json!({"conn": id, "bytes": n}))
                .collect::<Vec<_>>(),
        })
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
            // Connected clients (desktop, CLI, phones; a lab's phone link counts on main).
            "clients": self.subscribers.lock().len(),
            // Set on a lab lane daemon: the main hostd dir it follows.
            "lab": self.lab.as_ref().map(|l| l.main_dir().to_string_lossy().into_owned()),
            "hostname": hostname(),
            "queued": self.queued(),
        })
    }

    #[cfg(unix)]
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
    #[cfg(unix)]
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

    /// Register a push receiver (mobile sessions); returns its connection id.
    pub(crate) fn add_subscriber(&self, tx: Out) -> u64 {
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed);
        self.subscribers.lock().insert(id, tx);
        id
    }

    pub(crate) fn remove_subscriber(&self, id: u64) {
        self.subscribers.lock().remove(&id);
    }

    /// Push `line` to every connection. One over its byte cap (it stopped reading) misses this
    /// line: a stuck client loses broadcasts (it re-syncs on reconnect) instead of hostd queueing
    /// them for it. Nothing here blocks, and the other connections are untouched.
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

pub(crate) fn hostname() -> String {
    atlas_core::sys::hostname().unwrap_or_default()
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
    recover_orphaned_lock(dir, &path)
}

/// Unix: the lock fd can be inherited by a process that outlives its daemon, so a held lock with no live
/// hostd is orphaned: move the pid file aside and lock a fresh one.
#[cfg(unix)]
fn recover_orphaned_lock(dir: &Path, path: &Path) -> Result<std::fs::File> {
    // Serialize recoveries: two starters must not both move the file aside and lock a copy each.
    let guard = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("hostd.pid.recover"))?;
    // SAFETY: valid fd owned by `guard`; blocking until the other recovery is done.
    unsafe { libc::flock(guard.as_raw_fd(), libc::LOCK_EX) };
    // Re-check under the guard: an earlier recovery may already hold a fresh pid file.
    if let Some(f) = try_lock(path)? {
        return write_pid(f);
    }
    if crate::client::live_hostd_pid(dir).is_some() {
        return Err(another_running(dir));
    }
    std::fs::rename(path, dir.join("hostd.pid.orphaned"))?;
    match try_lock(path)? {
        Some(f) => write_pid(f),
        None => Err(another_running(dir)),
    }
}

/// Windows: a locked file cannot be moved aside, so there is no recovery: a held lock is reported as a running hostd.
#[cfg(windows)]
fn recover_orphaned_lock(dir: &Path, _path: &Path) -> Result<std::fs::File> {
    Err(another_running(dir))
}

fn another_running(dir: &Path) -> anyhow::Error {
    anyhow!("another atlas hostd is running ({})", dir.display())
}

/// Open (create) `path` and take a non-blocking exclusive lock (`flock`, `LockFileEx`); `None` when it is held.
fn try_lock(path: &Path) -> Result<Option<std::fs::File>> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    Ok(atlas_core::sys::try_lock(&f)?.then_some(f))
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
    /// Atlas Mobile without n0's public relays: direct addresses only (tests, a
    /// phone on the same network). `ATLAS_MOBILE_RELAY=0` for `atlas hostd`.
    pub no_mobile_relay: bool,
    /// Run as the lab lane's shadow daemon following the main hostd in this directory
    /// (`atlas hostd --lab`, see [`crate::lab`]). `serve_with`'s `dir` is then the lab's own.
    pub lab: Option<PathBuf>,
}

pub async fn serve(dir: &Path) -> Result<()> {
    serve_with(dir, ServeOpts::default()).await
}

pub async fn serve_with(dir: &Path, opts: ServeOpts) -> Result<()> {
    // A lab checks everything it depends on before it creates a file: it follows a main hostd
    // that must be there, in a directory that is not its own.
    let lab = match &opts.lab {
        Some(_) if opts.takeover.is_some() => {
            return Err(anyhow!(
                "a lab hostd owns no terminals: it cannot take over a daemon"
            ));
        }
        Some(main) => Some(crate::lab::connect(main, dir).await?),
        None => None,
    };
    std::fs::create_dir_all(dir)?;
    // Successor: receive the sessions and the pid lock while the old daemon still serves.
    // The lock fd is the old daemon's open file description, so the singleton lock never
    // has a gap. The old daemon stays authoritative until we confirm below.
    #[cfg(unix)]
    let mut takeover = match &opts.takeover {
        Some(sock) => Some(crate::upgrade::receive(sock).await?),
        None => None,
    };
    #[cfg(windows)]
    let mut takeover: Option<crate::upgrade::Takeover> = match &opts.takeover {
        Some(_) => return Err(anyhow!("live takeover is not supported on Windows")),
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
    #[cfg(unix)]
    let listener = if takeover.is_some() {
        let _ = std::fs::remove_file(&tmp_sock);
        let l = ipc::Listener::bind(&tmp_sock)
            .with_context(|| format!("bind {}", tmp_sock.display()))?;
        std::fs::set_permissions(&tmp_sock, std::fs::Permissions::from_mode(0o600))?;
        std::fs::rename(&tmp_sock, &sock)?;
        l
    } else {
        if sock.exists() {
            if ipc::connect(&sock).await.is_ok() {
                return Err(anyhow!("socket {} is in use", sock.display()));
            }
            std::fs::remove_file(&sock)?;
        }
        let l = ipc::Listener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?;
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
        l
    };
    // A named pipe (see `atlas_core::ipc`): no file to clean up, and a second bind fails while
    // another daemon serves the path (the pid lock above already rules that out).
    #[cfg(windows)]
    let listener = {
        let _ = &tmp_sock;
        ipc::Listener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?
    };

    let scroll_dir = paths::scrollback_dir(dir);
    std::fs::create_dir_all(&scroll_dir)?;
    let mut sessions = HashMap::new();
    #[cfg(windows)]
    let adopted = 0;
    #[cfg(unix)]
    let mut adopted = 0;
    #[cfg(unix)]
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
    // A lab owns no terminal, so there is no scrollback of its own to bring back.
    for e in std::fs::read_dir(&scroll_dir)?
        .flatten()
        .filter(|_| lab.is_none())
    {
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
        agents: Arc::new(crate::agents::AgentHub::default()),
        mobile: Arc::new(crate::mobile::Mobile::load(dir, !opts.no_mobile_relay)?),
        lab: lab.as_ref().map(|(l, _)| l.clone()),
    });
    crate::mobile::start(&hub);
    // Adopted terminals (live handoff) keep idling their agent when they exit.
    for s in hub.sessions_snapshot() {
        idle_agent_on_exit(&hub, &s);
    }
    // Agents travel with the ptys in a live handoff (Unix only, like the handoff).
    #[cfg(unix)]
    if let Some(t) = &takeover {
        hub.agents.restore(&t.agents);
    }
    // The lab never starts the hook server: it would replace the endpoint file the user's agents
    // read (`ATLAS_HOOK_ENDPOINT`) and steal their events from the real hostd.
    if lab.is_none()
        && let Err(e) =
            crate::agents::hook_server::start(dir, hub.agents.clone(), hub.mobile.clone()).await
    {
        eprintln!("hostd: agent hook server: {e}");
    }
    if let Some((l, first)) = lab {
        tokio::spawn(l.follow(Arc::downgrade(&hub), first));
    }
    {
        let mut rx = hub.agents.subscribe();
        let weak = Arc::downgrade(&hub);
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                let ev = match rx.recv().await {
                    Ok(x) => x,
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return,
                };
                let Some(hub) = weak.upgrade() else { return };
                hub.broadcast(ev.to_string());
            }
        });
    }
    tokio::spawn(crate::lsp::pump(Arc::downgrade(&hub), lsp_rx));
    eprintln!(
        "atlas hostd {} listening on {} (pid {}{})",
        env!("CARGO_PKG_VERSION"),
        sock.display(),
        std::process::id(),
        if opts.takeover.is_some() {
            format!(", took over {adopted} pty(s)")
        } else if let Some(main) = &opts.lab {
            format!(", LAB following {}", main.display())
        } else {
            String::new()
        }
    );

    if let Some(t) = takeover {
        // Everything is adopted and bound: the old daemon may exit now.
        t.confirm()?;
    }
    let mut term = Terminate::new()?;
    let limit = crate::idle::limit_from_env();
    let mut ticker = tokio::time::interval(crate::idle::tick(limit));
    let mut idle_since: Option<Instant> = None;
    loop {
        tokio::select! {
            acc = listener.accept() => {
                if let Ok(stream) = acc {
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
                } else if hub.lab.is_none() {
                    // A lab has no terminal by design, so "idle" is its resting state between
                    // phone sessions: it stops with `--stop`, never on a timer.
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
    let _ = ipc::remove_stale(&sock);
    drop(listener);
    // The lock is held through the file: release it before removing the file (Windows refuses to
    // delete an open file).
    let hub = Arc::try_unwrap(hub).ok();
    drop(hub);
    let _ = std::fs::remove_file(paths::pid(dir));
    Ok(())
}

/// `SIGTERM` (`atlas hostd --stop`, `retire_unusable`): stop and end the terminals. Windows ends
/// a process without a signal, so only the console's close/shutdown events count there.
#[cfg(unix)]
struct Terminate(tokio::signal::unix::Signal);
#[cfg(unix)]
impl Terminate {
    fn new() -> std::io::Result<Self> {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map(Self)
    }
    async fn recv(&mut self) -> Option<()> {
        self.0.recv().await
    }
}
#[cfg(windows)]
struct Terminate(tokio::signal::windows::CtrlShutdown);
#[cfg(windows)]
impl Terminate {
    fn new() -> std::io::Result<Self> {
        tokio::signal::windows::ctrl_shutdown().map(Self)
    }
    async fn recv(&mut self) -> Option<()> {
        self.0.recv().await
    }
}

async fn conn(hub: Arc<Hub>, stream: ipc::Stream) {
    let conn_id = hub.next_conn.fetch_add(1, Ordering::Relaxed);
    let (r, w) = ipc::split(stream);
    let mut lines = BufReader::new(r);
    let (tx, rx) = crate::pty::out_channel();
    // Returns the write half so `pty_attach_raw` can take the socket over.
    let writer = tokio::spawn(protocol::write_lines(rx, w));

    let authed = match read_line(&mut lines, LongLine::Skip).await {
        Some(l) => serde_json::from_str::<Value>(&l)
            .ok()
            .and_then(|v| serde_json::from_value::<Hello>(v.get("hello")?.clone()).ok())
            .is_some_and(|h| h.token == hub.token),
        None => false,
    };
    if !authed {
        let _ = tx.force(json!({"ok": false, "error": "unauthorized"}).to_string());
        drop(tx);
        let _ = writer.await;
        return;
    }
    let _ = tx.force(json!({"ok": true, "data": hub.info()}).to_string());
    hub.subscribers.lock().insert(conn_id, tx.clone());

    let mut watchers: HashMap<String, notify::RecommendedWatcher> = HashMap::new();
    let mut attached: Vec<Arc<Session>> = Vec::new();
    let mut link = crate::lab::Link::default();
    while let Some(line) = read_line(&mut lines, LongLine::Skip).await {
        // `""` is a line over the limit, skipped. Its id is unreadable, so no reply can
        // carry the failure: tell the client.
        if line.is_empty() {
            let msg = format!("request over {MAX_LINE_BYTES} bytes dropped");
            let _ = tx.force(json!({"ev": "error", "message": msg}).to_string());
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let id = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v.get("id")?.as_u64())
                    .unwrap_or(0);
                let _ = tx.force(protocol::reply(id, Err(format!("bad request: {e}"))));
                continue;
            }
        };
        let id = req.id;
        // The lab lane: forward to main, refuse, or relay a pty attach; `Route::Local` falls
        // through to this daemon's own code. Answered inline, so pty input keeps its order.
        if let Some(lab) = &hub.lab {
            let raw_req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            let args = &raw_req["args"];
            let handled = match &req.cmd {
                HostCmd::PtyAttach { .. } => Some(link.attach(lab, args.clone(), tx.clone()).await),
                HostCmd::PtyDetach { .. } => Some(link.detach(args.clone()).await),
                cmd => {
                    let name = raw_req["cmd"].as_str().unwrap_or_default();
                    crate::lab::dispatch(lab, cmd, name, args).await
                }
            };
            if let Some(res) = handled {
                let _ = tx.send(protocol::reply(id, res));
                continue;
            }
        }
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
                            return raw(s, conn_id, replay, id, tx, writer, lines).await;
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            HostCmd::Shutdown {} => {
                let _ = tx.force(protocol::reply(id, Ok(json!({}))));
                hub.stop.notify_one();
                break;
            }
            // Input to a pty must reach it in request order: answered inline.
            cmd @ (HostCmd::PtyWrite { .. }
            | HostCmd::PtyResize { .. }
            | HostCmd::PtySignal { .. }) => handle(&hub, cmd, conn_id).await,
            cmd => {
                // Slow commands (exec, kill) must not block this connection's other requests.
                let hub = hub.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx.force(protocol::reply(id, handle(&hub, cmd, conn_id).await));
                });
                continue;
            }
        };
        let _ = tx.force(protocol::reply(id, res));
    }
    for s in attached {
        s.detach(conn_id);
    }
    hub.subscribers.lock().remove(&conn_id);
    crate::lsp::connection_closed(&hub, conn_id).await;
    drop(tx);
    let _ = writer.await;
}

/// What to do with a line over `MAX_LINE_BYTES`.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum LongLine {
    /// Skip it (the protocol drops it) and keep the connection: local clients.
    Skip,
    /// End the connection: peers from the network, which could otherwise stream one
    /// endless line forever.
    Close,
}

/// Next request line, `""` for a skipped long one, or `None` when the peer closed (or
/// sent a long line under [`LongLine::Close`]). Never holds more than `MAX_LINE_BYTES`
/// of a line, so a peer cannot grow the buffer without bound.
pub(crate) async fn read_line<R: AsyncBufRead + Unpin>(
    r: &mut R,
    long_line: LongLine,
) -> Option<String> {
    let limit = MAX_LINE_BYTES as u64 + 1;
    let mut bytes = Vec::new();
    let mut long = false;
    loop {
        bytes.clear();
        match (&mut *r).take(limit).read_until(b'\n', &mut bytes).await {
            Ok(0) | Err(_) => return None,
            Ok(_) if bytes.ends_with(b"\n") => break,
            Ok(_) if (bytes.len() as u64) < limit => return None, // EOF mid-line
            Ok(_) if long_line == LongLine::Close => return None,
            Ok(_) => long = true,
        }
    }
    if long || bytes.len() > MAX_LINE_BYTES {
        return Some(String::new());
    }
    Some(String::from_utf8_lossy(&bytes).trim_end().to_string())
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
                // Forward slashes on Windows too: clients match these against their own roots.
                let p = atlas_core::paths::normalize(&p);
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

/// When the program in an agent's pane exits, the agent goes idle: hostd sees the exit
/// itself, so this holds with the app closed (phones would otherwise show a dead agent
/// as working). Listens like a client that wants no output.
fn idle_agent_on_exit(hub: &Hub, s: &Arc<Session>) {
    let Some(pane) = s.meta.scope.pane_key.clone() else {
        return;
    };
    if s.exited().is_some() {
        return;
    }
    let (tx, mut rx) = crate::pty::out_channel();
    s.attach(
        hub.next_conn.fetch_add(1, Ordering::Relaxed),
        tx,
        false,
        false,
    );
    let agents = hub.agents.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if line.contains(r#""ev":"pty_exit""#) {
                agents.idle(&pane);
                break;
            }
        }
    });
}

pub(crate) async fn handle(hub: &Hub, cmd: HostCmd, conn_id: u64) -> Result<Value, String> {
    let e = |e: anyhow::Error| format!("{e:#}");
    match cmd {
        HostCmd::PtyCreate(mut req) => {
            // Any agent started in a terminal of a pane reports to the hook server (the omp
            // extension and `atlas hook` need both); a launcher's own values win.
            if let Some(pane) = req.scope.pane_key.clone() {
                let endpoint = paths::hook_endpoint(&hub.dir)
                    .to_string_lossy()
                    .into_owned();
                req.env.entry("ATLAS_PANE_KEY".into()).or_insert(pane);
                req.env
                    .entry("ATLAS_HOOK_ENDPOINT".into())
                    .or_insert(endpoint);
            }
            let s = Session::spawn(req, &hub.scroll_dir).map_err(e)?;
            let id = s.id.clone();
            idle_agent_on_exit(hub, &s);
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
            // `ps` is a blocking fork: not on a runtime worker.
            let sc = s.clone();
            let mut v = tokio::task::spawn_blocking(move || sc.info_summary())
                .await
                .map_err(|e| e.to_string())?;
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
        HostCmd::AgentBind { pane, agent, cwd } => {
            hub.agents.bind(&pane, &agent, &cwd);
            Ok(json!({}))
        }
        HostCmd::AgentPaneExited { pane } => {
            hub.agents.idle(&pane);
            Ok(json!({}))
        }
        HostCmd::AgentSend { pane, text, steer } => agent_send(hub, &pane, &text, steer).await,
        HostCmd::AgentTree { pane } => {
            let st = hub.agents.status_of(&pane).ok_or("unknown agent pane")?;
            let file = st.session_file.ok_or("no session for pane")?;
            let title = crate::agents::history::title(&file);
            Ok(crate::agents::tree::build(
                &file, &st.agent, st.state, title,
            ))
        }
        HostCmd::AgentHistory { pane, limit, sub } => {
            use crate::agents::history;
            let file = hub
                .agents
                .session_file(&pane)
                .ok_or("no session for pane")?;
            let path = match &sub {
                Some(name) => crate::agents::tree::subagent_file(&file, name)?,
                None => file.clone(),
            };
            let mut v = history::read(&path, limit.min(1000))?;
            if sub.is_none() {
                let st = hub.agents.status_of(&pane).ok_or("unknown agent pane")?;
                let tree = crate::agents::tree::build(&file, &st.agent, st.state, None);
                v["subagents"] = json!(crate::agents::tree::direct_children(&tree));
            }
            Ok(v)
        }
        HostCmd::AgentInterrupt { pane } => agent_interrupt(hub, &pane),
        HostCmd::AgentLaunch {
            project,
            agent,
            prompt,
        } => agent_launch(hub, &project, &agent, prompt).await,
        HostCmd::ProjectList {} => {
            let (panes, status) = agent_panes(hub).await;
            crate::projects::list(hub.data_dir(), &panes, &status).await
        }
        HostCmd::ProjectDetail { id } => {
            let (panes, status) = agent_panes(hub).await;
            crate::projects::detail(hub.data_dir(), &id, &panes, &status).await
        }
        HostCmd::ProjectLogs {
            id,
            service,
            lines,
            query,
        } => crate::projects::logs(hub.data_dir(), &id, service, lines, query).await,
        HostCmd::ProjectService { id, service, op } => {
            crate::projects::service(hub.data_dir(), &id, service, &op).await
        }
        HostCmd::ProjectAction { id, key, confirmed } => {
            crate::projects::action(hub.data_dir(), &id, &key, confirmed).await
        }
        HostCmd::MobilePairStart {} => {
            hub.mobile.ensure_endpoint().await?;
            hub.mobile.pair_start()
        }
        HostCmd::MobileDevices {} => Ok(json!({"devices": hub.mobile.devices()})),
        HostCmd::MobileRevoke { node_id } => hub.mobile.revoke(&node_id).map(|()| json!({})),
        HostCmd::MobileSetScopes { node_id, scopes } => {
            hub.mobile.set_scopes(&node_id, scopes).map(|()| json!({}))
        }
        HostCmd::AgentApprove { pane, req, allow } => {
            hub.agents.approve(&pane, &req, allow).map(|()| json!({}))
        }
        HostCmd::AgentStatusAll {} => Ok(json!({
            "panes": hub.agents.all(),
            "hook_port": *hub.agents.port.lock(),
            "pending_approvals": hub.agents.pending_approvals(),
        })),
        HostCmd::AgentIntegrations {} => Ok(crate::agents::integrations::all().await),
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
    writer: tokio::task::JoinHandle<ipc::WriteHalf>,
    mut lines: BufReader<ipc::ReadHalf>,
) {
    let (rtx, mut rrx) = mpsc::channel::<Vec<u8>>(crate::pty::RAW_QUEUE);
    // Subscribe before replying: output produced meanwhile queues in `rrx`.
    let info = s.attach_raw(conn_id, rtx, replay);
    let _ = tx.force(protocol::reply(id, Ok(info)));
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
            _ = read_line(&mut lines, LongLine::Skip) => break,
        }
    }
    s.detach(conn_id);
    let _ = w.shutdown().await;
}

/// `(pane_key, cwd)` of every live terminal that belongs to a pane, and the agent status
/// of all panes: what [`crate::projects`] needs to attach agents to projects. A lab owns no
/// terminal, so it asks the main hostd for its terminals.
async fn agent_panes(hub: &Hub) -> (Vec<(String, String)>, Value) {
    let panes = match &hub.lab {
        Some(lab) => lab.panes().await,
        None => hub
            .sessions_snapshot()
            .into_iter()
            .filter(|s| s.exited().is_none())
            .filter_map(|s| Some((s.meta.scope.pane_key.clone()?, s.meta.cwd.clone())))
            .collect(),
    };
    (
        panes,
        serde_json::to_value(hub.agents.all()).unwrap_or_default(),
    )
}

/// Type a prompt into an agent's terminal: text, then Enter as a separate write (TUI
/// agents such as omp treat a pasted `\r` inside the text as part of it). Refused while
/// the agent works, so a phone never interleaves keystrokes with a running turn, except as
/// `steer` to omp, which queues it for the turn ([`crate::agents::send_allowed`]).
async fn agent_send(hub: &Hub, pane: &str, text: &str, steer: bool) -> Result<Value, String> {
    let st = hub.agents.status_of(pane).ok_or("unknown agent pane")?;
    crate::agents::send_allowed(st.state, &st.agent, steer).map_err(str::to_string)?;
    let s = agent_pty(hub, pane)?;
    s.write(text.as_bytes()).map_err(|e| format!("{e:#}"))?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    s.write(b"\r").map_err(|e| format!("{e:#}"))?;
    Ok(json!({}))
}

/// The live terminal of an agent pane.
fn agent_pty(hub: &Hub, pane: &str) -> Result<Arc<Session>, String> {
    hub.sessions_snapshot()
        .into_iter()
        .find(|s| s.meta.scope.pane_key.as_deref() == Some(pane) && s.exited().is_none())
        .ok_or_else(|| "no terminal for pane".to_string())
}

/// Escape stops a running turn in omp, Claude Code and Codex alike. Refused unless the agent
/// works, so a stale tap never clears a prompt the user is typing on the desktop.
fn agent_interrupt(hub: &Hub, pane: &str) -> Result<Value, String> {
    use crate::agents::State;
    match hub.agents.state_of(pane) {
        Some(State::Working | State::Blocked | State::Waiting) => {}
        None => return Err("unknown agent pane".into()),
        Some(_) => return Err("agent is not working".into()),
    }
    agent_pty(hub, pane)?
        .write(b"\x1b")
        .map_err(|e| format!("{e:#}"))?;
    Ok(json!({}))
}

/// A new agent tab in a project, made by the desktop (it owns tabs, worktrees and the
/// agent's launch env): `terminal_create` on its control socket.
async fn agent_launch(
    hub: &Hub,
    project: &str,
    agent: &str,
    prompt: Option<String>,
) -> Result<Value, String> {
    let root = crate::projects::root_of(&hub.dir, project)?;
    let args = json!({
        "worktree": root.to_string_lossy(),
        "agent": agent,
        "prompt": prompt.filter(|p| !p.trim().is_empty()),
    });
    // Same budget as the CLI's `atlas term new`: the agent binary may take a while to start.
    let tab = tokio::task::spawn_blocking(move || {
        atlas_core::desktop_ctl::call("terminal_create", &args, std::time::Duration::from_secs(60))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))?;
    Ok(json!({"pane": tab["tab"], "pty": tab["pty"]}))
}

#[cfg(all(test, unix))]
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

    /// A hostd on a temp dir and a hand-driven connection to it (requests are written in
    /// one go, without waiting for replies, the way a fast typist or a paste arrives).
    async fn wire() -> (tempfile::TempDir, crate::client::HostClient, ipc::Stream) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let d2 = dir.clone();
        tokio::spawn(async move { serve(&d2).await });
        let client = loop {
            if let Ok(c) = crate::client::HostClient::connect_dir(&dir, "test").await {
                break c;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        let mut s = ipc::connect(&paths::socket(&dir)).await.unwrap();
        let token = paths::read_token(&dir).unwrap();
        let hello = protocol::hello_line(&token, "raw");
        s.write_all(format!("{hello}\n").as_bytes()).await.unwrap();
        (tmp, client, s)
    }

    /// Incident: every request was handled on its own task, so two writes of one connection
    /// (paste then ⏎, two keys) could reach the pty swapped.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn writes_from_one_connection_reach_the_program_in_order() {
        let (_tmp, client, mut s) = wire().await;
        let pty = client
            .call(
                "pty_create",
                json!({"cwd": "/", "argv": ["/bin/sh", "-c", "stty -echo; cat"], "rows": 300}),
            )
            .await
            .unwrap()["pty"]
            .as_str()
            .unwrap()
            .to_string();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let mut out = String::new();
        for i in 0..200 {
            let data = B64.encode(format!("{i}\n"));
            out += &json!({"id": i + 1, "cmd": "pty_write", "args": {"pty": pty, "data": data}})
                .to_string();
            out.push('\n');
        }
        s.write_all(out.as_bytes()).await.unwrap();
        let want: Vec<String> = (0..200).map(|i| i.to_string()).collect();
        let mut got = Vec::new();
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let r = client
                .call("pty_read", json!({"pty": pty, "lines": 1000}))
                .await
                .unwrap();
            got = r["text"]
                .as_str()
                .unwrap()
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if got.len() >= 200 {
                break;
            }
        }
        assert_eq!(got, want);
    }

    /// Incident: a request over the line limit was dropped without a word (a big paste
    /// vanished). The client now gets an error event, and the connection keeps serving.
    #[tokio::test]
    async fn an_oversize_request_is_reported_and_the_connection_survives() {
        let (_tmp, _client, s) = wire().await;
        let (r, mut w) = ipc::split(s);
        let mut lines = BufReader::new(r).lines();
        let big = format!(
            "{{\"id\":7,\"cmd\":\"pty_write\",\"args\":{{\"pty\":\"x\",\"data\":\"{}\"}}}}\n",
            "A".repeat(2 * MAX_LINE_BYTES)
        );
        w.write_all(big.as_bytes()).await.unwrap();
        w.write_all(b"{\"id\":8,\"cmd\":\"pty_list\",\"args\":{}}\n")
            .await
            .unwrap();
        let mut seen_error = false;
        loop {
            let l = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
                .await
                .expect("hostd went quiet")
                .unwrap()
                .expect("connection closed");
            let v: Value = serde_json::from_str(&l).unwrap();
            if v["ev"] == "error" {
                seen_error = true;
            }
            if v["id"] == 8 {
                assert_eq!(v["ok"], true);
                break;
            }
        }
        assert!(seen_error, "no error event for the dropped request");
    }
}
