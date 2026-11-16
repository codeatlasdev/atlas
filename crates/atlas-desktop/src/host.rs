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
//! (the frontend re-attaches its terminals).
//!
//! The local daemon outlives app updates, so on every (re)connect of it `reconcile` compares
//! it with this app (`atlas_host::upgrade::decide`) and acts: live `upgrade`, silent restart
//! of an empty host, or - when terminals run and features are missing - ask the user. The
//! result is a [`HostState`], kept in the hub and pushed as `host://status` (camelCase);
//! `host://prompt {ptys, reason}` asks once per daemon pid, `host://toast {kind, text}` reports
//! a finished upgrade (`warning`: the event stream lagged and the terminals were resynced). Commands: `host_status`, `host_restart`, `host_postpone`,
//! `host_restart_when_idle`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use atlas_host::HostClient;
use atlas_host::upgrade::{
    Choice, Decision, caps_of, decide, live_ptys, missing_caps, skew_reason,
};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

pub use atlas_host::worktrees::registry::LOCAL;

/// How often an armed "restart when idle" looks at the terminals (pty exits also wake it).
const IDLE_POLL: Duration = Duration::from_secs(5);

/// A connection that lived this long was a healthy one; its end is an ordinary reconnect, not a crash loop.
const STABLE_AFTER: Duration = Duration::from_secs(30);
const RECONNECT_FAST: Duration = Duration::from_millis(300);
const RECONNECT_MAX: Duration = Duration::from_secs(10);

/// How long to wait before reconnecting after a session of `lived`, and the backoff to carry forward. A stable
/// session reconnects fast and resets the backoff; a short one (the host dropped the app right after the handshake)
/// waits `backoff` and doubles it up to [`RECONNECT_MAX`].
fn reconnect_pause(lived: Duration, backoff: Duration) -> (Duration, Duration) {
    if lived >= STABLE_AFTER {
        (RECONNECT_FAST, Duration::from_secs(1))
    } else {
        (backoff, (backoff * 2).min(RECONNECT_MAX))
    }
}

/// A running terminal, as listed in the restart prompt.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HostPty {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub argv: Vec<String>,
}

/// What the webview shows about the local terminal host (`host://status`, camelCase).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostState {
    /// `current | upgrading | upgraded | outdated | restarting`.
    pub status: &'static str,
    pub version: Option<String>,
    pub proto: Option<u64>,
    pub caps: Vec<String>,
    pub app_version: String,
    /// `none | upgradable | needsRestart`.
    pub skew: &'static str,
    /// Running terminals only.
    pub ptys: Vec<HostPty>,
    /// The sentence from `skew_reason`; empty when current.
    pub reason: String,
    /// The user chose "Later" this session.
    pub deferred: bool,
    /// The user chose "Restart when idle"; the host restarts the moment no terminal runs.
    pub idle_armed: bool,
    pub missing: Vec<String>,
}

/// Everything `reconcile` and the commands share for the local daemon.
struct HostCtl {
    state: Option<HostState>,
    /// What the user said about a host that cannot be replaced without ending terminals.
    choice: Choice,
    /// Daemon pid the restart prompt was already raised for (once per daemon per session).
    prompted_pid: Option<u64>,
    /// A prompt was raised and no webview has taken it yet: the daemon is usually reconciled
    /// before the webview listens, so the event alone would be lost (`host_take_prompt`).
    prompt_pending: bool,
    /// A live upgrade / restart of the local daemon was already tried this session: the
    /// installed CLI is the only thing that can produce a new daemon, so when it is older than
    /// the app a second try would loop forever (see `plan`).
    attempted: bool,
    /// Why the last attempt to connect to or start the local daemon failed (redacted); cleared
    /// on connect. Part of the "not reachable" message and of the diagnostics.
    last_error: Option<String>,
    /// The local daemon the app last reconciled with, to tell its death from a clean handover (#276).
    served: Option<Served>,
}

/// The daemon behind the current (or last) local connection.
struct Served {
    pid: u64,
    /// The app asked it to end (upgrade, shutdown, the user's restart): its exit is no incident.
    clean: bool,
}

/// The pid of the daemon that ended without anyone asking it to, given the one the app was served by and the one it
/// is connected to now. Needs two known pids that differ: the same pid after a reconnect is a daemon that is alive
/// (the connection dropped, not the daemon), and a daemon that reports no pid (proto 1) cannot be compared.
fn lost_hostd(prev: Option<Served>, now: u64) -> Option<u64> {
    let p = prev?;
    (!p.clean && p.pid != 0 && now != 0 && p.pid != now).then_some(p.pid)
}

impl HostCtl {
    /// The app is connected to the daemon `pid`: the old pid when that replaced one that died unasked.
    fn connected(&mut self, pid: u64) -> Option<u64> {
        lost_hostd(self.served.replace(Served { pid, clean: false }), pid)
    }

    /// The app is about to end daemon `pid` (or hand over from it): its exit is expected.
    fn expect_exit(&mut self, pid: u64) {
        if let Some(s) = self.served.as_mut().filter(|s| s.pid == pid) {
            s.clean = true;
        }
    }

    /// The daemon did not leave after all (a failed upgrade): a later exit is a death again.
    fn withdraw_expected_exit(&mut self) {
        if let Some(s) = self.served.as_mut() {
            s.clean = false;
        }
    }
}

/// What the incident says: both pids, and the old daemon's crash report when it left one (its panic hook ran). Its
/// exit status is not known: the daemon is not a child of the app.
fn hostd_exit_detail(old: u64, new: u64, crash: Option<&str>) -> String {
    let mut d = format!(
        "terminal host pid {old} ended without a shutdown, upgrade or restart from the app; the app is now \
         connected to pid {new}. Its terminals ended."
    );
    if let Some(c) = crash {
        d.push_str(&format!(" Crash report: {c}."));
    }
    d
}

/// The newest `crash-<unix_ms>-<pid>[-n].toml` in `dir` written by `pid` (atlas-core's naming).
fn crash_report_of(dir: &Path, pid: u64) -> Option<String> {
    let mut found: Vec<(u128, u32, String)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let rest = name.strip_prefix("crash-")?.strip_suffix(".toml")?;
            let mut parts = rest.splitn(3, '-');
            let ms: u128 = parts.next()?.parse().ok()?;
            (parts.next()?.parse::<u64>().ok()? == pid).then_some(())?;
            let n = parts.next().map_or(Some(0), |n| n.parse().ok())?;
            Some((ms, n, name))
        })
        .collect();
    found.sort();
    found.pop().map(|(_, _, name)| name)
}

impl Default for HostCtl {
    fn default() -> Self {
        Self {
            state: None,
            choice: Choice::Ask,
            prompted_pid: None,
            prompt_pending: false,
            attempted: false,
            last_error: None,
            served: None,
        }
    }
}

#[derive(Default)]
pub struct HostHub {
    clients: RwLock<HashMap<String, HostClient>>,
    pty_host: Mutex<HashMap<String, String>>,
    /// The reconciled state of the local daemon, kept so a webview that registers its
    /// listeners after startup still learns it (`host_status`).
    ctl: Mutex<HostCtl>,
    /// Woken by `set`, so a caller waiting for the local daemon continues the moment it connects.
    connected: tokio::sync::Notify,
    /// Woken when a pty exits, so an armed "restart when idle" reacts at once.
    ptys_changed: tokio::sync::Notify,
    /// Why each remote host is not connected (last reconnect error), so a call to it says what is wrong instead of
    /// pointing at Settings ("not connected" showed even when the ship was simply powered off).
    remote_errors: Mutex<HashMap<String, String>>,
    /// Debug only: `ATLAS_DEBUG_FAKE_HOST` is on and restarts only clear the fake state.
    #[cfg(debug_assertions)]
    fake: Mutex<Option<Fake>>,
}

/// Prefix of every "the local terminal host cannot be reached" error: the frontend matches it to
/// collapse them into one toast with a "Restart terminal host" action instead of one per caller.
pub const UNREACHABLE: &str = "Terminal host not reachable";

