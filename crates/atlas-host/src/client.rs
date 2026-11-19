//! Async client for hostd, used by Atlas Desktop and the CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use atlas_core::ipc;
use atlas_core::progress::Coded;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, oneshot};

use crate::paths;

/// The failures of this client that a program tells apart by `code` (`atlas_core::progress::Coded`): stable,
/// never translated, never reworded. Atlas Desktop renders its own sentence from the code and `args`; `message`
/// is English, for logs, the CLI and a client that does not know the code. A new code is added to [`CODES`] in
/// the same change as the desktop's message for it (`apps/desktop/src/lib/hostText.ts`).
///
/// | code | args | message |
/// |---|---|---|
/// | `hostd.closed` | none | `hostd closed the connection` |
/// | `hostd.disconnected` | none | `hostd disconnected` |
/// | `hostd.start_failed` | none | `hostd did not start` |
/// | `hostd.refused` | `why`: what hostd answered (`unauthorized`, `hello timeout`...) | `hostd: <why>` |
/// | `hostd.busy` | none | `hostd: hostd busy` |
/// | `hostd.too_many_clients` | none | `hostd: too many clients` |
/// | `hostd.cli_not_found` | `command`: the installer line for this OS | [`NOT_FOUND`] |
///
/// A failed request of a daemon that codes its failures (`protocol::REPLY_CODES`: the filesystem and `exec`) comes
/// back as a [`Coded`] of that code too (`reply_error`).
pub const CODES: &[&str] = &[
    CLOSED,
    DISCONNECTED,
    START_FAILED,
    REFUSED,
    BUSY,
    TOO_MANY_CLIENTS,
    CLI_NOT_FOUND,
];
pub const CLOSED: &str = "hostd.closed";
pub const DISCONNECTED: &str = "hostd.disconnected";
pub const START_FAILED: &str = "hostd.start_failed";
pub const REFUSED: &str = "hostd.refused";
pub const BUSY: &str = "hostd.busy";
pub const TOO_MANY_CLIENTS: &str = "hostd.too_many_clients";
pub const CLI_NOT_FOUND: &str = "hostd.cli_not_found";

fn coded(code: &'static str, args: Value, message: impl Into<String>) -> anyhow::Error {
    Coded::new(code, args, message).into()
}

fn closed() -> anyhow::Error {
    coded(CLOSED, json!({}), "hostd closed the connection")
}

fn disconnected() -> anyhow::Error {
    coded(DISCONNECTED, json!({}), "hostd disconnected")
}

fn start_failed() -> anyhow::Error {
    coded(START_FAILED, json!({}), "hostd did not start")
}

/// What hostd answers when it refuses a client because it already serves as many as it takes (the daemon's own
/// answer for a full house, next to [`crate::server::ERR_BUSY`]).
const ERR_TOO_MANY_CLIENTS: &str = "too many clients";

/// hostd answered a hello or a request with `why` instead of its data. A loaded daemon's two refusals (`hostd busy`,
/// `too many clients`) have a code of their own: the app tells them from a daemon that is gone by it.
fn refused(why: &str) -> anyhow::Error {
    let message = format!("hostd: {why}");
    match why {
        crate::server::ERR_BUSY => coded(BUSY, json!({}), message),
        ERR_TOO_MANY_CLIENTS => coded(TOO_MANY_CLIENTS, json!({}), message),
        _ => coded(REFUSED, json!({"why": why}), message),
    }
}

/// The failure a reply line says (`{"ok":false,"error":..}`): a [`Coded`] when it carries a `code` this build knows
/// (`protocol::REPLY_CODES`), else the text alone, which is all an older daemon sends and what every caller printed
/// before the codes. The text is the same either way.
pub(crate) fn reply_error(v: &Value) -> anyhow::Error {
    let message = v["error"].as_str().unwrap_or("error");
    let known = v["code"]
        .as_str()
        .and_then(|c| crate::protocol::REPLY_CODES.iter().find(|k| **k == c));
    match known {
        Some(code) => coded(code, v["args"].clone(), message),
        None => anyhow!(message.to_string()),
    }
}

/// How long a call waits for hostd's reply unless [`deadline_for`] says it legitimately takes longer (#298).
pub const CALL_DEADLINE: Duration = Duration::from_secs(10);

/// The hello (and the first reply of an attach) of a daemon that is listening but not answering (#298).
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(5);

/// While a call whose deadline is above [`CALL_DEADLINE`] waits, the client asks hostd `info` this often...
const PROBE_EVERY: Duration = Duration::from_secs(10);

/// ...and fails the call when that probe is not answered within this long: hostd serves each request of a
/// connection on its own task, so a long handler never delays `info`, and silence means a wedged daemon.
const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// A daemon that streams output but answers no `info` for this many probes in a row (about five minutes) is
/// wedged on its command path (a lock its readers do not need): output alone does not prove it alive forever.
const PROBE_MISSES: u32 = 20;

/// What "no deadline" means for a call that runs a user's own command: a day.
const NO_DEADLINE: Duration = Duration::from_secs(24 * 3600);

/// The cap of a call that is not provably quick (see [`LONG`]).
const LONG_CALL_DEADLINE: Duration = Duration::from_secs(3600);

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
    // The audit log's last entries, read from its end (bounded however long the log is).
    "mobile_activity",
    "mobile_devices",
    "mobile_invite",
    "mobile_invite_dismiss",
    "mobile_invites",
    // One small write of devices.json.
    "mobile_rename",
    "mobile_revoke",
    "mobile_set_scopes",
    "pty_attach",
    "pty_attach_raw",
    "pty_clear",
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

