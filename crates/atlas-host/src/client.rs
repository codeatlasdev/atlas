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
use tokio::sync::{broadcast, oneshot};

use crate::paths;

/// How long a call waits for hostd's reply unless [`deadline_for`] says it legitimately takes longer (#298).
pub const CALL_DEADLINE: Duration = Duration::from_secs(10);

/// The hello (and the first reply of an attach) of a daemon that is listening but not answering (#298).
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(5);

/// What "no deadline" means for a call that runs a user's own command: a day.
const NO_DEADLINE: Duration = Duration::from_secs(24 * 3600);

/// Commands that answer from memory or one small local read/write: [`CALL_DEADLINE`] (what an unlisted name gets;
/// the list exists so the test can tell "decided" from "forgotten").
#[cfg(test)]
const QUICK: &[&str] = &[
    "agent_approve",
    "agent_bind",
    "agent_interrupt",
    "agent_pane_exited",
    "agent_send",
    "agent_status_all",
    "crew_audit",
    "crew_github_connect_resume",
    "crew_pause",
    "crew_resume",
    "crew_servers",
    "crew_settings_get",
    "crew_settings_set",
    "crew_telegram_link",
    "crew_telegram_set",
    "crew_telegram_status",
    "crewmate_action_answer",
    "crewmate_answer",
    "crewmate_attachment",
    "crewmate_attachment_put",
    "crewmate_audit",
    "crewmate_cancel",
    "crewmate_create",
    "crewmate_list",
    "crewmate_memory",
    "crewmate_memory_set",
    "crewmate_routine_set",
    "crewmate_secret_set",
    "crewmate_task",
    "crewmate_tasks",
    "crewmate_triggers_set",
    "crewmate_update",
    "crewmate_usage",
    "engine_limits",
    "fs_list",
    "fs_mkdir",
    "fs_rename",
    "fs_stat",
    "fs_unwatch",
    "fs_watch",
    "info",
    "lsp_apply_edit_result",
    "lsp_cancel",
    "lsp_change",
    "lsp_close",
    "lsp_log",
    "lsp_save",
    "lsp_status",
    "mobile_devices",
    "mobile_invite",
    "mobile_invite_dismiss",
    "mobile_invites",
    "mobile_revoke",
    "mobile_set_scopes",
    "pty_attach",
    "pty_attach_raw",
    "pty_clear",
    "pty_create",
    "pty_detach",
    "pty_info",
    "pty_list",
    "pty_read",
    "pty_resize",
    "pty_signal",
    "pty_write",
    "push_forget",
    "push_register",
    "push_unregister",
    "shutdown",
];

/// Commands that legitimately outlast [`CALL_DEADLINE`], each with the bound its handler has plus a margin of
/// 10 s (so the handler's own error, not this deadline, is what the caller sees). Seconds, and why:
const SLOW: &[(&str, u64)] = &[
    // Session files read and parsed; integrations run the agents' own `status` (10 s each).
    ("agent_history", 30),
    ("agent_tree", 30),
    ("agent_integrations", 30),
    // Worktree list (70) + the desktop's `terminal_create` (60).
    ("agent_launch", 140),
    // `gh auth status` and `gh --version` are 120 s each (`crew::proc`); `login_start` also downloads gh
    // (once) and waits 30 s for the device code; `login_poll` sets up git credentials and asks again.
    ("crew_github_status", 250),
    ("crew_github_login_start", 360),
    ("crew_github_login_poll", 250),
    ("crew_github_logout", 60),
    // Probes of the four engines run together, 20 s each; the login only spawns its terminal.
    ("crew_engine_status", 40),
    ("crew_engine_login", 30),
    // A running turn is waited for 30 s, then four git calls (120 s each).
    ("crewmate_delete", 300),
    // Large reads and writes; a recursive remove of a tree such as node_modules.
    ("fs_read", 30),
    ("fs_write", 30),
    ("fs_remove", 120),
    // `GIT_TIMEOUT` 60 s per git call. Network (push, `gh pr create`): `NET_TIMEOUT` 120 s.
    ("git_status", 70),
    ("git_diff", 70),
    ("git_stage", 70),
    ("git_unstage", 70),
    ("git_commit", 70),
    ("git_push", 130),
    ("git_pr_create", 130),
    // Runs the user's chosen agent CLI on the diff: no bound of its own, so a quarter of an hour.
    ("git_commit_message", 900),
    // Server start (`init_timeout` 120 s) then the request (`request_timeout` 60 s); install downloads.
    ("lsp_open", 130),
    ("lsp_restart", 130),
    ("lsp_request", 190),
    ("lsp_install", 600),
    ("lsp_uninstall", 30),
    ("lsp_catalog", 30),
    // Binds the iroh endpoint.
    ("mobile_pair_start", 30),
    // `wait_exit` 30 s + 10 s; `pty_open` lists worktrees first (70 s).
    ("pty_kill", 60),
    ("pty_open", 80),
    // atlas tool calls (status, logs) per project; `service` starts and stops a stack and waits for it.
    ("project_list", 120),
    ("project_detail", 120),
    ("project_logs", 60),
    ("project_service", 600),
    // Git calls of 60 s; `create` runs the project's setup (`setup_timeout`, 600 s by default, per project).
    ("worktree_list", 70),
    ("worktree_create", 3600),
    ("worktree_remove", 300),
    // Accept 20 s + ack 20 s of the handoff, plus spawning the new daemon.
    ("upgrade", 60),
];