/// What the person reads: what happened and what to do. `why` is the connect loop's last error
/// (already redacted); the log path stays out of it, "Report" carries the log tail instead.
fn unreachable_message(why: Option<&str>) -> String {
    let mut m = format!(
        "{UNREACHABLE}. Terminals cannot start or reconnect until it is back. Use Restart terminal host, or Report if it keeps happening."
    );
    if let Some(w) = why.filter(|w| !w.is_empty()) {
        m.push_str(&format!(" ({w})"));
    }
    m
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
            return Err(match self.remote_errors.lock().get(host) {
                Some(why) => format!("{host} is not reachable: {why}"),
                None => format!("{host} is still connecting; try again in a few seconds"),
            });
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            // Register interest before checking, so a `set` in between is not missed. A
            // `Notified` only counts for `notify_waiters` once polled or enabled (#226).
            let woken = self.connected.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            if let Some(c) = self.clients.read().await.get(LOCAL).cloned() {
                return Ok(c);
            }
            if tokio::time::timeout_at(deadline, woken).await.is_err() {
                let why = self.ctl.lock().last_error.clone();
                return Err(unreachable_message(why.as_deref()));
            }
        }
    }

    pub async fn set(&self, host: &str, c: Option<HostClient>) {
        let mut m = self.clients.write().await;
        if c.is_some() {
            self.remote_errors.lock().remove(host);
        }
        match c {
            Some(c) => m.insert(host.to_string(), c),
            None => m.remove(host),
        };
        drop(m);
        self.connected.notify_waiters();
    }

    /// The last reason `host` could not be (re)connected, shown by `client()` instead of a generic "not connected".
    pub fn set_remote_error(&self, host: &str, why: &str) {
        self.remote_errors
            .lock()
            .insert(host.to_string(), why.to_string());
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
            let mut env = crate::shell_env::vars_json_async().await;
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

    /// Re-learn what `host` runs (after (re)connecting, and after its events lagged): which host owns each existing
    /// pty and, for the local daemon, which terminals the restart state still lists as running. Returns the new
    /// state when it changed, for the caller to publish.
    async fn sync_ptys(
        &self,
        host: &str,
        c: &HostClient,
    ) -> Result<Option<HostState>, atlas_host::client::CallError> {
        let list = c.call("pty_list", json!({})).await?;
        {
            let mut m = self.pty_host.lock();
            for p in list["ptys"].as_array().into_iter().flatten() {
                if let Some(id) = p["pty"].as_str() {
                    m.insert(id.to_string(), host.to_string());
                }
            }
        }
        if host != LOCAL {
            return Ok(None);
        }
        let live = live_list(&list);
        let state = {
            let mut g = self.ctl.lock();
            g.state.as_mut().and_then(|s| {
                let before = s.ptys.len();
                s.ptys.retain(|p| live.iter().any(|l| l.id == p.id));
                (s.ptys.len() != before).then(|| s.clone())
            })
        };
        self.ptys_changed.notify_one();
        Ok(state)
    }

    /// Run `cmd` on every connected host; failures (e.g. an older host) are skipped.
    pub async fn call_all(&self, cmd: &str, args: Value) -> Vec<Value> {
        let clients: Vec<HostClient> = self.clients.read().await.values().cloned().collect();
        let mut out = Vec::new();
        for c in clients {
            if let Ok(v) = c.call(cmd, args.clone()).await {
                out.push(v);
            }
        }
        out
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

/// Terminal output waiting for the next frame (`pty_batch.rs`): one event per pty per frame instead of one per
/// hostd chunk, which on WebKitGTK (Linux) froze the app with two busy terminals.
static PTY_OUT: std::sync::LazyLock<Mutex<crate::pty_batch::PtyBatcher>> =
    std::sync::LazyLock::new(Default::default);
/// About one display frame.
const PTY_FLUSH: Duration = Duration::from_millis(16);

fn flush_pty_output(app: &AppHandle) {
    use base64::Engine;
    let out = PTY_OUT.lock().drain();
    for b in out {
        // Only the main window shows terminals; `emit` also evaluated every chunk in the tray webview.
        let _ = app.emit_to(
            "main",
            &format!("pty://{}", b.pty),
            json!({
                "data": base64::engine::general_purpose::STANDARD.encode(&b.bytes),
                "replay": b.replay
            }),
        );
    }
}

/// Start the per-frame flusher (once per app).
fn start_pty_flusher(app: &AppHandle) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    let app = app.clone();
    STARTED.call_once(move || {
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(PTY_FLUSH);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                if !PTY_OUT.lock().is_empty() {
                    flush_pty_output(&app);
                }
            }
        });
    });
}

fn forward(app: &AppHandle, ev: &Value) {
    match ev["ev"].as_str() {
        Some(k @ ("pty_output" | "pty_replay")) => {
            use base64::Engine;
            let pty = ev["pty"].as_str().unwrap_or_default();
            let Ok(bytes) =
                base64::engine::general_purpose::STANDARD.decode(ev["data"].as_str().unwrap_or(""))
            else {
                return;
            };
            start_pty_flusher(app);
            if PTY_OUT.lock().push(pty, &bytes, k == "pty_replay") {
                flush_pty_output(app);
            }
        }
        Some("agent_status") => {
            let _ = app.emit(
                "agent://status",
                json!({"pane_key": ev["pane_key"], "status": ev["status"]}),
            );
        }
        Some("disconnected") | None => {}
        Some(_) => {
            let _ = app.emit("host://event", ev);
        }
    }
}

/// A host that is connected but does not answer (or whose events cannot be trusted any more): one incident per
/// outage, and the webview's "Terminal host not reachable" toast with its restart action. `why` is redacted here.
fn report_unreachable(app: &AppHandle, hub: &HostHub, host: &str, why: &str) {
    let why = crate::diag::redact(why);
    if host != LOCAL {
        hub.set_remote_error(host, &why);
        return;
    }
    let first = hub.ctl.lock().last_error.replace(why.clone()).is_none();
    if first {
        crate::diag::incident("unreachable", &why);
    }
    let _ = app.emit("host://error", unreachable_message(Some(&why)));
}

/// What a (re)connect and a lag of the event stream both do: re-learn what the daemon runs (a `pty_exit` among
/// dropped events must not leave its terminal listed as running), tell the webview to re-read and re-attach.
/// `reattach` also rebuilds the native (GPU) terminals: right after a connect, never after a lag (it replays every
/// terminal, which is load on a host that just proved it cannot keep up; the webview's xterm re-attach is enough).
/// `false` when the daemon did not answer: it is reported and the caller drops the connection.
async fn resync(
    app: &AppHandle,
    hub: &HostHub,
    host: &str,
    c: &HostClient,
    reattach: bool,
) -> bool {
    match hub.sync_ptys(host, c).await {
        Ok(Some(state)) => {
            let _ = app.emit("host://status", state);
        }
        Ok(None) => {}
        Err(e) if e.downcast_ref::<atlas_host::client::Unanswered>().is_some() => {
            report_unreachable(app, hub, host, &format!("{e:#}"));
            return false;
        }
        // An older daemon without `pty_list`: nothing to learn.
        Err(_) => {}
    }
    let _ = app.emit("host://reconnected", json!({"host": host}));
    if host == LOCAL && reattach {
        crate::term::reattach_all().await;
    }
    true
}

/// One resync, one notice and one incident per this long, however many times the event stream lags: each resync
/// asks the host for more and makes the webview re-attach, which is exactly the load that causes the next lag.
const LAG_WINDOW: Duration = Duration::from_secs(5);

/// Coalesces lags: the first acts at once, later ones inside the window are counted and acted on together when it
/// closes (events lost in between may hold a `pty_exit`, so they are never just ignored).
struct LagGate {
    window: Duration,
    quiet_until: Option<std::time::Instant>,
    lost: u64,
    deferred: bool,
}

impl LagGate {
    fn new(window: Duration) -> Self {
        Self {
            window,
            quiet_until: None,
            lost: 0,
            deferred: false,
        }
    }

    /// `n` events were dropped at `now`: `Some(total dropped since the last action)` when to resync now.
    fn lagged(&mut self, now: std::time::Instant, n: u64) -> Option<u64> {
        self.lost += n;
        if self.quiet_until.is_none_or(|q| now >= q) {
            return Some(self.fire(now));
        }
        self.deferred = true;
        None
    }