/// Everything not provably quick: its handler spawns a process, reads or writes files of unbounded size, waits for
/// another process, or loops over worktrees/projects (each git call up to 60 s). The handlers' own bounds are not
/// hugged (they multiply, and drift): [`LONG_CALL_DEADLINE`] only caps a call, and the liveness probe (below) is what
/// notices a wedged daemon. The note says why a command is not [`QUICK`].
const LONG: &[&str] = &[
    "agent_history",
    "agent_tree",
    "agent_integrations",
    "agent_launch",
    "crew_github_status",
    "crew_github_login_start",
    "crew_github_login_poll",
    "crew_github_logout",
    "crew_engine_status",
    "crew_engine_login",
    "crewmate_delete",
    "fs_read",
    "fs_write",
    "fs_remove",
    "git_status",
    "git_diff",
    "git_stage",
    "git_unstage",
    "git_commit",
    "git_push",
    "git_pr_create",
    "git_commit_message",
    "lsp_open",
    "lsp_restart",
    "lsp_request",
    "lsp_install",
    "lsp_uninstall",
    "lsp_catalog",
    "mobile_pair_start",
    "pty_create",
    "pty_kill",
    "pty_open",
    "project_list",
    "project_detail",
    "project_logs",
    "project_service",
    "worktree_list",
    "worktree_create",
    "worktree_remove",
    "upgrade",
    // Run `atlas update --check` (up to 45 s on a slow network); `atlas_update` checks first, then starts it.
    "atlas_update_check",
    "atlas_update",
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
        c if LONG.contains(&c) => LONG_CALL_DEADLINE,
        _ => CALL_DEADLINE,
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

/// The daemon cannot be used by this client, whatever it is doing: nothing listens on its socket, its token
/// cannot be read, or it refused ours (`unauthorized`). Only this (never [`Unanswered`], never any other error
/// of a daemon that did answer) lets [`ensure_in`] retire a live daemon: a slow or wedged one is not ours to kill.
#[derive(Debug)]
pub struct Unusable(String);

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unusable {}

/// Does failing to connect with `kind` prove nothing serves the socket? Only "no such socket" and "refused" do. A
/// busy listener (Windows: one pipe instance, a stalled accept loop answers "pipe busy" as `TimedOut`; Unix: a full
/// backlog is `WouldBlock`) belongs to a live daemon, which is not ours to kill.
fn proves_dead_socket(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    )
}

/// Did `connect` fail in a way that proves the daemon unusable by us (see [`Unusable`])?
pub fn is_unusable(e: &CallError) -> bool {
    e.downcast_ref::<Unusable>().is_some()
}

/// A pushed event: `{"ev": "...", ...}`.
pub type HostEvent = Arc<Value>;

type ReplyRx = oneshot::Receiver<Result<Value>>;

/// How a wait for a reply ended.
enum Waited {
    Reply(Result<Value>),
    /// The connection went away with the call unanswered.
    Dropped,
    /// The liveness probe found the daemon silent.
    Silent,
}

/// What one liveness probe found.
enum Probed {
    Answered,
    /// No reply and nothing received at all.
    Quiet,
    /// No reply, but the daemon kept sending (output, events).
    Streaming,
}

/// A call's entry in `pending`, removed when the call ends however it ends (reply, deadline, probe, or the
/// caller dropping the future mid-wait).
struct Slot<'a> {
    client: &'a HostClient,
    id: u64,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if let Some(m) = self.client.pending.lock().as_mut() {
            m.remove(&self.id);
        }
    }
}

/// ` (see <hostd.log>)` for a local daemon, nothing for a tunnel or relay (that log is not on this machine).
fn see_log(log: Option<&Path>) -> String {
    log.map(|l| format!(" (see {})", l.display()))
        .unwrap_or_default()
}

/// Calls waiting for their reply. `None` once the connection dropped: the reader closes it as it
/// exits, so a call can never register after the last moment anyone would answer it (#226).
type Pending = Arc<Mutex<Option<HashMap<u64, oneshot::Sender<Result<Value>>>>>>;

#[derive(Clone)]
pub struct HostClient {
    tx: crate::pty::Out,
    pending: Pending,
    next: Arc<AtomicU64>,
    events: broadcast::Sender<HostEvent>,
    closed: Arc<tokio::sync::Notify>,
    /// `hostd.log` of the local daemon this client was opened on ([`connect_dir`](Self::connect_dir)), named in the
    /// error of a call that got no answer. `None` for a tunnel or relay: that log is not on this machine.
    log: Option<Arc<Path>>,
    /// `(interval, deadline)` of the liveness probe of long calls; a field so a test can shorten it. `None` on a
    /// lab daemon, which answers its forwarded commands inline in its read loop, so it cannot answer a probe.
    probe: Option<(Duration, Duration)>,
    /// Consecutive unanswered probes, while output keeps arriving, before the call fails.
    probe_misses: u32,
    /// Lines received so far, any kind: a daemon that is streaming is alive however slowly it answers.
    received: Arc<AtomicU64>,
}

impl HostClient {
    /// Connect with a token. The handshake is bounded by [`HANDSHAKE_DEADLINE`].
    pub async fn connect(socket: &Path, token: &str, client: &str) -> Result<Self> {
        Self::connect_within(socket, token, client, HANDSHAKE_DEADLINE, None).await
    }