/// Commands whose deadline depends on the call (see [`deadline_for`]) or that have none.
#[cfg(test)]
const SPECIAL: &[&str] = &[
    // The user's own command, as long as it runs.
    "project_action",
    // The command's own `timeout_ms`.
    "exec",
    // The remote crew server's command: its own deadline, plus the 20 s the relay waits to connect.
    "crew_call",
];

/// How long `cmd` may take before hostd counts as wedged. Every hostd command is classified above; the test
/// `every_hostd_command_has_a_deadline_class` fails for a new one that is not, so a long command cannot
/// silently get [`CALL_DEADLINE`]. An unknown name (a newer daemon's command) gets the plain deadline.
pub fn deadline_for(cmd: &str, args: &Value) -> Duration {
    match cmd {
        "project_action" => NO_DEADLINE,
        "exec" => {
            Duration::from_millis(args["timeout_ms"].as_u64().unwrap_or(60_000)) + CALL_DEADLINE
        }
        "crew_call" => {
            deadline_for(args["cmd"].as_str().unwrap_or_default(), &args["args"])
                + Duration::from_secs(30)
        }
        _ => match SLOW.iter().find(|(c, _)| *c == cmd) {
            Some((_, secs)) => Duration::from_secs(*secs),
            None => CALL_DEADLINE,
        },
    }
}

/// The error of a call; [`Unanswered`] inside when hostd was too slow.
pub use anyhow::Error as CallError;

/// hostd did not answer in time (a call's deadline or the handshake's): the daemon is wedged, or the reply was lost.
/// Callers that can tell "slow" from "refused" check it with `err.downcast_ref::<Unanswered>()`.
#[derive(Debug)]
pub struct Unanswered(String);

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unanswered {}

/// A pushed event: `{"ev": "...", ...}`.
pub type HostEvent = Arc<Value>;

/// Calls waiting for their reply. `None` once the connection dropped: the reader closes it as it
/// exits, so a call can never register after the last moment anyone would answer it (#226).
type Pending = Arc<Mutex<Option<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>>;

#[derive(Clone)]
pub struct HostClient {
    tx: crate::pty::Out,
    pending: Pending,
    next: Arc<AtomicU64>,
    events: broadcast::Sender<HostEvent>,
    closed: Arc<tokio::sync::Notify>,
    /// `hostd.log` next to the socket, named in the error of a call that got no answer.
    log: Arc<Path>,
}

impl HostClient {
    pub async fn connect(socket: &Path, token: &str, client: &str) -> Result<Self> {
        Self::connect_within(socket, token, client, HANDSHAKE_DEADLINE).await
    }