    /// When the deferred resync is due, if one is waiting.
    fn due(&self) -> Option<std::time::Instant> {
        self.deferred.then_some(self.quiet_until).flatten()
    }

    /// The deferred resync, once its time came.
    fn flush(&mut self, now: std::time::Instant) -> Option<u64> {
        (self.deferred && self.due().is_some_and(|d| now >= d)).then(|| self.fire(now))
    }

    fn fire(&mut self, now: std::time::Instant) -> u64 {
        self.quiet_until = Some(now + self.window);
        self.deferred = false;
        std::mem::take(&mut self.lost)
    }
}

/// The event stream of `host` lagged by `lost` events: leave a trace the user can see and a report carries, and
/// resync as a reconnect does. `false` when the daemon did not answer.
async fn lag_resync(app: &AppHandle, hub: &HostHub, host: &str, c: &HostClient, lost: u64) -> bool {
    crate::diag::incident(
        "events-lagged",
        &format!("host {host}: {lost} events were dropped; terminals were resynced"),
    );
    let _ = app.emit(
        "host://toast",
        json!({"kind": "warning", "text": "Terminal events were dropped under load. Terminals were resynced."}),
    );
    resync(app, hub, host, c, false).await
}

/// Register a connected client and pump its events until it disconnects.
pub async fn serve_client(app: &AppHandle, host: &str, c: HostClient) {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let mut rx = c.events();
    hub.set(host, Some(c.clone())).await;
    if !resync(app, &hub, host, &c, true).await {
        hub.set(host, None).await;
        return;
    }
    let mut gate = LagGate::new(LAG_WINDOW);
    loop {
        let due = gate.due();
        // Events first: a closed connection must end the loop before a deferred resync runs against it.
        let received = tokio::select! {
            biased;
            r = rx.recv() => r,
            () = async {
                match due {
                    Some(t) => tokio::time::sleep_until(tokio::time::Instant::from_std(t)).await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(lost) = gate.flush(std::time::Instant::now())
                    && !lag_resync(app, &hub, host, &c, lost).await
                {
                    break;
                }
                continue;
            }
        };
        let ev = match received {
            Ok(ev) => ev,
            // A slow consumer lags; that is not a disconnect, but the dropped events may have held a `pty_exit`
            // (#299): resync as a reconnect does, at most once per window.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                if let Some(lost) = gate.lagged(std::time::Instant::now(), n)
                    && !lag_resync(app, &hub, host, &c, lost).await
                {
                    break;
                }
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        if ev["ev"] == "disconnected" {
            break;
        }
        if host == LOCAL && ev["ev"] == "pty_exit" {
            hub.drop_pty(app, ev["pty"].as_str().unwrap_or_default());
        }
        forward(app, &ev);
    }
    hub.set(host, None).await;
}

impl HostHub {
    /// A local terminal exited: it no longer counts in the restart prompt, and an armed
    /// "restart when idle" looks again.
    fn drop_pty(&self, app: &AppHandle, pty: &str) {
        let state = {
            let mut g = self.ctl.lock();
            let Some(s) = g.state.as_mut() else { return };
            let before = s.ptys.len();
            s.ptys.retain(|p| p.id != pty);
            (s.ptys.len() != before).then(|| s.clone())
        };
        if let Some(s) = state {
            let _ = app.emit("host://status", s);
        }
        self.ptys_changed.notify_one();
    }
}

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Running terminals out of a `pty_list` reply (exited ones are not at stake in a restart).
fn live_list(list: &Value) -> Vec<HostPty> {
    list["ptys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["exited"].is_null())
        .filter_map(|p| {
            Some(HostPty {
                id: p["pty"].as_str()?.to_string(),
                title: p["title"].as_str().unwrap_or_default().to_string(),
                cwd: p["cwd"].as_str().unwrap_or_default().to_string(),
                argv: p["argv"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect()
}

/// `info` with the `upgrade` capability removed: what `decide` must see after a handoff failed
/// (or for a daemon an upgrade was already tried on).
fn without_upgrade(info: &Value) -> Value {
    let mut i = info.clone();
    let caps: Vec<&str> = caps_of(info)
        .into_iter()
        .filter(|c| *c != "upgrade")
        .collect();
    i["caps"] = json!(caps);
    i
}

/// The state to show for `info` after `decision`. Only the skew that matters is `needsRestart`
/// (the daemon cannot be brought current without ending terminals); `Passive` keeps the
/// status `current` so nothing nags, but still carries the reason for the calm chip.
fn state_for(info: &Value, ptys: Vec<HostPty>, decision: Decision, choice: Choice) -> HostState {
    let (status, skew) = match decision {
        Decision::Current => ("current", "none"),
        Decision::Upgrade => ("upgrading", "upgradable"),
        Decision::RestartNow => ("restarting", "needsRestart"),
        Decision::Passive => ("current", "needsRestart"),
        Decision::Prompt | Decision::WaitIdle | Decision::Deferred => ("outdated", "needsRestart"),
    };
    HostState {
        status,
        version: info["version"].as_str().map(str::to_string),
        proto: info
            .is_object()
            .then(|| info["proto"].as_u64().unwrap_or(1)),
        caps: caps_of(info).into_iter().map(str::to_string).collect(),
        app_version: APP_VERSION.to_string(),
        skew,
        ptys,
        reason: skew_reason(info, APP_VERSION, &format!("Atlas {APP_VERSION}")).unwrap_or_default(),
        deferred: choice == Choice::Later && decision == Decision::Deferred,
        idle_armed: decision == Decision::WaitIdle,
        missing: missing_caps(info).into_iter().map(str::to_string).collect(),
    }
}

/// The decision for a daemon, with the loop guard applied. A live upgrade or restart can only
/// ever produce the *installed CLI's* version, so once one was tried (`attempted` is cleared
/// only when a daemon reaches this app's version, or by the user's own restart) trying again
/// cannot converge: whatever is in front of us is the product of that attempt (new pid, the
/// CLI's version) or the same daemon, and it is only reported. Keyed on the old daemon's
/// `(pid, version)` this looped forever, since every handoff mints a new pid (1.5.0-alpha.5
/// incident, see `an_app_older_than_the_installed_cli_does_not_hand_over_forever`).
fn plan(info: &Value, live: usize, choice: Choice, attempted: bool) -> Decision {
    if !attempted {
        return decide(info, live, APP_VERSION, choice);
    }
    match decide(&without_upgrade(info), live, APP_VERSION, choice) {
        Decision::RestartNow => Decision::Passive,
        d => d,
    }
}

/// The restart prompt is shown at most once per daemon pid per session, and only for
/// `Decision::Prompt` (never after "Later", never when "when idle" is armed).
fn should_prompt(decision: Decision, prompted: Option<u64>, pid: u64) -> bool {
    decision == Decision::Prompt && prompted != Some(pid)
}

impl HostHub {
    fn publish(&self, app: &AppHandle, state: HostState) {
        self.ctl.lock().state = Some(state.clone());
        let _ = app.emit("host://status", state);
    }
}

/// What the connect loop does after `reconcile`.
enum Outcome {
    /// Keep this connection.
    Serve,
    /// The daemon was replaced or is going away: drop the connection and connect again.
    Reconnect,
    /// The daemon accepted the connection but did not answer `info` / `pty_list` (wedged): nothing to reconcile
    /// against. Carries why; the connect loop reports it and retries with backoff.
    Degraded(String),
}

/// Compare the daemon we connected to with this app and act (see the module docs). Runs on
/// every (re)connect of the local daemon.
async fn reconcile(app: &AppHandle, hub: &HostHub, c: &HostClient, bin: &Path) -> Outcome {
    let (info, list) = match probe(hub, c).await {
        Ok(p) => p,
        Err(e) => return Outcome::Degraded(format!("{e:#}")),
    };
    let ptys = live_list(&list);
    let pid = info["pid"].as_u64().unwrap_or(0);
    let (choice, attempted, lost) = {
        let mut g = hub.ctl.lock();
        (g.choice, g.attempted, g.connected(pid))
    };
    if let Some(old) = lost {
        let crash = crash_report_of(&atlas_core::crash::crash_dir(), old);
        crate::diag::incident(
            "hostd-exited",
            &hostd_exit_detail(old, pid, crash.as_deref()),
        );
    }
    let mut decision = plan(&info, ptys.len(), choice, attempted);

    if decision == Decision::Upgrade {
        hub.ctl.lock().attempted = true;
        hub.publish(app, state_for(&info, ptys.clone(), decision, choice));
        hub.ctl.lock().expect_exit(pid);
        match c
            .call("upgrade", json!({"exe": bin.to_string_lossy()}))
            .await
        {
            Ok(_) => return Outcome::Reconnect,
            Err(e) => {
                hub.ctl.lock().withdraw_expected_exit();
                crate::diag::incident("upgrade-failed", &format!("{e:#}"));
                let _ = app.emit(
                    "host://error",
                    format!("Terminal host upgrade failed ({e:#}). Terminals keep running on the old host."),
                );
                // The old host keeps running: decide again as for one that cannot hand over.
                decision = plan(&info, ptys.len(), choice, hub.ctl.lock().attempted);
            }
        }
    }

    if decision == Decision::RestartNow {
        hub.ctl.lock().attempted = true;
        hub.publish(app, state_for(&info, ptys, decision, choice));
        #[cfg(debug_assertions)]
        if hub.fake.lock().is_some() {
            return Outcome::Serve;
        }
        hub.ctl.lock().expect_exit(pid);
        let _ = c.call("shutdown", json!({})).await;
        return Outcome::Reconnect;
    }

    let prompt = {
        let mut g = hub.ctl.lock();
        let p = should_prompt(decision, g.prompted_pid, pid);
        if p {
            g.prompted_pid = Some(pid);
            g.prompt_pending = true;
        }
        p
    };
    let state = state_for(&info, ptys, decision, choice);
    let ask = json!({"ptys": state.ptys, "reason": state.reason});
    // Status first: the webview opens the dialog from its own state when the prompt arrives.
    hub.publish(app, state);
    if prompt {
        let _ = app.emit("host://prompt", ask);
    }
    Outcome::Serve
}

/// `info` + `pty_list` of the local daemon (the debug fake replaces both). A daemon that does not answer in time is
/// an error, never an empty list: no terminals listed would read as "nothing to lose" and restart it.
async fn probe(
    hub: &HostHub,
    c: &HostClient,
) -> Result<(Value, Value), atlas_host::client::CallError> {
    #[cfg(debug_assertions)]
    if let Some(f) = hub.fake.lock().as_ref() {
        return Ok((f.info(), f.list()));
    }
    #[cfg(not(debug_assertions))]
    let _ = hub;
    let info = c.call("info", json!({})).await?;
    // Always listed: the prompt names the terminals, and a proto 1 daemon has no `live_ptys`.
    let list = match c.call("pty_list", json!({})).await {
        Err(e) if e.downcast_ref::<atlas_host::client::Unanswered>().is_some() => return Err(e),
        r => r.unwrap_or(json!({})),
    };
    Ok((info, list))
}

/// After a reconnect that followed an `upgrade`: the new daemon is this app's version.
fn verify_upgrade(app: &AppHandle, hub: &HostHub) {
    let g = hub.ctl.lock();
    let done = g.attempted
        && g.state
            .as_ref()
            .is_some_and(|s| s.status == "current" && s.version.as_deref() == Some(APP_VERSION));
    drop(g);
    if done {
        hub.ctl.lock().attempted = false;
        let _ = app.emit(
            "host://toast",
            json!({"kind": "success", "text": "Terminal host updated"}),
        );
    }
}

/// Running terminals of the local daemon now (`None` when it cannot be asked).
async fn live_now(hub: &HostHub) -> Option<usize> {
    #[cfg(debug_assertions)]
    if let Some(f) = hub.fake.lock().as_ref() {
        return Some(f.ptys.len());
    }
    let c = hub.clients.read().await.get(LOCAL).cloned()?;
    let info = c.call("info", json!({})).await.ok()?;
    let from_list = if info["live_ptys"].is_null() {
        live_list(&c.call("pty_list", json!({})).await.ok()?).len()
    } else {
        0
    };
    Some(live_ptys(&info, from_list))
}

/// Ask the local daemon to exit (its terminals end); `start`'s loop reconnects to a fresh one
/// from the installed CLI. With the debug fake only the fake state is cleared.
async fn restart_local(app: &AppHandle, hub: &HostHub) {
    {
        let mut g = hub.ctl.lock();
        g.attempted = false;
        g.choice = Choice::Ask;
        g.prompt_pending = false;
    }
    #[cfg(debug_assertions)]
    if hub.fake.lock().take().is_some() {
        hub.publish(app, HostState::fake_current());
        let _ = app.emit("host://reconnected", json!({"host": LOCAL}));
        return;
    }
    let mut s = hub.ctl.lock().state.clone();
    if let Some(s) = s.as_mut() {
        s.status = "restarting";
        s.idle_armed = false;
    }
    if let Some(s) = s {
        hub.publish(app, s);
    }
    // Also when there is no client: a wedged daemon is retired by `host_restart_force`.
    {
        let mut g = hub.ctl.lock();
        if let Some(pid) = g.served.as_ref().map(|s| s.pid) {
            g.expect_exit(pid);
        }
    }
    if let Some(c) = hub.clients.read().await.get(LOCAL).cloned() {
        let _ = c.call("shutdown", json!({})).await;
    }
}

/// Armed "restart when idle": restart the moment no terminal runs. Polls every [`IDLE_POLL`]
/// and on every pty exit. Runs for the whole session.
fn spawn_idle_watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let hub = app.state::<Arc<HostHub>>().inner().clone();
        loop {
            let woken = hub.ptys_changed.notified();
            let _ = tokio::time::timeout(IDLE_POLL, woken).await;
            let armed = hub.ctl.lock().state.as_ref().is_some_and(|s| s.idle_armed);
            if armed && live_now(&hub).await == Some(0) {
                restart_local(&app, &hub).await;
            }
        }
    });
}

/// App is quitting: an armed "restart when idle" with no terminal left ends the old host now,
/// so the next launch starts a current one. Terminals are never ended here.
pub fn on_exit(app: &AppHandle) {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    if !hub.ctl.lock().state.as_ref().is_some_and(|s| s.idle_armed) {
        return;
    }
    #[cfg(debug_assertions)]
    if hub.fake.lock().is_some() {
        return;
    }
    tauri::async_runtime::block_on(async {
        if live_now(&hub).await == Some(0)
            && let Some(c) = hub.clients.read().await.get(LOCAL).cloned()
        {
            let _ =
                tokio::time::timeout(Duration::from_secs(2), c.call("shutdown", json!({}))).await;
        }
    });
}

/// Connect to the local hostd (starting it if needed) and keep it alive.
pub fn start(app: &AppHandle) {
    #[cfg(debug_assertions)]
    {
        let hub = app.state::<Arc<HostHub>>();
        *hub.fake.lock() = Fake::from_env();
    }
    spawn_idle_watch(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let hub = app.state::<Arc<HostHub>>().inner().clone();
        let mut backoff = Duration::from_secs(1);
        loop {
            // The first call may wait for the login-shell probe (rc files): never on the async executor.
            let bin = match tauri::async_runtime::spawn_blocking(crate::cmds::atlas_bin)
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
            {
                Ok(b) => b,
                Err(e) => {
                    crate::diag::incident("start-failed", &e);
                    let _ = app.emit("host://error", e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            // A daemon that holds the lock but refuses this app is about to be replaced by
            // `ensure_local`: say so (its terminals end), so the report explains the loss.
            if let Some(pid) =
                atlas_host::client::unusable_hostd_pid(&atlas_host::paths::base()).await
            {
                // Said below as "restarted": not a second incident when the app connects to its replacement.
                hub.ctl.lock().expect_exit(u64::from(pid));
                crate::diag::incident(
                    "restarted",
                    &format!(
                        "terminal host pid {pid} did not accept this app; replaced it (its terminals ended)"
                    ),
                );
            }
            match atlas_host::client::ensure_local(&bin, "desktop").await {
                Ok(c) => {
                    // Every (re)connect is reconciled; the loop guard in `plan` keeps an
                    // installed CLI older than the app from looping.
                    match reconcile(&app, &hub, &c, &bin).await {
                        Outcome::Serve => {}
                        Outcome::Reconnect => {
                            // Replaced: drop this connection and connect to the new daemon
                            // (`serve_client` then emits `host://reconnected`).
                            drop(c);
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            continue;
                        }
                        Outcome::Degraded(why) => {
                            report_unreachable(&app, &hub, LOCAL, &why);
                            drop(c);
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(10));
                            continue;
                        }
                    }
                    hub.ctl.lock().last_error = None;
                    verify_upgrade(&app, &hub);
                    let since = tokio::time::Instant::now();
                    serve_client(&app, LOCAL, c).await;
                    // A host that drops the app right after the handshake must not be reconnected in a tight loop.
                    let (wait, next) = reconnect_pause(since.elapsed(), backoff);
                    backoff = next;
                    tokio::time::sleep(wait).await;
                }
                Err(e) => {
                    // One incident per outage, not one per retry.
                    report_unreachable(&app, &hub, LOCAL, &format!("{e:#}"));
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(10));
                }
            }
        }
    });
}

/// Debug only (`ATLAS_DEBUG_FAKE_HOST=proto1`, `ATLAS_DEBUG_FAKE_PTYS=<n>`): pretend the local
/// daemon is a 0.6.2 proto-1 host running `n` terminals, to exercise the prompt without
/// touching any real daemon. Restarting only clears this fake.
#[cfg(debug_assertions)]
struct Fake {
    ptys: Vec<HostPty>,
}

#[cfg(debug_assertions)]
impl Fake {
    fn from_env() -> Option<Self> {
        if std::env::var("ATLAS_DEBUG_FAKE_HOST").ok()?.is_empty() {
            return None;
        }
        let n: usize = std::env::var("ATLAS_DEBUG_FAKE_PTYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        const SAMPLES: [(&str, &str, &str); 4] = [
            ("zsh", "/Users/me/Projetos/atlas", "zsh"),
            (
                "bun run dev",
                "/Users/me/Projetos/analisa-ai/apps/web",
                "bun",
            ),
            (
                "claude",
                "/Users/me/Projetos/atlas/.worktrees/amber-fox",
                "claude",
            ),
            ("cargo watch -x test", "/Users/me/Projetos/atlas", "cargo"),
        ];
        let ptys = (0..n)
            .map(|i| {
                let (title, cwd, argv0) = SAMPLES[i % SAMPLES.len()];
                HostPty {
                    id: format!("fake-{i}"),
                    title: title.into(),
                    cwd: cwd.into(),
                    argv: vec![argv0.into()],
                }
            })
            .collect();
        Some(Self { ptys })
    }

    fn info(&self) -> Value {
        json!({"version": "0.6.2"})
    }

    fn list(&self) -> Value {
        let ptys: Vec<Value> = self
            .ptys
            .iter()
            .map(|p| json!({"pty": p.id, "title": p.title, "cwd": p.cwd, "argv": p.argv, "exited": null}))
            .collect();
        json!({"ptys": ptys})
    }
}

#[cfg(debug_assertions)]
impl HostState {
    /// The state after the fake host was "restarted": a current daemon, same path as a real one.
    fn fake_current() -> Self {
        let info = json!({"version": APP_VERSION, "proto": atlas_host::protocol::PROTO,
                          "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"]});
        state_for(&info, vec![], Decision::Current, Choice::Ask)
    }
}

/// Merge the `pty_list` replies of every host (`host` added to each item). `None` = the host
/// is not connected. The local host must answer: an empty list from a host that could not be
/// asked would read as "every terminal is gone" (1.5.0-alpha.5), so that is an error and the
/// frontend keeps what it knew. A remote that fails is skipped; its tabs stay as they were.
fn merge_pty_lists(
    replies: Vec<(String, Option<Result<Value, String>>)>,
) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut local_answered = false;
    for (h, reply) in replies {
        match reply {
            Some(Ok(v)) => {
                local_answered |= h == LOCAL;
                for mut p in v["ptys"].as_array().cloned().unwrap_or_default() {
                    p["host"] = json!(h);
                    out.push(p);
                }
            }
            Some(Err(e)) if h == LOCAL => return Err(e),
            _ => {}
        }
    }
    if local_answered {
        Ok(out)
    } else {
        Err(unreachable_message(None))
    }
}

/// `pty_list` of every connected host, merged (`host` added to each item).
#[tauri::command]
pub async fn host_pty_list(hub: tauri::State<'_, Arc<HostHub>>) -> Result<Vec<Value>, String> {
    let mut hosts: Vec<(String, Option<HostClient>)> = hub
        .clients
        .read()
        .await
        .iter()
        .map(|(k, v)| (k.clone(), Some(v.clone())))
        .collect();
    if !hosts.iter().any(|(h, _)| h == LOCAL) {
        hosts.push((LOCAL.into(), None));
    }
    let mut replies = Vec::new();
    for (h, c) in hosts {
        let reply = match c {
            Some(c) => Some(
                c.call("pty_list", json!({}))
                    .await
                    .map_err(|e| format!("{e:#}")),
            ),
            None => None,
        };
        replies.push((h, reply));
    }
    let list = merge_pty_lists(replies)?;
    for p in &list {
        if let (Some(id), Some(h)) = (p["pty"].as_str(), p["host"].as_str()) {
            hub.pty_host.lock().insert(id.to_string(), h.to_string());
        }
    }
    Ok(list)
}

/// The reconciled state of the local terminal host, `null` before the first reconcile.
#[tauri::command]
pub fn host_status(hub: tauri::State<'_, Arc<HostHub>>) -> Option<HostState> {
    hub.ctl.lock().state.clone()
}

/// The webview is ready to ask: `true` once per raised prompt (the daemon was reconciled before
/// it listened, or the event arrived while it was reloading). Cleared on read.
#[tauri::command]
pub fn host_take_prompt(hub: tauri::State<'_, Arc<HostHub>>) -> bool {
    std::mem::take(&mut hub.ctl.lock().prompt_pending)
}

/// "Restart now": the user agreed to end the terminals. The host exits; `start`'s loop
/// reconnects to a fresh one from the installed CLI (`restarting` then `current`).
#[tauri::command]
pub async fn host_restart(
    app: AppHandle,
    hub: tauri::State<'_, Arc<HostHub>>,
) -> Result<(), String> {
    restart_local(&app, &hub).await;
    Ok(())
}

/// "Restart terminal host" from an error: works when the daemon answers (graceful shutdown) and
/// when it does not (a wedged one): it is retired by its exact pid (`retire_hostd`). Ends the terminals it hosts; the frontend asked first.
#[tauri::command]
pub async fn host_restart_force(
    app: AppHandle,
    hub: tauri::State<'_, Arc<HostHub>>,
) -> Result<(), String> {
    crate::diag::incident("restarted", "restart requested from the error toast");
    restart_local(&app, &hub).await;
    // A daemon that did not act on `shutdown` (wedged) is ended by its exact pid, which `ensure_in` no longer does
    // for one that is only unresponsive: the user asked for this.
    atlas_host::client::retire_hostd(&atlas_host::paths::base(), Duration::from_secs(2)).await;
    // Drop the dead client so the next call waits for the new daemon instead of failing on it.
    hub.set(LOCAL, None).await;
    Ok(())
}

/// Everything a bug report needs: versions of the app, the installed CLI and the running
/// terminal host, the OS and the redacted tails of the logs (`diag.rs`).
#[tauri::command]
pub async fn host_diagnostics(
    hub: tauri::State<'_, Arc<HostHub>>,
) -> Result<atlas_host::bundle::Diagnostics, String> {
    let info = match hub.clients.read().await.get(LOCAL).cloned() {
        Some(c) => tokio::time::timeout(Duration::from_secs(2), c.call("info", json!({})))
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    let cli = match crate::cmds::atlas_bin() {
        Ok(bin) => crate::updates::cli_version(&bin).await,
        Err(_) => None,
    };
    let versions = atlas_host::bundle::Versions::from_info(APP_VERSION, cli, info.as_ref());
    // The probe runs bounded subprocess queries (package manager, coredumpctl): not on the runtime.
    tokio::task::spawn_blocking(move || crate::diag::build(&atlas_host::paths::base(), versions))
        .await
        .map_err(|e| format!("diagnostics probe failed: {e}"))
}

/// "Later": no prompt for the rest of this session (a new launch asks again); the chip stays.
#[tauri::command]
pub fn host_postpone(app: AppHandle, hub: tauri::State<'_, Arc<HostHub>>) {
    choose(&app, &hub, Choice::Later);
}

/// "Restart when idle": the host restarts the moment no terminal runs (or when the app
/// quits with none running).
#[tauri::command]
pub fn host_restart_when_idle(app: AppHandle, hub: tauri::State<'_, Arc<HostHub>>) {
    choose(&app, &hub, Choice::WhenIdle);
    hub.ptys_changed.notify_one();
}

/// Remember the user's choice and reflect it in the state at once.
fn choose(app: &AppHandle, hub: &HostHub, choice: Choice) {
    let state = {
        let mut g = hub.ctl.lock();
        g.choice = choice;
        g.prompt_pending = false;
        let Some(s) = g.state.as_mut() else { return };
        // Only a host that is waiting on the user changes; a current one ignores the choice.
        if s.status == "outdated" {
            s.deferred = choice == Choice::Later;
            s.idle_armed = choice == Choice::WhenIdle;
        }
        s.clone()
    };
    let _ = app.emit("host://status", state);
}

/// Generic passthrough to hostd (`pty_*`, `exec`, `fs_*`, `info`), routed by host. A `pty_attach`
/// answer's title is also sent as `pty_title`: a title the program set before this attach (PowerShell
/// sets it once, at start) was pushed while the app was not listening to that pty yet.
#[tauri::command]
pub async fn host_call(
    app: AppHandle,
    hub: tauri::State<'_, Arc<HostHub>>,
    cmd: String,
    args: Option<Value>,
) -> Result<Value, String> {
    if cmd == "shutdown" {
        return Err("shutdown is not allowed from the app".into());
    }
    let args = args.unwrap_or_else(|| json!({}));
    let pty = (cmd == "pty_attach")
        .then(|| args["pty"].as_str().map(str::to_owned))
        .flatten();
    let out = hub.call(&cmd, args).await?;
    if let Some(ev) = pty.and_then(|p| atlas_host::protocol::title_event(&p, &out)) {
        let _ = app.emit("host://event", ev);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old() -> Value {
        json!({"version": "0.6.2", "pid": 7})
    }

    fn pty(id: &str, exited: bool) -> Value {
        json!({"pty": id, "title": "zsh", "cwd": "/w", "argv": ["zsh", "-l"],
               "exited": if exited { json!({"code": 0}) } else { Value::Null }})
    }

    /// A hostd that says hello, then on its first `info` pushes `flood` output events and a `pty_exit` for `p1`
    /// BEFORE replying (same stream, so a client whose `info` returned has broadcast them all), and lists `p1`
    /// as exited in `pty_list`.
    async fn hostd_that_floods(flood: usize) -> (tempfile::TempDir, HostClient) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let listener = atlas_core::ipc::Listener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (r, mut w) = atlas_core::ipc::split(listener.accept().await.unwrap());
            let mut r = BufReader::new(r).lines();
            r.next_line().await.unwrap();
            w.write_all(b"{\"ok\":true}\n").await.unwrap();
            while let Ok(Some(line)) = r.next_line().await {
                let v: Value = serde_json::from_str(&line).unwrap();
                let data = match v["cmd"].as_str() {
                    Some("info") => {
                        let mut out = String::new();
                        for _ in 0..flood {
                            out.push_str(
                                "{\"ev\":\"pty_output\",\"pty\":\"p1\",\"data\":\"eA==\"}\n",
                            );
                        }
                        out.push_str("{\"ev\":\"pty_exit\",\"pty\":\"p1\",\"code\":0}\n");
                        w.write_all(out.as_bytes()).await.unwrap();
                        json!({})
                    }
                    Some("pty_list") => json!({"ptys": [pty("p1", true)]}),
                    _ => json!({}),
                };
                let reply = json!({"id": v["id"], "ok": true, "data": data});
                w.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
            }
        });
        let c = HostClient::connect(&sock, "t", "test").await.unwrap();
        (tmp, c)
    }

    // Regression: #299. `serve_client` answered `RecvError::Lagged` with an `eprintln!` and went on: a
    // `pty_exit` among the dropped events left the terminal in the restart prompt (and the pty index) as
    // running until the next reconnect. The resync every lag now runs is the reconnect's own.
    #[tokio::test]
    async fn a_pty_exit_lost_to_event_lag_is_found_by_the_resync() {
        let (_tmp, c) = hostd_that_floods(5000).await;
        let mut rx = c.events();
        c.call("info", json!({})).await.unwrap();
        // 5000 events + the exit overflowed the 4096 slots: the receiver learns it lagged, not what it missed.
        assert!(matches!(
            rx.recv().await,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
        let hub = HostHub::default();
        let running = HostPty {
            id: "p1".into(),
            title: "zsh".into(),
            cwd: "/w".into(),
            argv: vec![],
        };
        hub.ctl.lock().state = Some(state_for(
            &json!({"version": APP_VERSION, "pid": 7}),
            vec![running],
            Decision::Current,
            Choice::Ask,
        ));

        let state = hub
            .sync_ptys(LOCAL, &c)
            .await
            .unwrap()
            .expect("the state changed: p1 is no longer running");
        assert!(state.ptys.is_empty());
        assert!(hub.ctl.lock().state.as_ref().unwrap().ptys.is_empty());
        // The pty is still routed to its host (it exists, exited): attach and reads keep working.
        assert_eq!(
            hub.pty_host.lock().get("p1").map(String::as_str),
            Some(LOCAL)
        );
    }

    // #299 review: every lag used to toast, record an incident, resync and rebuild the GPU terminals, which is the
    // load that causes the next lag. One action per window, the dropped events counted, the tail never lost.
    #[test]
    fn a_lag_storm_acts_once_per_window_and_never_loses_the_tail() {
        let t0 = std::time::Instant::now();
        let s = Duration::from_secs;
        let mut g = LagGate::new(s(5));
        assert_eq!(g.lagged(t0, 100), Some(100));
        assert_eq!(g.lagged(t0 + s(1), 50), None);
        assert_eq!(g.lagged(t0 + s(2), 25), None);
        // The deferred resync is due when the window closes, and carries what was lost since.
        assert_eq!(g.due(), Some(t0 + s(5)));
        assert_eq!(g.flush(t0 + s(4)), None);
        assert_eq!(g.flush(t0 + s(5)), Some(75));
        assert_eq!(g.due(), None);
        assert_eq!(g.flush(t0 + s(6)), None, "nothing deferred: nothing to do");
        // That action opened a new window.
        assert_eq!(g.lagged(t0 + s(7), 10), None);
        assert_eq!(g.lagged(t0 + s(10), 5), Some(15));
        // A lag long after the last one acts at once.
        assert_eq!(g.lagged(t0 + s(60), 1), Some(1));
    }

    #[test]
    fn only_running_terminals_are_listed_with_their_shape() {
        let l = live_list(&json!({"ptys": [pty("a", false), pty("b", true), {"title": "no id"}]}));
        assert_eq!(
            l,
            vec![HostPty {
                id: "a".into(),
                title: "zsh".into(),
                cwd: "/w".into(),
                argv: vec!["zsh".into(), "-l".into()],
            }]
        );
        assert!(live_list(&json!({})).is_empty());
    }

    #[test]
    fn state_serializes_camel_case_per_the_contract() {
        let s = state_for(&old(), vec![], Decision::Prompt, Choice::Ask);
        let v = serde_json::to_value(&s).unwrap();
        for k in [
            "status",
            "version",
            "proto",
            "caps",
            "appVersion",
            "skew",
            "ptys",
            "reason",
            "deferred",
            "idleArmed",
            "missing",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(
            (v["status"].as_str(), v["skew"].as_str()),
            (Some("outdated"), Some("needsRestart"))
        );
        assert_eq!(v["proto"], 1, "a daemon that reports no proto is proto 1");
        assert_eq!(v["missing"], json!(atlas_host::upgrade::NEEDED));
        assert!(
            v["reason"]
                .as_str()
                .unwrap()
                .starts_with("Terminal host 0.6.2 is older than Atlas")
        );
    }

    #[test]
    fn status_and_skew_per_decision() {
        let t = |d, c| {
            let s = state_for(&old(), vec![], d, c);
            (s.status, s.skew, s.deferred, s.idle_armed)
        };
        assert_eq!(
            t(Decision::Upgrade, Choice::Ask),
            ("upgrading", "upgradable", false, false)
        );
        assert_eq!(
            t(Decision::RestartNow, Choice::Ask),
            ("restarting", "needsRestart", false, false)
        );
        assert_eq!(
            t(Decision::Prompt, Choice::Ask),
            ("outdated", "needsRestart", false, false)
        );
        assert_eq!(
            t(Decision::WaitIdle, Choice::WhenIdle),
            ("outdated", "needsRestart", false, true)
        );
        assert_eq!(
            t(Decision::Deferred, Choice::Later),
            ("outdated", "needsRestart", true, false)
        );
        // Only the version differs: calm, no nag, but the skew is still visible.
        assert_eq!(
            t(Decision::Passive, Choice::Ask),
            ("current", "needsRestart", false, false)
        );
    }

    #[test]
    fn a_current_host_has_no_reason_and_nothing_missing() {
        let info = json!({"version": APP_VERSION, "pid": 1, "proto": 2,
                          "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"]});
        let s = state_for(&info, vec![], Decision::Current, Choice::Ask);
        assert_eq!((s.status, s.skew), ("current", "none"));
        assert!(s.reason.is_empty() && s.missing.is_empty());
        assert_eq!(s.proto, Some(2));
    }

    #[test]
    fn the_prompt_is_shown_once_per_daemon_and_only_when_asking() {
        assert!(should_prompt(Decision::Prompt, None, 7));
        assert!(!should_prompt(Decision::Prompt, Some(7), 7));
        assert!(
            should_prompt(Decision::Prompt, Some(7), 8),
            "a new daemon asks again"
        );
        for d in [
            Decision::WaitIdle,
            Decision::Deferred,
            Decision::Passive,
            Decision::Current,
        ] {
            assert!(!should_prompt(d, None, 7));
        }
    }

    #[test]
    fn a_replacement_is_tried_once_per_session() {
        let upgradeable = json!({"version": "0.6.2", "pid": 7,
                                 "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"]});
        assert_eq!(plan(&upgradeable, 3, Choice::Ask, false), Decision::Upgrade);
        // Same daemon still there: the CLI cannot make it current, so it is only reported.
        assert_eq!(plan(&upgradeable, 3, Choice::Ask, true), Decision::Passive);
        // An empty host the restart did not replace must not be restarted in a loop either.
        assert_eq!(plan(&old(), 0, Choice::Ask, false), Decision::RestartNow);
        assert_eq!(plan(&old(), 0, Choice::Ask, true), Decision::Passive);
        // The daemon a handoff produced has a NEW pid and the CLI's version: still only reported.
        let product = json!({"version": "0.6.3", "pid": 8,
                             "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"]});
        assert_eq!(plan(&product, 3, Choice::Ask, true), Decision::Passive);
    }

    #[test]
    fn a_failed_handoff_falls_back_to_the_user_path() {
        let info = json!({"version": "0.9.0", "pid": 7, "caps": ["pty_attach_raw", "upgrade"]});
        let stripped = without_upgrade(&info);
        assert_eq!(caps_of(&stripped), vec!["pty_attach_raw"]);
        assert_eq!(
            decide(&stripped, 2, APP_VERSION, Choice::Ask),
            Decision::Prompt
        );
    }

    #[test]
    fn choices_decide_what_a_blocked_host_does() {
        let d = |c| plan(&old(), 2, c, false);
        assert_eq!(d(Choice::Ask), Decision::Prompt);
        assert_eq!(d(Choice::WhenIdle), Decision::WaitIdle);
        assert_eq!(d(Choice::Later), Decision::Deferred);
    }

    /// Incident 1.5.0-alpha.5 (maintainer's `~/.atlas/hostd.log`: 11 consecutive
    /// `1.5.0-alpha.5 ... took over 2 pty(s)` lines within seconds of `atlas update`).
    /// The running app was still alpha.4 while the freshly installed CLI was alpha.5: every
    /// handoff produced a daemon at the CLI's version with a NEW pid, which `plan` (keyed on
    /// `(pid, version)`) treated as an unseen daemon, so it handed over again, forever; each
    /// handoff drops the connection, which is what the user saw as "hostd is not reachable"
    /// toasts and "Terminal is gone".
    ///
    /// Models the loop of `start()`: connect -> `plan` -> `upgrade` (new pid, CLI version) ->
    /// reconnect. A sane guard hands over at most once per app run for a given target version.
    #[test]
    fn an_app_older_than_the_installed_cli_does_not_hand_over_forever() {
        let cli_version = "9.9.9"; // the installed CLI; this app is APP_VERSION (older)
        assert_ne!(cli_version, APP_VERSION);
        let mut attempted = false;
        let mut pid = 100;
        let mut info = json!({"version": "0.0.1", "pid": pid,
                              "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"], "live_ptys": 2});
        let mut handoffs = 0;
        for _ in 0..50 {
            match plan(&info, 2, Choice::Ask, attempted) {
                Decision::Upgrade => {
                    handoffs += 1;
                    attempted = true;
                    pid += 1;
                    info = json!({"version": cli_version, "pid": pid,
                                  "caps": ["pty_attach_raw", "upgrade", "lsp", "agents"], "live_ptys": 2});
                }
                _ => break,
            }
        }
        assert!(
            handoffs <= 1,
            "{handoffs} handoffs in a row: the daemon is replaced in a loop"
        );
    }

    /// Incident 1.5.0-alpha.5, "Terminal is gone": `host_pty_list` returned `[]` (no error) while
    /// the local client was absent (between a handoff's disconnect and the reconnect) or its
    /// `pty_list` failed, so the frontend replaced its terminal map with `{}` and showed every
    /// tab as gone, with a Restart that then called the same absent host.
    #[test]
    fn an_unreachable_local_host_is_an_error_not_an_empty_list() {
        let ok = |ptys: Value| Some(Ok(json!({"ptys": ptys})));
        // Local answered with nothing: a true empty list.
        assert_eq!(
            merge_pty_lists(vec![(LOCAL.into(), ok(json!([])))])
                .unwrap()
                .len(),
            0
        );
        // Local not connected at all.
        let e = merge_pty_lists(vec![]).unwrap_err();
        assert!(e.contains("not reachable"), "{e}");
        // Local connected but the call failed.
        let failed: Option<Result<Value, String>> = Some(Err("hostd disconnected".into()));
        let e = merge_pty_lists(vec![(LOCAL.into(), failed)]).unwrap_err();
        assert!(e.contains("hostd disconnected"), "{e}");
    }

    #[test]
    fn a_remote_that_fails_does_not_hide_the_local_terminals() {
        let local = Some(Ok(json!({"ptys": [{"pty": "a"}]})));
        let remote: Option<Result<Value, String>> = Some(Err("timeout".into()));
        let out = merge_pty_lists(vec![(LOCAL.into(), local), ("ssh:box".into(), remote)]).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["host"], LOCAL);
        assert_eq!(out[0]["pty"], "a");
    }

    /// The text of the toast: what happened and what to do, never a path to a log.
    #[test]
    fn the_unreachable_message_says_what_to_do_and_names_no_log_path() {
        let m = unreachable_message(None);
        assert!(m.contains("Restart terminal host"), "{m}");
        assert!(!m.contains("hostd.log") && !m.contains('~'), "{m}");
        let m = unreachable_message(Some("socket in use"));
        assert!(m.contains("socket in use"), "{m}");
    }

    /// Incident class: after `serve_client` returned, `start` went straight back to `ensure_local` with no pause and
    /// with the backoff reset by the connect. A daemon that accepts the app and then dies (or refuses it right after
    /// the handshake) made the app spawn, connect and drop in a tight loop: a core pinned, `host://reconnected` and
    /// `reattach_all` firing at the UI as fast as the machine allowed.
    #[test]
    fn a_connection_that_drops_at_once_backs_off_instead_of_spinning() {
        let one = Duration::from_secs(1);
        let mut b = one;
        let mut waits = vec![];
        for _ in 0..6 {
            let (wait, next) = reconnect_pause(Duration::from_millis(40), b);
            waits.push(wait);
            b = next;
        }
        assert_eq!(waits[0], one, "the very first drop already waits");
        assert!(waits.windows(2).all(|w| w[1] >= w[0]), "{waits:?}");
        assert_eq!(
            *waits.last().unwrap(),
            Duration::from_secs(10),
            "capped, never beyond"
        );
    }

    /// A session that lived for a while was a healthy one: its end is an ordinary reconnect (an update's handoff, a
    /// daemon restart), not part of a crash loop, so it comes back fast and the backoff starts over.
    #[test]
    fn a_long_session_that_ends_reconnects_quickly_and_resets_the_backoff() {
        let (wait, next) = reconnect_pause(Duration::from_secs(120), Duration::from_secs(10));
        assert!(wait <= Duration::from_millis(300), "{wait:?}");
        assert_eq!(next, Duration::from_secs(1));
    }

    /// The edge: exactly at the stability threshold counts as stable; one tick under does not.
    #[test]
    fn the_stability_threshold_is_a_clean_edge() {
        let b = Duration::from_secs(4);
        assert!(reconnect_pause(STABLE_AFTER, b).0 < b);
        assert!(reconnect_pause(STABLE_AFTER - Duration::from_millis(1), b).0 >= b);
    }

    fn served(pid: u64, clean: bool) -> Option<Served> {
        Some(Served { pid, clean })
    }

    // Regression: #276. A hostd that aborted and was respawned by `ensure_local` left nothing in app.log or the
    // report, so "my terminals vanished" had no trace. A daemon that ended without anyone asking is one incident.
    #[test]
    fn a_daemon_replaced_without_being_asked_to_leave_is_a_death() {
        assert_eq!(lost_hostd(served(10, false), 11), Some(10));
    }

    #[test]
    fn a_shutdown_an_upgrade_or_a_user_restart_is_not_a_death() {
        assert_eq!(lost_hostd(served(10, true), 11), None);
    }

    /// The same pid after a reconnect is a daemon that is alive (the connection dropped, not the daemon), the first
    /// connect has nothing to compare, and a daemon that reports no pid (proto 1, the debug fake) cannot be compared.
    #[test]
    fn nothing_is_reported_without_two_known_pids_that_differ() {
        assert_eq!(lost_hostd(served(10, false), 10), None);
        assert_eq!(lost_hostd(None, 10), None);
        assert_eq!(lost_hostd(served(0, false), 10), None);
        assert_eq!(lost_hostd(served(10, false), 0), None);
    }

    /// A crash loop is one incident per death, not per reconnect: reconnecting to the same pid says nothing, every
    /// new pid says one thing, and a clean restart in the middle costs none.
    #[test]
    fn a_reconnect_loop_reports_each_death_once() {
        let mut ctl = HostCtl::default();
        let mut deaths = vec![];
        for pid in [10, 10, 10, 11, 11, 12] {
            deaths.extend(ctl.connected(pid));
        }
        assert_eq!(deaths, [10, 11]);
        ctl.expect_exit(12);
        assert_eq!(ctl.connected(13), None, "the user's restart");
        assert_eq!(
            ctl.connected(14),
            Some(13),
            "and the next death counts again"
        );
    }

    /// `expect_exit` names the daemon being asked to leave: a restart of one daemon must not excuse the death of
    /// another, and a failed upgrade (the daemon keeps running) must not excuse its later death.
    #[test]
    fn an_expected_exit_excuses_only_that_daemon_and_only_until_it_is_withdrawn() {
        let mut ctl = HostCtl::default();
        ctl.connected(10);
        ctl.expect_exit(99);
        assert_eq!(ctl.connected(11), Some(10));
        ctl.expect_exit(11);
        ctl.withdraw_expected_exit();
        assert_eq!(ctl.connected(12), Some(11));
    }

    /// The silence itself, through the state `reconcile` and the restart commands drive: the daemon the app asked to
    /// leave (an upgrade, a shutdown, the user's Restart) is replaced without an incident, and a daemon that stayed
    /// after being asked (the restart did nothing) is not excused for the death after it.
    #[test]
    fn the_daemon_the_app_asked_to_leave_is_replaced_without_an_incident() {
        let mut ctl = HostCtl::default();
        ctl.connected(10);
        ctl.expect_exit(10);
        assert_eq!(ctl.connected(11), None, "handover or restart");
        ctl.expect_exit(11);
        assert_eq!(ctl.connected(11), None, "asked, but it stayed");
        assert_eq!(
            ctl.connected(12),
            Some(11),
            "its later death is not excused"
        );
    }

    #[test]
    fn the_incident_names_both_pids_and_the_crash_report_of_the_old_one() {
        let d = hostd_exit_detail(10, 11, Some("crash-1-10.toml"));
        assert!(d.contains("pid 10") && d.contains("pid 11"), "{d}");
        assert!(d.contains("crash-1-10.toml"), "{d}");
        assert!(
            !hostd_exit_detail(10, 11, None).contains("crash"),
            "no report, no mention"
        );
    }

    /// `crash-<unix_ms>-<pid>[-n].toml` (atlas-core): the pid is the old daemon's, never a longer pid that ends
    /// the same way, and the newest report wins.
    #[test]
    fn the_crash_report_is_found_by_the_exact_pid() {
        let tmp = tempfile::tempdir().unwrap();
        for f in [
            "crash-100-10.toml",
            "crash-200-10-1.toml",
            "crash-300-110.toml",
            "crash-400-7.toml",
            "notes.txt",
        ] {
            std::fs::write(tmp.path().join(f), "").unwrap();
        }
        assert_eq!(
            crash_report_of(tmp.path(), 10).as_deref(),
            Some("crash-200-10-1.toml")
        );
        assert_eq!(crash_report_of(tmp.path(), 1), None);
        assert_eq!(crash_report_of(&tmp.path().join("missing"), 10), None);
    }

    /// A real client against a real socket whose daemon is killed mid-session (it vanishes without a goodbye),
    /// then a new daemon with another pid: exactly the sequence `start` goes through, one death reported.
    #[tokio::test]
    async fn killing_the_daemon_mid_session_yields_one_death_on_the_next_connect() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        async fn daemon(pid: u64) -> (tempfile::TempDir, tokio::task::JoinHandle<()>, HostClient) {
            let tmp = tempfile::tempdir().unwrap();
            let sock = tmp.path().join("h.sock");
            let listener = atlas_core::ipc::Listener::bind(&sock).unwrap();
            let task = tokio::spawn(async move {
                let (r, mut w) = atlas_core::ipc::split(listener.accept().await.unwrap());
                let mut r = BufReader::new(r).lines();
                r.next_line().await.unwrap();
                w.write_all(b"{\"ok\":true}\n").await.unwrap();
                while let Ok(Some(line)) = r.next_line().await {
                    let v: Value = serde_json::from_str(&line).unwrap();
                    let reply = json!({"id": v["id"], "ok": true, "data": {"pid": pid}});
                    w.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
                }
            });
            let c = HostClient::connect(&sock, "t", "test").await.unwrap();
            (tmp, task, c)
        }
        let mut ctl = HostCtl::default();
        let (_a, killed, first) = daemon(4242).await;
        let pid = first.call("info", json!({})).await.unwrap()["pid"]
            .as_u64()
            .unwrap();
        assert_eq!(ctl.connected(pid), None);
        // SIGKILL: the socket closes, nothing was said.
        killed.abort();
        let _ = killed.await;
        tokio::time::timeout(Duration::from_secs(5), first.closed())
            .await
            .unwrap();
        let (_b, _task, second) = daemon(4243).await;
        let pid = second.call("info", json!({})).await.unwrap()["pid"]
            .as_u64()
            .unwrap();
        assert_eq!(ctl.connected(pid), Some(4242));
        assert_eq!(ctl.connected(pid), None, "once");
    }
}