    /// [`connect`](Self::connect) with an explicit handshake deadline; `log` is the daemon's log when it is local.
    async fn connect_within(
        socket: &Path,
        token: &str,
        client: &str,
        handshake: Duration,
        log: Option<Arc<Path>>,
    ) -> Result<Self> {
        let stream = ipc::connect(socket).await.map_err(|e| {
            let why = format!("connect {}: {e}", socket.display());
            if proves_dead_socket(e.kind()) {
                CallError::new(Unusable(why))
            } else {
                anyhow!(why)
            }
        })?;
        let (r, mut w) = ipc::split(stream);
        let mut lines = BufReader::new(r).lines();
        let hello = crate::protocol::hello_line(token, client);
        let first = tokio::time::timeout(handshake, async {
            w.write_all(format!("{hello}\n").as_bytes()).await?;
            lines.next_line().await?.ok_or_else(closed)
        })
        .await
        .map_err(|_| {
            Unanswered(format!(
                "hostd did not answer the handshake within {}s{}",
                handshake.as_secs(),
                see_log(log.as_deref())
            ))
        })??;
        let v: Value = serde_json::from_str(&first)?;
        if v["ok"] != true {
            let why = v["error"].as_str().unwrap_or("handshake failed");
            // Only a refused token proves the daemon unusable; any other answer is a daemon that is alive.
            return Err(match why {
                "unauthorized" => Unusable(format!("hostd: {why}")).into(),
                _ => refused(why),
            });
        }
        let lab = !v["data"]["lab"].is_null();

        let (tx, rx) = crate::pty::out_channel();
        tokio::spawn(crate::protocol::write_lines(rx, w));
        let pending: Pending = Arc::new(Mutex::new(Some(HashMap::new())));
        let (events, _) = broadcast::channel(4096);
        let closed = Arc::new(tokio::sync::Notify::new());
        let received = Arc::new(AtomicU64::new(0));
        let (p, ev, cl, rx_lines) = (
            pending.clone(),
            events.clone(),
            closed.clone(),
            received.clone(),
        );
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                rx_lines.fetch_add(1, Ordering::Relaxed);
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    let tx = p.lock().as_mut().and_then(|m| m.remove(&id));
                    if let Some(tx) = tx {
                        let res = if v["ok"] == true {
                            Ok(v.get("data").cloned().unwrap_or(Value::Null))
                        } else {
                            Err(reply_error(&v))
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
            probe: (!lab).then_some((PROBE_EVERY, PROBE_DEADLINE)),
            probe_misses: PROBE_MISSES,
            received,
        })
    }

    /// Send a command and wait for its reply, at most [`deadline_for`] it.
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value> {
        let deadline = deadline_for(cmd, &args);
        self.call_within(cmd, args, deadline).await
    }

    /// Register a call and send it: its pending slot (freed when dropped) and the receiver of its reply. `Err`
    /// when the connection is gone.
    fn send(&self, cmd: &str, args: Value) -> Result<(Slot<'_>, ReplyRx)> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (otx, orx) = oneshot::channel();
        match self.pending.lock().as_mut() {
            Some(m) => m.insert(id, otx),
            None => return Err(disconnected()),
        };
        let slot = Slot { client: self, id };
        let line = json!({"id": id, "cmd": cmd, "args": args}).to_string();
        if self.tx.force(line).is_err() {
            return Err(disconnected());
        }
        Ok((slot, orx))
    }

    /// One liveness probe: an `info` that must be answered within `deadline`. No answer is [`Probed::Quiet`] when
    /// nothing at all arrived meanwhile (dead), [`Probed::Streaming`] when output kept arriving: hostd queues
    /// replies behind up to 8 MiB of pty output (minutes on a slow tunnel), so that is slow, not yet wedged. A
    /// dropped connection counts as answered: the call being waited for reports that itself.
    async fn probe_once(&self, deadline: Duration) -> Probed {
        let before = self.received.load(Ordering::Relaxed);
        let Ok((_slot, rx)) = self.send("info", json!({})) else {
            return Probed::Answered;
        };
        if tokio::time::timeout(deadline, rx).await.is_ok() {
            Probed::Answered
        } else if self.received.load(Ordering::Relaxed) == before {
            Probed::Quiet
        } else {
            Probed::Streaming
        }
    }