    /// [`connect`](Self::connect) with an explicit handshake deadline.
    async fn connect_within(
        socket: &Path,
        token: &str,
        client: &str,
        handshake: Duration,
    ) -> Result<Self> {
        let log: Arc<Path> = paths::log(socket.parent().unwrap_or(Path::new("."))).into();
        let stream = ipc::connect(socket)
            .await
            .with_context(|| format!("connect {}", socket.display()))?;
        let (r, mut w) = ipc::split(stream);
        let mut lines = BufReader::new(r).lines();
        let hello = crate::protocol::hello_line(token, client);
        let first = tokio::time::timeout(handshake, async {
            w.write_all(format!("{hello}\n").as_bytes()).await?;
            lines
                .next_line()
                .await?
                .ok_or_else(|| anyhow!("hostd closed the connection"))
        })
        .await
        .map_err(|_| {
            Unanswered(format!(
                "hostd did not answer the handshake within {}s (see {})",
                handshake.as_secs(),
                log.display()
            ))
        })??;
        let v: Value = serde_json::from_str(&first)?;
        if v["ok"] != true {
            return Err(anyhow!(
                "hostd: {}",
                v["error"].as_str().unwrap_or("handshake failed")
            ));
        }

        let (tx, rx) = crate::pty::out_channel();
        tokio::spawn(crate::protocol::write_lines(rx, w));
        let pending: Pending = Arc::new(Mutex::new(Some(HashMap::new())));
        let (events, _) = broadcast::channel(4096);
        let closed = Arc::new(tokio::sync::Notify::new());
        let (p, ev, cl) = (pending.clone(), events.clone(), closed.clone());
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    let tx = p.lock().as_mut().and_then(|m| m.remove(&id));
                    if let Some(tx) = tx {
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
            // Dropping every waiting sender fails those calls; `None` fails every later one.
            *p.lock() = None;
            cl.notify_waiters();
            let _ = ev.send(Arc::new(json!({"ev": "disconnected"})));
        });
        Ok(Self {
            tx,
            pending,
            next: Arc::new(AtomicU64::new(1)),
            events,
            closed,
            log,
        })
    }

    /// Send a command and wait for its reply, at most [`deadline_for`] it.
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value> {
        let deadline = deadline_for(cmd, &args);
        self.call_within(cmd, args, deadline).await
    }

    /// [`call`](Self::call) with an explicit deadline. A daemon that never answers (wedged, or
    /// the reply lost) fails the call and frees its `pending` slot instead of hanging the caller (#298).
    pub async fn call_within(&self, cmd: &str, args: Value, deadline: Duration) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (otx, orx) = oneshot::channel();
        match self.pending.lock().as_mut() {
            Some(m) => m.insert(id, otx),
            None => return Err(anyhow!("hostd disconnected")),
        };
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string();
        if self.tx.force(line).is_err() {
            if let Some(m) = self.pending.lock().as_mut() {
                m.remove(&id);
            }
            return Err(anyhow!("hostd disconnected"));
        }
        match tokio::time::timeout(deadline, orx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => Err(anyhow!("hostd disconnected")),
            Err(_) => {
                if let Some(m) = self.pending.lock().as_mut() {
                    m.remove(&id);
                }
                Err(Unanswered(format!(
                    "hostd did not answer `{cmd}` within {}s (see {})",
                    deadline.as_secs(),
                    self.log.display()
                ))
                .into())
            }
        }
    }

    pub fn events(&self) -> broadcast::Receiver<HostEvent> {
        self.events.subscribe()
    }

    /// Resolves when the connection drops, also when it dropped before this was called.
    pub async fn closed(&self) {
        // Register before checking: the reader's single `notify_waiters` cannot fall in between.
        let notified = self.closed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.tx.is_closed() || self.pending.lock().is_none() {
            return;
        }
        notified.await
    }

    /// Connect to the hostd at `dir` (default `~/.atlas`).
    pub async fn connect_dir(dir: &Path, client: &str) -> Result<Self> {
        let token = paths::read_token(dir).context("hostd token")?;
        Self::connect(&paths::socket(dir), &token, client).await
    }
}

/// The `atlas` binary used to start hostd, resolved from the process `PATH` (see
/// [`atlas_core::paths::resolve_atlas_cli`]).
pub fn atlas_bin() -> Result<PathBuf> {
    atlas_bin_with(None)
}

/// [`atlas_bin`] that also looks on `login_path` (the login shell's `PATH`) before the process one: an app started
/// from Finder or the Dock has a minimal `PATH`.
pub fn atlas_bin_with(login_path: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    atlas_core::paths::resolve_atlas_cli(
        std::env::var_os("ATLAS_BIN").as_deref(),
        dirs::home_dir().as_deref(),
        atlas_core::paths::bundled_cli(),
        login_path,
        std::env::var_os("PATH").as_deref(),
    )
    .ok_or_else(|| anyhow!(NOT_FOUND))
}

/// What the user can do when no runnable `atlas` was found (the CLI that `atlas desktop --install` lives in is the
/// one that is missing, so the message gives the installer itself).
pub const NOT_FOUND: &str = if cfg!(windows) {
    "atlas CLI not found — install (PowerShell): irm https://atlas.codeatlas.com.br/install.ps1 | iex"
} else {
    "atlas CLI not found — install: curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash"
};

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

/// One `{"ok":..}` line from hostd, as its `data`; [`HANDSHAKE_DEADLINE`] at most.
async fn reply_line(r: &mut BufReader<ipc::ReadHalf>) -> Result<Value> {
    let mut line = String::new();
    let n = tokio::time::timeout(HANDSHAKE_DEADLINE, r.read_line(&mut line))
        .await
        .map_err(|_| {
            Unanswered(format!(
                "hostd did not answer within {}s",
                HANDSHAKE_DEADLINE.as_secs()
            ))
        })??;
    if n == 0 {
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
    use super::{HostClient, deadline_for, is_hostd_command};
    use atlas_core::ipc;
    use serde_json::json;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    /// A fake hostd that accepts one client and answers its hello. It hangs up once it has read
    /// `lines` commands (never answering them) and the returned sender fired or was dropped.
    async fn fake_hostd(
        lines: usize,
    ) -> (
        tempfile::TempDir,
        HostClient,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        let (hang_up, go) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let (r, mut w) = ipc::split(listener.accept().await.unwrap());
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.write_all(b"{\"ok\":true}\n").await.unwrap();
            for _ in 0..lines {
                r.next_line().await.unwrap();
            }
            let _ = go.await;
        });
        let c = HostClient::connect(&sock, "t", "test").await.unwrap();
        (tmp, c, hang_up)
    }

    /// Make the fake hang up and wait until the client's reader has seen it.
    async fn hang_up(c: &HostClient, hang_up: tokio::sync::oneshot::Sender<()>) {
        let mut ev = c.events();
        drop(hang_up);
        tokio::time::timeout(Duration::from_secs(5), async {
            while ev.recv().await.unwrap()["ev"] != "disconnected" {}
        })
        .await
        .expect("the client never saw the hang-up");
    }

    // Regression: #226. A call made after the reader saw the connection drop was put in the
    // pending map that nobody would ever drain again, and the writer accepted its line, so the
    // call waited forever (the lab forwarding to a main that had just died hung a CI test to its
    // 180 s timeout).
    #[tokio::test]
    async fn a_call_on_a_connection_that_already_dropped_fails_at_once() {
        let (_tmp, c, h) = fake_hostd(0).await;
        hang_up(&c, h).await;
        let r = tokio::time::timeout(Duration::from_secs(5), c.call("info", json!({})))
            .await
            .expect("a call on a dead connection must not wait forever");
        assert!(r.unwrap_err().to_string().contains("disconnected"));
    }

    // Same class: `closed()` after the drop missed the one-shot wake-up and waited forever.
    #[tokio::test]
    async fn closed_resolves_when_asked_after_the_drop() {
        let (_tmp, c, h) = fake_hostd(0).await;
        hang_up(&c, h).await;
        tokio::time::timeout(Duration::from_secs(5), c.closed())
            .await
            .expect("closed() must resolve for a connection that is already gone");
    }

    // A call in flight when the daemon hangs up without answering it fails, it does not hang.
    #[tokio::test]
    async fn a_call_in_flight_when_the_daemon_hangs_up_fails() {
        let (_tmp, c, h) = fake_hostd(1).await;
        drop(h);
        let r = tokio::time::timeout(Duration::from_secs(5), c.call("info", json!({})))
            .await
            .expect("an unanswered call must fail when the connection drops");
        assert!(r.unwrap_err().to_string().contains("disconnected"));
    }

    // Regression: #298. A daemon that answered hello and then never replies hung the caller for
    // good and left its entry in `pending`.
    #[tokio::test]
    async fn a_call_the_daemon_never_answers_fails_at_its_deadline_and_frees_its_slot() {
        let (_tmp, c, _h) = fake_hostd(1).await;
        let t = std::time::Instant::now();
        let r = tokio::time::timeout(
            Duration::from_secs(5),
            c.call_within("info", json!({}), Duration::from_millis(300)),
        )
        .await
        .expect("the deadline must end the call, not the test's own timeout");
        let e = r.unwrap_err();
        assert!(t.elapsed() < Duration::from_secs(2));
        assert!(e.to_string().contains("hostd.log") && e.to_string().contains("`info`"));
        assert!(e.downcast_ref::<super::Unanswered>().is_some());
        assert!(c.pending.lock().as_ref().unwrap().is_empty());
    }

    // Regression: #298. A listener that accepts and never says hello hung `connect` (and the
    // desktop's whole start-up) for good.
    #[tokio::test]
    async fn connect_to_a_daemon_that_never_says_hello_fails_at_the_handshake_deadline() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let _held = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let t = std::time::Instant::now();
        let e = HostClient::connect_within(&sock, "t", "test", Duration::from_millis(300))
            .await
            .err()
            .expect("a silent daemon must not connect")
            .to_string();
        assert!(t.elapsed() < Duration::from_secs(2));
        assert!(e.contains("handshake") && e.contains("hostd.log"), "{e}");
        assert!(super::HANDSHAKE_DEADLINE <= Duration::from_secs(5));
    }

    // #298: a command added to hostd without a decision here would silently get the 10 s deadline and
    // fail while it is working. The commands are read from the wire fixture, which its own golden test
    // keeps equal to the real `HostCmd`.
    #[test]
    fn every_hostd_command_has_a_deadline_class() {
        let fixture: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(crate::protocol_golden::fixture("commands.json")).unwrap(),
        )
        .unwrap();
        let wire: std::collections::BTreeSet<&str> = fixture["commands"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let classes: Vec<&str> = super::QUICK
            .iter()
            .copied()
            .chain(super::SLOW.iter().map(|(c, _)| *c))
            .chain(super::SPECIAL.iter().copied())
            .collect();
        let known: std::collections::BTreeSet<&str> = classes.iter().copied().collect();
        assert_eq!(known.len(), classes.len(), "a command is classified twice");
        let unclassified: Vec<_> = wire.difference(&known).collect();
        assert!(
            unclassified.is_empty(),
            "classify these in client.rs (QUICK, SLOW or SPECIAL): {unclassified:?}"
        );
        let stale: Vec<_> = known.difference(&wire).collect();
        assert!(stale.is_empty(), "no longer hostd commands: {stale:?}");
    }

    #[test]
    fn deadlines_follow_the_class_and_the_call() {
        let none = json!({});
        assert_eq!(deadline_for("info", &none), super::CALL_DEADLINE);
        assert_eq!(deadline_for("upgrade", &none), Duration::from_secs(60));
        assert_eq!(deadline_for("lsp_install", &none), Duration::from_secs(600));
        assert!(deadline_for("project_action", &none) >= Duration::from_secs(3600));
        // `exec` outlives its own timeout by the plain deadline, so hostd's answer is not raced.
        assert_eq!(
            deadline_for("exec", &json!({"timeout_ms": 120_000})),
            Duration::from_secs(130)
        );
        // A relayed command gets the remote command's deadline plus the relay's connect wait.
        assert_eq!(
            deadline_for("crew_call", &json!({"cmd": "git_push", "args": {}})),
            Duration::from_secs(160)
        );
        assert_eq!(
            deadline_for("crew_call", &json!({"cmd": "crewmate_list"})),
            Duration::from_secs(40)
        );
    }

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