    /// [`call`](Self::call) with an explicit deadline. A daemon that never answers (wedged, or
    /// the reply lost) fails the call and frees its `pending` slot instead of hanging the caller (#298).
    ///
    /// A call allowed more than [`CALL_DEADLINE`] also probes hostd while it waits, so a daemon that wedges
    /// mid-call is noticed within one probe interval and deadline, not after the class deadline (an hour, or a day).
    pub async fn call_within(&self, cmd: &str, args: Value, deadline: Duration) -> Result<Value> {
        let (_slot, mut orx) = self.send(cmd, args)?;
        let probe = self.probe.filter(|_| deadline > CALL_DEADLINE);
        let wait = async {
            let mut missed = 0;
            loop {
                tokio::select! {
                    r = &mut orx => return r.map_or(Waited::Dropped, Waited::Reply),
                    () = tokio::time::sleep(probe.map_or(Duration::MAX, |p| p.0)), if probe.is_some() => {
                        match self.probe_once(probe.map_or(Duration::ZERO, |p| p.1)).await {
                            Probed::Answered => missed = 0,
                            Probed::Streaming => missed += 1,
                            Probed::Quiet => missed = u32::MAX,
                        }
                        if missed >= self.probe_misses {
                            // A reply that landed while the probe waited wins over the verdict.
                            return match orx.try_recv() {
                                Ok(r) => Waited::Reply(r),
                                Err(oneshot::error::TryRecvError::Closed) => Waited::Dropped,
                                Err(oneshot::error::TryRecvError::Empty) => Waited::Silent,
                            };
                        }
                    }
                }
            }
        };
        match tokio::time::timeout(deadline, wait).await {
            Ok(Waited::Reply(Ok(v))) => Ok(v),
            Ok(Waited::Reply(Err(e))) => Err(e),
            Ok(Waited::Dropped) => Err(disconnected()),
            Ok(Waited::Silent) => Err(Unanswered(format!(
                "hostd stopped answering while `{cmd}` was running: no reply to its liveness probes{}",
                see_log(self.log.as_deref())
            ))
            .into()),
            Err(_) => Err(Unanswered(format!(
                "hostd did not answer `{cmd}` within {}s{}",
                deadline.as_secs(),
                see_log(self.log.as_deref())
            ))
            .into()),
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
        Self::connect_dir_within(dir, client, HANDSHAKE_DEADLINE).await
    }

    async fn connect_dir_within(dir: &Path, client: &str, handshake: Duration) -> Result<Self> {
        let token = paths::read_token(dir).map_err(|e| Unusable(format!("hostd token: {e}")))?;
        let log: Arc<Path> = paths::log(dir).into();
        Self::connect_within(&paths::socket(dir), &token, client, handshake, Some(log)).await
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
    .ok_or_else(cli_not_found)
}

/// The line that installs the CLI on this OS (the CLI that `atlas desktop --install` lives in is the one that is
/// missing, so the message gives the installer itself).
pub const INSTALL_COMMAND: &str = if cfg!(windows) {
    "irm https://atlas.codeatlas.com.br/install.ps1 | iex"
} else {
    "curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash"
};

/// What the user can do when no runnable `atlas` was found: English, for logs and a client that does not know
/// `hostd.cli_not_found`.
pub const NOT_FOUND: &str = if cfg!(windows) {
    "atlas CLI not found. Install it in PowerShell: irm https://atlas.codeatlas.com.br/install.ps1 | iex"
} else {
    "atlas CLI not found. Install it with: curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash"
};

fn cli_not_found() -> anyhow::Error {
    cli_not_found_with(INSTALL_COMMAND, NOT_FOUND)
}

/// `hostd.cli_not_found` for a given installer line: production passes this OS's, the desktop contract test passes
/// one fixed line so the fixture is the same on every OS (the desktop shows `command` as given).
fn cli_not_found_with(command: &str, message: &str) -> anyhow::Error {
    coded(CLI_NOT_FOUND, json!({"command": command}), message)
}

/// Connect to the local hostd, starting it (detached, own session) if needed.
pub async fn ensure_local(atlas: &Path, client: &str) -> Result<HostClient> {
    ensure_in(&paths::base(), atlas, client).await
}

/// [`ensure_local`] for the hostd at `dir`.
///
/// A daemon that holds the singleton lock but is unusable by us (its token file was replaced, so it refuses ours)
/// would make every spawn below die with "another atlas hostd is running", so the caller never connected
/// (1.5.0-alpha.5). [`retire_unusable`] ends exactly that daemon first. One that is merely slow to answer or
/// wedged is NOT ended (its terminals would die with it): the error is returned and the user restarts it.
pub async fn ensure_in(dir: &Path, atlas: &Path, client: &str) -> Result<HostClient> {
    ensure_in_within(dir, atlas, client, HANDSHAKE_DEADLINE).await
}

async fn ensure_in_within(
    dir: &Path,
    atlas: &Path,
    client: &str,
    handshake: Duration,
) -> Result<HostClient> {
    match HostClient::connect_dir_within(dir, client, handshake).await {
        Ok(c) => return Ok(c),
        // A live daemon that is slow to say hello (a busy box, a lock held) or wedged is not ours to kill: its
        // terminals and crew agents would die with it. Report it; the user restarts it knowingly.
        Err(e) if !is_unusable(&e) => {
            // ...but a daemon that is on its way out (a normal restart) is given a moment to finish, so the caller
            // starts the next one instead of reporting an outage.
            if let Some(pid) = live_hostd_pid(dir)
                && !wait_gone(pid, EXIT_GRACE).await
            {
                return Err(e);
            }
        }
        Err(_) => {}
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
        // i18n-ignore: context of an error chain (names the program being started), read in logs and after the coded message
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
    Err(last.unwrap_or_else(start_failed))
        // i18n-ignore: context of an error chain (names the log to read), read in logs and after the coded message
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
/// `connect_dir` fails in a way that proves it unusable, see [`Unusable`]): what `ensure_in` is about to retire.
/// `None` when nothing runs, it answers, or it is merely slow.
pub async fn unusable_hostd_pid(dir: &Path) -> Option<u32> {
    let pid = live_hostd_pid(dir)?;
    HostClient::connect_dir(dir, "probe")
        .await
        .err()
        .filter(is_unusable)
        .map(|_| pid)
}

/// The daemon of `dir` is alive but unusable ([`Unusable`]): end it now. See [`retire_hostd`].
async fn retire_unusable(dir: &Path) {
    retire_hostd(dir, Duration::ZERO).await;
}

/// End the atlas hostd of `dir` if it is still alive after `grace`: `SIGTERM` (its handler ends its terminals
/// cleanly), then wait for its pid lock to free. Exact pid only, and only after [`live_hostd_pid`] confirmed it is
/// an atlas hostd; never a port, never a name. The desktop's "Restart terminal host" uses it for a daemon that
/// does not answer `shutdown`.
pub async fn retire_hostd(dir: &Path, grace: Duration) {
    retire_hostd_within(dir, grace, TERM_WAIT).await;
}

/// How long a daemon that is going away on its own is waited for before it is called unusable.
const EXIT_GRACE: Duration = Duration::from_secs(1);

/// How long a `SIGTERM`ed daemon is waited for before it is killed outright.
const TERM_WAIT: Duration = Duration::from_secs(5);

/// Wait up to `d` for `pid` to end; `true` when it did.
async fn wait_gone(pid: u32, d: Duration) -> bool {
    let until = tokio::time::Instant::now() + d;
    while atlas_core::sys::pid_alive(pid) {
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

async fn retire_hostd_within(dir: &Path, grace: Duration, term_wait: Duration) {
    let Some(pid) = live_hostd_pid(dir) else {
        return;
    };
    // Every signal is preceded by a fresh check that `pid` is still this dir's atlas hostd: after a wait the
    // process may be gone and its pid reused.
    let ours = || live_hostd_pid(dir) == Some(pid);
    if wait_gone(pid, grace).await || !ours() {
        return;
    }
    // Windows ends it outright: its terminals' pseudo consoles close with it.
    atlas_core::sys::terminate(pid);
    if wait_gone(pid, term_wait).await || !ours() {
        return;
    }
    // A runtime that is blocked never runs its `SIGTERM` handler (it shares the `select!` of the accept loop).
    // The caller chose to lose the terminals.
    atlas_core::sys::force_kill(pid);
    wait_gone(pid, term_wait).await;
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
    let token = paths::read_token(dir)
        // i18n-ignore: context of an error chain (names the file being read), read in logs
        .context("hostd token")?;
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
        return Err(closed());
    }
    let v: Value = serde_json::from_str(&line)?;
    if v["ok"] != true {
        return Err(refused(v["error"].as_str().unwrap_or("error")));
    }
    Ok(v.get("data").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::{HostClient, deadline_for, is_hostd_command};
    use atlas_core::ipc;
    use atlas_core::scaling::HANG;
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
        std::fs::write(crate::paths::token(tmp.path()), "t").unwrap();
        let listener = ipc::Listener::bind(&crate::paths::socket(tmp.path())).unwrap();
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
        let c = HostClient::connect_dir(tmp.path(), "test").await.unwrap();
        (tmp, c, hang_up)
    }

    /// Make the fake hang up and wait until the client's reader has seen it.
    async fn hang_up(c: &HostClient, hang_up: tokio::sync::oneshot::Sender<()>) {
        let mut ev = c.events();
        drop(hang_up);
        tokio::time::timeout(HANG, async {
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
        let r = tokio::time::timeout(HANG, c.call("info", json!({})))
            .await
            .expect("a call on a dead connection must not wait forever");
        assert!(r.unwrap_err().to_string().contains("disconnected"));
    }

    // Same class: `closed()` after the drop missed the one-shot wake-up and waited forever.
    #[tokio::test]
    async fn closed_resolves_when_asked_after_the_drop() {
        let (_tmp, c, h) = fake_hostd(0).await;
        hang_up(&c, h).await;
        tokio::time::timeout(HANG, c.closed())
            .await
            .expect("closed() must resolve for a connection that is already gone");
    }

    // A call in flight when the daemon hangs up without answering it fails, it does not hang.
    #[tokio::test]
    async fn a_call_in_flight_when_the_daemon_hangs_up_fails() {
        let (_tmp, c, h) = fake_hostd(1).await;
        drop(h);
        let r = tokio::time::timeout(HANG, c.call("info", json!({})))
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
            HANG,
            c.call_within("info", json!({}), Duration::from_millis(300)),
        )
        .await
        .expect("the deadline must end the call, not the test's own timeout");
        let e = r.unwrap_err();
        // The deadline was waited for, not given up at once; that it ends the call at all is the timeout above.
        assert!(
            t.elapsed() >= Duration::from_millis(300),
            "{:?}",
            t.elapsed()
        );
        assert!(e.to_string().contains("hostd.log") && e.to_string().contains("`info`"));
        assert!(e.downcast_ref::<super::Unanswered>().is_some());
        assert!(c.pending.lock().as_ref().unwrap().is_empty());
    }

    // #298 follow-up: a long call (a project action may run for hours) must not keep a wedged daemon hidden
    // until its class deadline. The fake answers hello, accepts the call and then goes silent for everything.
    #[tokio::test]
    async fn a_long_call_fails_at_the_first_unanswered_probe_not_at_its_class_deadline() {
        let (_tmp, mut c, _h) = fake_hostd(1).await;
        c.probe = Some((Duration::from_millis(200), Duration::from_millis(200)));
        let r = tokio::time::timeout(
            HANG,
            c.call_within("project_action", json!({}), Duration::from_secs(3600)),
        )
        .await
        .expect("a probe must end the call long before its deadline");
        let e = r.unwrap_err();
        // Ended by the probe: the class deadline (an hour) is far past HANG, so only the probe can get here.
        assert!(e.downcast_ref::<super::Unanswered>().is_some());
        assert!(e.to_string().contains("hostd.log") && e.to_string().contains("project_action"));
        assert!(c.pending.lock().as_ref().unwrap().is_empty());
    }

    // The probe must not cry wolf: a long call whose daemon keeps answering `info` runs to its reply.
    #[tokio::test]
    async fn a_long_call_on_a_daemon_that_answers_probes_waits_for_its_reply() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (r, mut w) = ipc::split(listener.accept().await.unwrap());
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.write_all(b"{\"ok\":true}\n").await.unwrap();
            let mut slow_id = None;
            let mut probes = 0;
            while let Ok(Some(line)) = r.next_line().await {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                if v["cmd"] == "info" {
                    probes += 1;
                    w.write_all(
                        format!("{{\"id\":{},\"ok\":true,\"data\":{{}}}}\n", v["id"]).as_bytes(),
                    )
                    .await
                    .unwrap();
                    if probes == 3 {
                        let id = slow_id.clone().unwrap();
                        w.write_all(
                            format!("{{\"id\":{id},\"ok\":true,\"data\":{{\"done\":1}}}}\n")
                                .as_bytes(),
                        )
                        .await
                        .unwrap();
                    }
                } else {
                    slow_id = Some(v["id"].clone());
                }
            }
        });
        let mut c = HostClient::connect(&sock, "t", "test").await.unwrap();
        c.probe = Some((Duration::from_millis(100), Duration::from_millis(500)));
        let v = c
            .call_within("project_action", json!({}), Duration::from_secs(3600))
            .await
            .unwrap();
        assert_eq!(v["done"], 1);
    }

    // Regression: #298. A listener that accepts and never says hello hung `connect` (and the
    // desktop's whole start-up) for good.
    #[tokio::test]
    async fn connect_to_a_daemon_that_never_says_hello_fails_at_the_handshake_deadline() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(crate::paths::token(tmp.path()), "t").unwrap();
        let listener = ipc::Listener::bind(&crate::paths::socket(tmp.path())).unwrap();
        tokio::spawn(async move {
            let _held = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let t = std::time::Instant::now();
        let e = tokio::time::timeout(
            HANG,
            HostClient::connect_dir_within(tmp.path(), "test", Duration::from_millis(300)),
        )
        .await
        .expect("the handshake deadline must end the connect, not the test's own timeout")
        .err()
        .expect("a silent daemon must not connect");
        assert!(
            t.elapsed() >= Duration::from_millis(300),
            "{:?}",
            t.elapsed()
        );
        assert!(e.to_string().contains("handshake") && e.to_string().contains("hostd.log"));
        assert!(!super::is_unusable(&e), "slow is not unusable");
        assert!(super::HANDSHAKE_DEADLINE <= Duration::from_secs(5));
    }

    /// A listener that answers the hello line with `reply`, then holds the connection.
    async fn hello_replying(reply: &'static str) -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (r, mut w) = ipc::split(listener.accept().await.unwrap());
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
            std::future::pending::<()>().await;
        });
        (tmp, sock)
    }

    // #298 review: hostd queues replies behind up to 8 MiB of pty output, so on a slow tunnel `info` can take
    // longer than the probe deadline while the daemon is perfectly healthy and streaming. Only silence counts.
    #[tokio::test]
    async fn a_long_call_survives_a_slow_info_reply_while_the_daemon_streams() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (r, w) = ipc::split(listener.accept().await.unwrap());
            let w = std::sync::Arc::new(tokio::sync::Mutex::new(w));
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.lock().await.write_all(b"{\"ok\":true}\n").await.unwrap();
            let ws = w.clone();
            tokio::spawn(async move {
                loop {
                    let line = b"{\"ev\":\"pty_output\",\"pty\":\"p\",\"data\":\"eA==\"}\n";
                    if ws.lock().await.write_all(line).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
            });
            while let Ok(Some(line)) = r.next_line().await {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                let (id, delay) = (
                    v["id"].clone(),
                    if v["cmd"] == "info" { 1500 } else { 1200 },
                );
                let w = w.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    let reply = format!("{{\"id\":{id},\"ok\":true,\"data\":{{\"done\":1}}}}\n");
                    let _ = w.lock().await.write_all(reply.as_bytes()).await;
                });
            }
        });
        let mut c = HostClient::connect(&sock, "t", "test").await.unwrap();
        c.probe = Some((Duration::from_millis(100), Duration::from_millis(200)));
        let v = c
            .call_within("project_action", json!({}), Duration::from_secs(3600))
            .await
            .expect("a streaming daemon is alive: its slow `info` must not fail the call");
        assert_eq!(v["done"], 1);
    }

    // #298 review: output alone must not keep a long call alive for an hour. A daemon whose command path is stuck
    // (`info` blocked on a lock) while its readers stream pty output answers no probe: after `probe_misses` in a
    // row the call fails.
    #[tokio::test]
    async fn a_hostd_that_streams_but_never_answers_info_is_declared_silent() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (r, mut w) = ipc::split(listener.accept().await.unwrap());
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.write_all(b"{\"ok\":true}\n").await.unwrap();
            // Reads requests (never answers) and keeps streaming output, as a hostd with a blocked command path.
            tokio::spawn(async move { while let Ok(Some(_)) = r.next_line().await {} });
            loop {
                let line = b"{\"ev\":\"pty_output\",\"pty\":\"p\",\"data\":\"eA==\"}\n";
                if w.write_all(line).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let mut c = HostClient::connect(&sock, "t", "test").await.unwrap();
        c.probe = Some((Duration::from_millis(50), Duration::from_millis(50)));
        c.probe_misses = 3;
        let e = tokio::time::timeout(
            HANG,
            c.call_within("project_action", json!({}), Duration::from_secs(3600)),
        )
        .await
        .expect(
            "streaming must not keep a call that gets no probe reply alive for its class deadline",
        )
        .unwrap_err();
        // The class deadline (an hour) is far past HANG: only the missed probes can end the call.
        assert!(e.downcast_ref::<super::Unanswered>().is_some());
        assert!(c.pending.lock().as_ref().unwrap().is_empty());
    }

    // #298 review: on Windows a stalled accept loop answers "pipe busy" (`TimedOut`) while the daemon is alive.
    #[test]
    fn only_a_missing_or_refusing_socket_proves_the_daemon_dead() {
        use std::io::ErrorKind::*;
        assert!(super::proves_dead_socket(NotFound));
        assert!(super::proves_dead_socket(ConnectionRefused));
        for busy in [TimedOut, WouldBlock, PermissionDenied, Interrupted, Other] {
            assert!(!super::proves_dead_socket(busy), "{busy:?}");
        }
    }

    // #298 review: a forced restart must end a hostd whose runtime is blocked: it never runs its SIGTERM handler.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn retire_kills_a_hostd_that_ignores_sigterm() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // SIGTERM ignored before `exec`, so the `atlas hostd` process inherits it.
        let mut hostd = std::process::Command::new("bash")
            .args(["-c", "trap '' TERM; exec -a 'atlas hostd' sleep 600"])
            .spawn()
            .unwrap();
        std::fs::write(crate::paths::pid(dir), hostd.id().to_string()).unwrap();
        tokio::time::timeout(HANG, async {
            while super::live_hostd_pid(dir).is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the fake hostd never showed an `atlas hostd` command line");

        super::retire_hostd_within(dir, Duration::from_millis(100), Duration::from_millis(400))
            .await;
        // The process outlives HANG by itself, so only the kill can end it: wait for that exit event.
        let t0 = std::time::Instant::now();
        let ended = loop {
            if hostd.try_wait().unwrap().is_some() {
                break true;
            }
            if t0.elapsed() > HANG {
                break false;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = hostd.kill();
        let _ = hostd.wait();
        assert!(ended, "a hostd that ignores SIGTERM was left running");
    }

    // #298 review: during an ordinary restart the old daemon refuses the handshake for a moment while it exits;
    // `ensure_in` waits for that and starts the next one instead of reporting an outage.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_hostd_that_exits_during_the_handshake_is_replaced_not_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(crate::paths::token(dir), "t").unwrap();
        let listener = ipc::Listener::bind(&crate::paths::socket(dir)).unwrap();
        tokio::spawn(async move {
            let _held = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let mut hostd = std::process::Command::new("bash")
            .args(["-c", "exec -a 'atlas hostd' sleep 0.6"])
            .spawn()
            .unwrap();
        std::fs::write(crate::paths::pid(dir), hostd.id().to_string()).unwrap();
        tokio::time::timeout(HANG, async {
            while super::live_hostd_pid(dir).is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the fake hostd never showed an `atlas hostd` command line");
        let reaper = std::thread::spawn(move || hostd.wait());

        let e = super::ensure_in_within(
            dir,
            std::path::Path::new("/nonexistent/atlas"),
            "test",
            Duration::from_millis(300),
        )
        .await
        .err()
        .unwrap();
        let _ = reaper.join();
        // It got as far as starting a new daemon (which cannot be spawned here): not the handshake error.
        assert!(e.to_string().contains("spawn"), "{e:#}");
    }

    // #298 review: a lab daemon answers its forwarded commands inline in its read loop, so it cannot answer a
    // probe while one runs; its hello says `lab` and its calls are not probed.
    #[tokio::test]
    async fn a_lab_daemon_is_not_probed() {
        let (_t, sock) = hello_replying(r#"{"ok":true,"data":{"lab":"/main"}}"#).await;
        let c = HostClient::connect(&sock, "t", "test").await.unwrap();
        assert!(c.probe.is_none());
        let (_t, sock) = hello_replying(r#"{"ok":true,"data":{"lab":null}}"#).await;
        assert!(
            HostClient::connect(&sock, "t", "test")
                .await
                .unwrap()
                .probe
                .is_some()
        );
    }

    // #298 review: a call dropped mid-wait (the caller's own timeout, a closed window) must not leave its entry,
    // nor the probe's, in `pending`.
    #[tokio::test]
    async fn a_call_dropped_mid_probe_leaves_nothing_pending() {
        let (_tmp, mut c, _h) = fake_hostd(2).await;
        c.probe = Some((Duration::from_millis(50), Duration::from_secs(5)));
        let r = tokio::time::timeout(
            Duration::from_millis(200),
            c.call_within("project_action", json!({}), Duration::from_secs(3600)),
        )
        .await;
        assert!(
            r.is_err(),
            "the fake never answers: only the caller's timeout ends this"
        );
        assert!(c.pending.lock().as_ref().unwrap().is_empty());
    }

    // #298 review: only errors that PROVE a daemon unusable by us may lead to its retirement.
    #[tokio::test]
    async fn only_a_dead_socket_or_a_refused_token_is_unusable() {
        let (_t, sock) = hello_replying(r#"{"ok":false,"error":"unauthorized"}"#).await;
        let e = HostClient::connect(&sock, "t", "test").await.err().unwrap();
        assert!(super::is_unusable(&e), "{e:#}");

        // Any other answer is a live daemon speaking, e.g. the ones a loaded hostd gives (`hello timeout`, an
        // eviction from its pending cap, `hostd busy`): not ours to kill.
        for why in ["hello timeout", "hostd busy", "too many clients"] {
            let (_t, sock) = hello_replying(match why {
                "hello timeout" => r#"{"ok":false,"error":"hello timeout"}"#,
                "hostd busy" => r#"{"ok":false,"error":"hostd busy"}"#,
                _ => r#"{"ok":false,"error":"too many clients"}"#,
            })
            .await;
            let e = HostClient::connect(&sock, "t", "test").await.err().unwrap();
            assert!(!super::is_unusable(&e), "{why}: {e:#}");
        }

        let tmp = tempfile::tempdir().unwrap();
        let e = HostClient::connect(&tmp.path().join("none.sock"), "t", "test")
            .await
            .err()
            .unwrap();
        assert!(super::is_unusable(&e), "{e:#}");
    }

    // #298 review (blocker): the 5 s handshake made a live-but-slow hostd look "unusable", and `ensure_in` ended it
    // by SIGTERM: every terminal and crew agent died. A verified live atlas hostd that never says hello must be
    // reported, not killed.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_live_hostd_that_never_says_hello_is_reported_not_killed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(crate::paths::token(dir), "t").unwrap();
        let listener = ipc::Listener::bind(&crate::paths::socket(dir)).unwrap();
        tokio::spawn(async move {
            let _held = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        // A real process whose command line is `atlas hostd`, so `live_hostd_pid` verifies it.
        let mut hostd = std::process::Command::new("bash")
            .args(["-c", "exec -a 'atlas hostd' sleep 60"])
            .spawn()
            .unwrap();
        std::fs::write(crate::paths::pid(dir), hostd.id().to_string()).unwrap();
        let verified = async {
            while super::live_hostd_pid(dir).is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(HANG, verified)
            .await
            .expect("the fake hostd never showed an `atlas hostd` command line");

        let r = super::ensure_in_within(
            dir,
            std::path::Path::new("/nonexistent/atlas"),
            "test",
            Duration::from_millis(300),
        )
        .await;
        let alive = hostd.try_wait().unwrap().is_none();
        let _ = hostd.kill();
        let _ = hostd.wait();
        let e = r.err().expect("a daemon that never says hello is an error");
        assert!(e.to_string().contains("handshake"), "{e:#}");
        assert!(
            alive,
            "ensure_in ended a live hostd that was only slow to answer"
        );
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
            .chain(super::LONG.iter().copied())
            .chain(super::SPECIAL.iter().copied())
            .collect();
        let known: std::collections::BTreeSet<&str> = classes.iter().copied().collect();
        assert_eq!(known.len(), classes.len(), "a command is classified twice");
        let unclassified: Vec<_> = wire.difference(&known).collect();
        assert!(
            unclassified.is_empty(),
            "classify these in client.rs (QUICK, LONG or SPECIAL): {unclassified:?}"
        );
        let stale: Vec<_> = known.difference(&wire).collect();
        assert!(stale.is_empty(), "no longer hostd commands: {stale:?}");
    }

    #[test]
    fn deadlines_follow_the_class_and_the_call() {
        let none = json!({});
        assert_eq!(deadline_for("info", &none), super::CALL_DEADLINE);
        // Not provably quick: a generous cap, the liveness probe notices a wedge sooner.
        for c in [
            "pty_create",
            "worktree_list",
            "crewmate_delete",
            "lsp_install",
            "fs_remove",
        ] {
            assert_eq!(deadline_for(c, &none), Duration::from_secs(3600), "{c}");
        }
        assert!(deadline_for("project_action", &none) >= Duration::from_secs(3600));
        // `exec` outlives its own timeout by the plain deadline, so hostd's answer is not raced.
        assert_eq!(
            deadline_for("exec", &json!({"timeout_ms": 120_000})),
            Duration::from_secs(130)
        );
        // A relayed command gets the remote command's deadline plus the relay's connect wait.
        assert_eq!(
            deadline_for("crew_call", &json!({"cmd": "git_push", "args": {}})),
            Duration::from_secs(3630)
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

#[cfg(test)]
mod coded_error_tests {
    use super::*;

    fn wire(e: &anyhow::Error) -> Value {
        e.chain()
            .find_map(|c| c.downcast_ref::<Coded>())
            .expect("a Coded error")
            .wire()
    }

    /// Contract with the desktop: `apps/desktop/src/lib/__fixtures__/hostd-client-errors.json` is the REAL wire
    /// shape of every coded failure of this client (each built by the function the client uses), next to [`CODES`],
    /// which `apps/desktop/src/lib/hostText.test.ts` renders from the catalog: one message per code, or the test
    /// fails. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn client_errors_json_contract() {
        let samples = [
            closed(),
            disconnected(),
            start_failed(),
            refused("hello timeout"),
            refused("hostd busy"),
            refused("too many clients"),
            // One fixed line, not this OS's: the fixture must be byte-identical on every CI runner.
            cli_not_found_with(
                "curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash",
                "atlas CLI not found. Install it with: curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash",
            ),
        ];
        let errors: Vec<Value> = samples.iter().map(wire).collect();
        for c in CODES {
            assert!(
                errors.iter().any(|e| e["code"] == *c),
                "no sample for the code {c}"
            );
        }
        for e in &errors {
            assert!(
                CODES.contains(&e["code"].as_str().unwrap()),
                "{e} is not in CODES"
            );
        }
        let value = json!({"codes": CODES, "errors": errors});

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/hostd-client-errors.json");
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&value, &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, value,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    /// A code with a context around it (`see <log>`, added by `ensure_in`) is still found, and the text stays the
    /// one every caller printed before: the English of the whole chain.
    #[test]
    fn a_coded_error_survives_the_context_added_around_it() {
        let e: anyhow::Error = Err::<(), _>(start_failed())
            .with_context(|| "see /tmp/hostd.log")
            .unwrap_err();
        assert_eq!(wire(&e)["code"], START_FAILED);
        assert_eq!(format!("{e:#}"), "see /tmp/hostd.log: hostd did not start");
    }

    /// What every caller matched on or printed before the codes: the sentences are unchanged.
    #[test]
    fn the_english_of_every_error_is_what_it_always_was() {
        assert_eq!(closed().to_string(), "hostd closed the connection");
        assert_eq!(disconnected().to_string(), "hostd disconnected");
        assert_eq!(start_failed().to_string(), "hostd did not start");
        assert_eq!(refused("unauthorized").to_string(), "hostd: unauthorized");
        // The two refusals of a loaded daemon read as before and have a code of their own.
        assert_eq!(refused("hostd busy").to_string(), "hostd: hostd busy");
        assert_eq!(
            refused("too many clients").to_string(),
            "hostd: too many clients"
        );
        assert_eq!(wire(&refused("hostd busy"))["code"], BUSY);
        assert_eq!(
            wire(&refused("too many clients"))["code"],
            TOO_MANY_CLIENTS
        );
        assert_eq!(wire(&refused("hello timeout"))["code"], REFUSED);
        assert!(
            cli_not_found()
                .to_string()
                .starts_with("atlas CLI not found. Install it")
        );
    }
}
