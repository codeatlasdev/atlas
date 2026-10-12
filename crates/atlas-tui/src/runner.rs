//! The runner: `atlas dev --headless` / `atlas dev --detach` engine, plus the
//! server side of the attach protocol.
//!
//! One loop owns the [`ServiceManager`]. A [`Hub`] sits next to it: it keeps the
//! recent log tail and side events in memory, and fans every manager event out to
//! subscribed clients as one JSON line each. A client's `subscribe` is answered
//! with a snapshot built **in the same loop turn** that registers its stream, so
//! the snapshot and the events that follow are gap-free and duplicate-free.
//!
//! Wire events (`"t"` discriminates), all produced by [`Hub::on_event`]:
//!
//! | `t` | payload |
//! |---|---|
//! | `log` | `name`, `line`, `stream` (`stdout`/`stderr`) |
//! | `state` | `name`, `state` + the service facts (`exit_code`, `restarts`, `note`, `completed_ok`, `paused_by`, `deselected`) |
//! | `rows` | every service row + `tunnel`, sent at most once a second when changed |
//! | `note` | `name`, `message` (scheduler narration) |
//! | `alert` | `service`, `message` |
//! | `tunnel` | `state` + `tunnel` object |
//! | `action_progress` | `label`, `line` |
//! | `action_done` | `label`, `ok` |
//! | `started` | all app services were queued |
//! | `bye` | the runner is shutting down |
//!
//! The snapshot also carries `action_runs` (oldest first): per recent action run
//! `label`, `start`/`end` (epoch ms, `end` null while running), `status`
//! (`running`/`ok`/`failed`), the last [`ACTION_TAIL_LINES`] output `lines` and how many
//! earlier lines were `skipped`. Older runners omit it; clients treat it as optional.

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{Value, json};
use tokio::signal;
use tokio::signal::unix::{Signal, SignalKind};
use tokio::sync::mpsc;

use crate::config::{self, ProjectConfig};
use crate::control::{
    self, ControlCmd, ControlRequest, ControlResponse, ControlServer, Resolved, resolve,
};
use crate::runtime::SingletonLock;
use crate::runtime::deps::{DepsOutcome, ensure_deps};
use crate::runtime::logs::{LogBuffer, LogEntry, LogStream};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

/// Log lines kept for a newly attached client (per service ring, like the TUI's).
const LOG_TAIL_PER_SERVICE: usize = 1000;
/// Raw side events replayed to a new client (the sidebar shows far fewer).
const EVENT_HISTORY: usize = 50;
/// Action runs remembered for a newly attached client.
const ACTION_RUNS: usize = 10;
/// Output lines kept per remembered action run.
const ACTION_TAIL_LINES: usize = 500;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// One execution of a non-session action, as replayed to new clients.
struct ActionRun {
    label: String,
    start: u64,
    /// `None` while running, else whether it exited 0.
    done: Option<(u64, bool)>,
    tail: VecDeque<String>,
    skipped: u64,
}

impl ActionRun {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            start: now_ms(),
            done: None,
            tail: VecDeque::new(),
            skipped: 0,
        }
    }

    fn push(&mut self, line: &str) {
        if self.tail.len() >= ACTION_TAIL_LINES {
            self.tail.pop_front();
            self.skipped += 1;
        }
        self.tail.push_back(line.to_string());
    }

    fn json(&self) -> Value {
        let (status, end) = match self.done {
            None => ("running", Value::Null),
            Some((t, true)) => ("ok", t.into()),
            Some((t, false)) => ("failed", t.into()),
        };
        json!({
            "label": self.label, "start": self.start, "end": end, "status": status,
            "lines": self.tail, "skipped": self.skipped,
        })
    }
}

/// How the headless engine was started: attached to a terminal, or `--detach`ed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// `atlas dev --headless`: streams service output to stdout.
    Foreground,
    /// `atlas dev --detach`: stdout/stderr go to `.logs/runner.log`; service output
    /// is only in the per-service disk logs and the attach stream.
    Background,
}

pub const RUNNER_LOG: &str = "runner.log";

pub(crate) fn stream_name(stream: LogStream) -> &'static str {
    match stream {
        LogStream::Stdout => "stdout",
        LogStream::Stderr => "stderr",
    }
}

/// Wire form of one log entry. `t`/`name`/`line`/`stream` are the original
/// fields; `seq`, `ts` (epoch ms), `level` (`debug|info|warn|error`), `kind`
/// (`line|restart|mark`) and `trunc` are additive: old readers ignore them.
pub(crate) fn log_json(e: &LogEntry) -> Value {
    let mut v = json!({
        "t": "log", "name": e.service, "line": e.content,
        "stream": stream_name(e.stream), "seq": e.seq, "ts": e.timestamp,
        "level": e.level.wire(),
    });
    if e.restart {
        v["kind"] = "restart".into();
    } else if e.mark {
        v["kind"] = "mark".into();
    }
    if e.truncated {
        v["trunc"] = true.into();
    }
    v
}

pub(crate) fn state_name(state: &ServiceState) -> &'static str {
    match state {
        ServiceState::Stopped => "stopped",
        ServiceState::Starting => "starting",
        ServiceState::Running => "running",
        ServiceState::Failed => "failed",
        ServiceState::Disabled => "disabled",
        ServiceState::Suspended => "suspended",
    }
}

pub(crate) fn parse_state(s: &str) -> Option<ServiceState> {
    Some(match s {
        "stopped" => ServiceState::Stopped,
        "starting" => ServiceState::Starting,
        "running" => ServiceState::Running,
        "failed" => ServiceState::Failed,
        "disabled" => ServiceState::Disabled,
        "suspended" => ServiceState::Suspended,
        _ => return None,
    })
}

/// State + facts of one service, as sent on the wire (`state` events and `rows`).
fn service_json(manager: &ServiceManager, name: &str, state: &ServiceState) -> Value {
    json!({
        "name": name,
        "state": state_name(state),
        "exit_code": manager.exit_code(name),
        "restarts": manager.restart_count(name),
        "note": manager.service_note(name),
        "completed_ok": manager.completed_ok(name),
        "paused_by": manager.paused_by(name),
        "deselected": manager.is_deselected(name),
        // Whole MB / whole percent: finer values would re-send `rows` every sample.
        "mem_mb": manager.usage(name).map(|u| u.mem >> 20),
        "cpu": manager.usage(name).map(|u| u.cpu.round() as u32),
    })
}

fn tunnel_json(manager: &ServiceManager) -> Value {
    match manager.tunnel_status() {
        Some(t) => json!({
            "state": state_name(&t.state),
            "local_port": t.local_port,
            "remote": t.remote,
            "ssh_host": t.ssh_host,
        }),
        None => Value::Null,
    }
}

/// An attached client. A burst can outrun it; rather than cutting it off (it would
/// reconnect and lose the burst anyway), its log lines are skipped and counted, and a
/// `gap` event tells it how many once it reads again. Other events are never skipped.
struct Subscriber {
    tx: mpsc::Sender<String>,
    skipped: u64,
}

impl Subscriber {
    /// `false` = the connection is gone.
    fn send(&mut self, line: &str, is_log: bool) -> bool {
        use mpsc::error::TrySendError;
        if self.skipped > 0 {
            let gap = json!({"t": "gap", "skipped": self.skipped}).to_string();
            match self.tx.try_send(gap) {
                Ok(()) => self.skipped = 0,
                Err(TrySendError::Closed(_)) => return false,
                Err(TrySendError::Full(_)) => {}
            }
        }
        if self.skipped > 0 && is_log {
            self.skipped += 1;
            return true;
        }
        match self.tx.try_send(line.to_string()) {
            Ok(()) => true,
            Err(TrySendError::Closed(_)) => false,
            // A full buffer on a state event means the client is truly stuck.
            Err(TrySendError::Full(_)) if is_log => {
                self.skipped += 1;
                true
            }
            Err(TrySendError::Full(_)) => false,
        }
    }
}

/// Memory of the run plus the attached clients.
pub struct Hub {
    project: String,
    logs: LogBuffer,
    /// Recent `state`/`note`/`alert`/`tunnel` events, replayed to new clients.
    history: VecDeque<Value>,
    subscribers: Vec<Subscriber>,
    /// Last `rows` payload sent; a new one goes out only when it differs.
    last_rows: String,
    started: bool,
    /// Non-session action in flight (`label`, last output line).
    operation: Option<(String, String)>,
    /// Recent action runs, oldest first, for clients that attach later.
    action_runs: VecDeque<ActionRun>,
    mode: &'static str,
}

impl Hub {
    pub fn new(project: &str) -> Self {
        Self {
            project: project.to_string(),
            logs: LogBuffer::new(LOG_TAIL_PER_SERVICE),
            history: VecDeque::with_capacity(EVENT_HISTORY),
            subscribers: Vec::new(),
            last_rows: String::new(),
            started: false,
            operation: None,
            action_runs: VecDeque::with_capacity(ACTION_RUNS),
            mode: "runner",
        }
    }

    pub fn clients(&self) -> usize {
        self.subscribers.len()
    }

    fn broadcast(&mut self, ev: &Value) {
        if self.subscribers.is_empty() {
            return;
        }
        let line = ev.to_string();
        let is_log = ev["t"] == "log";
        self.subscribers.retain_mut(|s| s.send(&line, is_log));
    }

    fn remember(&mut self, ev: Value) {
        if self.history.len() >= EVENT_HISTORY {
            self.history.pop_front();
        }
        self.broadcast(&ev);
        self.history.push_back(ev);
    }

    /// Record one manager event and push it to subscribers. Called by the run loop
    /// *before* the manager reacts, so `state` facts describe the transition.
    pub fn on_event(&mut self, manager: &ServiceManager, event: &ManagerEvent) {
        match event {
            ManagerEvent::LogLine { name, line, stream } => {
                let ev = log_json(self.logs.push(name.clone(), line.clone(), *stream));
                self.broadcast(&ev);
            }
            ManagerEvent::ServiceRestarted { name } => {
                if self.logs.filter_service(name).is_empty() {
                    return;
                }
                self.logs.push_restart(name.clone());
                if let Some(e) = self.logs.last() {
                    self.broadcast(&log_json(e));
                }
            }
            ManagerEvent::StateChanged { name, state } => {
                let mut ev = service_json(manager, name, state);
                ev["t"] = "state".into();
                self.remember(ev);
            }
            ManagerEvent::ServiceNote { name, message } => {
                self.remember(json!({ "t": "note", "name": name, "message": message }));
            }
            ManagerEvent::CriticalAlert { service, message } => {
                self.remember(json!({ "t": "alert", "service": service, "message": message }));
            }
            ManagerEvent::TunnelState { state } => {
                self.remember(
                    json!({ "t": "tunnel", "state": state_name(state), "tunnel": tunnel_json(manager) }),
                );
            }
            ManagerEvent::ActionProgress { label, line } => {
                if let Some(op) = self.operation.as_mut()
                    && op.0 == *label
                {
                    op.1 = line.clone();
                }
                self.action_run(label).push(line);
                self.broadcast(&json!({ "t": "action_progress", "label": label, "line": line }));
            }
            ManagerEvent::ActionCompleted { label, ok, .. } => {
                if self.operation.as_ref().is_some_and(|o| o.0 == *label) {
                    self.operation = None;
                }
                // Empty label = the manager's resume ping after a session, not an action.
                if !label.is_empty() {
                    self.action_run(label).done = Some((now_ms(), *ok));
                }
                self.broadcast(&json!({ "t": "action_done", "label": label, "ok": ok }));
            }
            ManagerEvent::AllStarted => {
                self.started = true;
                self.broadcast(&json!({ "t": "started" }));
            }
            _ => {}
        }
    }

    /// Per-second upkeep: drop clients that hung up (so `--status` counts only live
    /// ones), then push the `rows` event when anything a row shows changed.
    pub fn tick(&mut self, manager: &mut ServiceManager) {
        self.subscribers.retain(|s| !s.tx.is_closed());
        manager.set_clients(self.clients());
        if self.subscribers.is_empty() {
            return;
        }
        let ev = self.rows_event(manager);
        let text = ev.to_string();
        if text != self.last_rows {
            self.last_rows = text;
            self.broadcast(&ev);
        }
    }

    fn rows_event(&self, manager: &ServiceManager) -> Value {
        let rows: Vec<Value> = manager
            .services()
            .iter()
            .map(|s| service_json(manager, &s.name, &s.state))
            .collect();
        json!({ "t": "rows", "services": rows, "tunnel": tunnel_json(manager) })
    }

    /// The unfinished run of `label` (a label has at most one in flight), started on
    /// first need: output and completion may arrive without a prior `start`.
    fn action_run(&mut self, label: &str) -> &mut ActionRun {
        let open = self
            .action_runs
            .iter()
            .rposition(|r| r.label == label && r.done.is_none());
        let i = open.unwrap_or_else(|| {
            if self.action_runs.len() >= ACTION_RUNS {
                self.action_runs.pop_front();
            }
            self.action_runs.push_back(ActionRun::new(label));
            self.action_runs.len() - 1
        });
        &mut self.action_runs[i]
    }

    /// Everything a new client needs; registering `stream` in the same call is what
    /// makes the following events gap-free.
    fn subscribe(&mut self, manager: &ServiceManager, stream: mpsc::Sender<String>) -> Value {
        let mut rows = self.rows_event(manager);
        let logs: Vec<Value> = self
            .logs
            .all()
            .iter()
            .map(|e| {
                let mut v = log_json(e);
                v["name"] = e.service.clone().into();
                v.as_object_mut().map(|o| o.remove("t"));
                v
            })
            .collect();
        let operation = self
            .operation
            .as_ref()
            .map(|(label, detail)| json!({ "label": label, "detail": detail }));
        let action_runs: Vec<Value> = self.action_runs.iter().map(ActionRun::json).collect();
        self.subscribers.push(Subscriber {
            tx: stream,
            skipped: 0,
        });
        // The client's first `rows` event must not be skipped for being "unchanged".
        self.last_rows.clear();
        json!({
            "project": self.project,
            "pid": std::process::id(),
            "mode": self.mode,
            "version": env!("CARGO_PKG_VERSION"),
            "started": self.started,
            "services": rows["services"].take(),
            "tunnel": rows["tunnel"].take(),
            "logs": logs,
            "events": self.history,
            "operation": operation,
            "action_runs": action_runs,
        })
    }

    /// Tell every client the runner is going away (they then exit their TUI).
    pub fn bye(&mut self) {
        self.broadcast(&json!({ "t": "bye" }));
        self.subscribers.clear();
    }

    /// Handle one control request on the runner. Returns `true` for `shutdown`.
    pub async fn handle(
        &mut self,
        manager: &mut ServiceManager,
        cfg: &ProjectConfig,
        req: ControlRequest,
    ) -> bool {
        let ControlRequest { cmd, reply, stream } = req;
        let resp = match cmd {
            ControlCmd::Info => ControlResponse::ok(json!({
                "mode": self.mode,
                "pid": std::process::id(),
                "project": self.project,
                "clients": self.clients(),
                "version": env!("CARGO_PKG_VERSION"),
            })),
            ControlCmd::Shutdown => {
                let _ = reply.send(ControlResponse::ok(json!({ "stopping": true })));
                return true;
            }
            ControlCmd::Subscribe => match stream {
                Some(stream) => {
                    let snapshot = self.subscribe(manager, stream);
                    manager.set_clients(self.clients());
                    ControlResponse::ok(snapshot)
                }
                None => ControlResponse::err("subscribe needs a streaming connection"),
            },
            service_cmd => self.service_command(manager, cfg, service_cmd).await,
        };
        let _ = reply.send(resp);
        // A client that went away since the last turn must not keep the count up.
        self.subscribers.retain(|s| !s.tx.is_closed());
        manager.set_clients(self.clients());
        false
    }

    async fn service_command(
        &mut self,
        manager: &mut ServiceManager,
        cfg: &ProjectConfig,
        cmd: ControlCmd,
    ) -> ControlResponse {
        match resolve(cmd, cfg) {
            Err(e) => ControlResponse::err(e),
            Ok(Resolved::StartService(n) | Resolved::RestartService(n)) => {
                manager.restart_service(&n).await;
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::StopService(n)) => {
                manager.stop_service_disabled(&n).await;
                ControlResponse::ok(json!({ "service": n, "queued": true }))
            }
            Ok(Resolved::RunAction { key, label }) => {
                let action = manager.find_action(key);
                if action.as_ref().is_some_and(|a| a.platform.is_some()) {
                    ControlResponse::err(format!(
                        "action '{label}' needs the interactive device picker; run it from an owner TUI (atlas down, then atlas dev)"
                    ))
                } else if action.as_ref().is_some_and(|a| a.session) {
                    ControlResponse::err(format!(
                        "action '{label}' is an interactive session; sessions are owner-only (atlas down, then atlas dev)"
                    ))
                } else if manager.action_running(&label) {
                    ControlResponse::err(format!("action '{label}' is already running"))
                } else {
                    manager.run_action(key).await;
                    self.operation = Some((label.clone(), String::new()));
                    ControlResponse::ok(json!({ "action": label, "queued": true }))
                }
            }
        }
    }
}

/// Refuse to start when another atlas owns the project (the CLI prints the error and exits 1).
pub(crate) fn lock_or_err(root_dir: &Path) -> Result<SingletonLock> {
    let log_dir = crate::runtime::log_dir(root_dir);
    let lock_path = log_dir.join(".tui.lock");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| anyhow::anyhow!("atlas dev: cannot create {}: {e}", log_dir.display()))?;
    SingletonLock::acquire(&lock_path).map_err(|_| match SingletonLock::existing_pid(&lock_path) {
        Some(pid) => {
            anyhow::anyhow!("atlas dev is already running (pid {pid}); use 'atlas dev --status'")
        }
        None => anyhow::anyhow!("atlas dev is already running; use 'atlas dev --status'"),
    })
}

pub(crate) fn term_signals() -> Result<(Signal, Signal)> {
    Ok((
        signal::unix::signal(SignalKind::terminate())?,
        signal::unix::signal(SignalKind::hangup())?,
    ))
}

/// Same `pre_command` staleness logic as the TUI, but before any service starts.
async fn run_deps(root_dir: &Path, cfg: &ProjectConfig) {
    match ensure_deps(
        root_dir,
        cfg.pre_command.as_deref(),
        cfg.pre_command_when,
        false,
    )
    .await
    {
        DepsOutcome::UpToDate => {}
        DepsOutcome::Installed => eprintln!("\x1b[32m✓\x1b[0m dependencies installed"),
        DepsOutcome::Failed(reason) => {
            eprintln!("\x1b[31m✗\x1b[0m pre_command failed: {reason} (continuing)")
        }
    }
}

/// The headless engine's state: services, health, control socket, status heartbeat,
/// disk logs, and the attach stream. Built by `RunnerLoop::start`, consumed by `run`.
struct RunnerLoop {
    cfg: ProjectConfig,
    foreground: bool,
    manager: ServiceManager,
    hub: Hub,
    event_rx: mpsc::UnboundedReceiver<ManagerEvent>,
    control_rx: Option<mpsc::UnboundedReceiver<ControlRequest>>,
    sigterm: Signal,
    sighup: Signal,
    /// Watches the docker daemon and alerts when it dies or recovers; aborted on shutdown.
    watchdog: tokio::task::JoinHandle<()>,
}

impl RunnerLoop {
    /// Print the banner, build the manager and start every selected service.
    async fn start(
        root_dir: &Path,
        cfg: ProjectConfig,
        selection: &config::Selection,
        foreground: bool,
        control_rx: Option<mpsc::UnboundedReceiver<ControlRequest>>,
    ) -> Result<Self> {
        println!(
            "\x1b[1;34m●\x1b[0m atlas dev ({})",
            if foreground { "headless" } else { "runner" }
        );
        println!("  project: {}", cfg.name);
        match selection.names() {
            Some(names) => println!(
                "  services: {} of {} ({})",
                names.len(),
                cfg.services.len(),
                names
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => println!("  services: {}", cfg.services.len()),
        }
        println!();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut manager =
            ServiceManager::with_selection(root_dir.to_path_buf(), &cfg, event_tx, selection);
        manager.set_mode("runner");
        let hub = Hub::new(&cfg.name);
        let (sigterm, sighup) = term_signals()?;

        run_deps(root_dir, &cfg).await;

        manager.start_all().await;

        let watchdog = tokio::spawn(crate::runtime::docker::watch_daemon(
            manager.event_tx.clone(),
        ));

        if foreground {
            println!("\x1b[1;32m✓\x1b[0m Starting services. Press Ctrl+C to stop.");
        } else {
            println!("\x1b[1;32m✓\x1b[0m Starting services. Stop with: atlas down");
        }
        println!();

        Ok(Self {
            cfg,
            foreground,
            manager,
            hub,
            event_rx,
            control_rx,
            sigterm,
            sighup,
            watchdog,
        })
    }

    /// Single loop owns the manager: forwards logs and, on InfraReady, starts
    /// the app services exactly like the TUI effect loop does. The 1s tick runs
    /// check_health (heartbeat for `--status`, reaping of exited children).
    async fn run(mut self) -> Result<()> {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = signal::ctrl_c(), if self.foreground => break,
                _ = self.sigterm.recv() => break,
                _ = self.sighup.recv() => break,
                Some(req) = async { self.control_rx.as_mut().unwrap().recv().await }, if self.control_rx.is_some() => {
                    if self.hub.handle(&mut self.manager, &self.cfg, req).await {
                        break;
                    }
                }
                _ = tick.tick() => {
                    self.manager.check_health().await;
                    self.hub.tick(&mut self.manager);
                }
                Some(event) = self.event_rx.recv() => {
                    self.hub.on_event(&self.manager, &event);
                    handle_event(&mut self.manager, event, self.foreground).await;
                }
            }
        }

        println!();
        println!("\x1b[33m●\x1b[0m Shutting down...");

        self.watchdog.abort();
        self.hub.bye();
        self.manager.stop_all().await;

        println!("\x1b[32m✓\x1b[0m Done.");
        Ok(())
    }
}

/// The headless engine: services, health, control socket, status heartbeat, disk
/// logs, and the attach stream. Runs until SIGTERM/SIGHUP/Ctrl-C or `shutdown`.
pub async fn run(root_dir: &Path, select: &config::SelectionRequest, kind: RunKind) -> Result<()> {
    let cfg = config::load(root_dir)?;
    let selection = config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;
    // Held for the whole run: the singleton lock and the control socket.
    let _lock = lock_or_err(root_dir)?;
    let (_control, control_rx) = match ControlServer::bind(root_dir) {
        Ok((s, rx)) => (Some(s), Some(rx)),
        Err(_) => (None, None),
    };
    RunnerLoop::start(
        root_dir,
        cfg,
        &selection,
        kind == RunKind::Foreground,
        control_rx,
    )
    .await?
    .run()
    .await
}

/// What the runner does with one manager event (printing + the scheduler hooks the
/// TUI effect loop would otherwise run).
async fn handle_event(manager: &mut ServiceManager, event: ManagerEvent, foreground: bool) {
    match event {
        ManagerEvent::InfraReady { app_names } => {
            manager.start_app_services(&app_names).await;
        }
        // Action output is printed from ActionProgress below.
        ManagerEvent::LogLine { name, .. } if name.starts_with("action:") => {}
        // Service output has its own disk log; only a foreground run echoes it.
        ManagerEvent::LogLine { name, line, .. } => {
            if foreground {
                let prefix: String = name.chars().take(5).collect();
                println!("[{prefix:<5}] {line}");
            }
        }
        ManagerEvent::StateChanged { name, state } => {
            let icon = match state {
                ServiceState::Running => "\x1b[32m●\x1b[0m",
                ServiceState::Starting => "\x1b[33m◌\x1b[0m",
                ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                ServiceState::Stopped => "\x1b[90m○\x1b[0m",
                ServiceState::Disabled => "\x1b[90m⊘\x1b[0m",
                ServiceState::Suspended => "\x1b[90m⏸\x1b[0m",
            };
            eprintln!("{icon} {name}: {state:?}");
            manager.update_state(&name, state);
        }
        ManagerEvent::CriticalAlert { message, .. } => {
            eprintln!("  [!] {message}");
        }
        ManagerEvent::ServiceNote { name, message } => {
            eprintln!("\x1b[36m↻\x1b[0m {name}: {message}");
        }
        ManagerEvent::LogReady { name } => manager.mark_log_ready(&name),
        ManagerEvent::TunnelState { state } => {
            eprintln!("\x1b[36m⇄\x1b[0m tunnel: {state:?}");
            manager.write_status_file();
        }
        ManagerEvent::ActionProgress { label, line } => {
            println!("[action {label}] {line}");
        }
        ManagerEvent::ActionCompleted { label, ok, resume } => {
            let (icon, word) = if ok {
                ("\x1b[32m✓\x1b[0m", "done")
            } else {
                ("\x1b[31m✗\x1b[0m", "failed")
            };
            eprintln!("{icon} action {label}: {word}");
            // Same as the TUI: services paused for the action come back afterwards.
            for name in resume {
                manager.restart_service(&name).await;
            }
        }
        _ => {}
    }
}

/// Result of `atlas dev --detach`.
pub enum Detached {
    Started {
        pid: i32,
    },
    /// A runner already owns the project (selection arguments are not applied to it).
    AlreadyRunning {
        pid: i64,
    },
}

/// Last 64 KiB of a log file (lossy UTF-8); empty when it cannot be read. A runner
/// that crashed at startup may leave a huge log; the error is at the end.
fn read_log_tail(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL_BYTES: u64 = 64 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    if f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
        .is_err()
    {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// `atlas dev --detach`: validate, start the runner as a detached daemon and wait
/// until it is reachable.
pub async fn detach(root_dir: &Path, select: &config::SelectionRequest) -> Result<Detached> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    // Fail on a bad atlas.yaml / selection here, on the user's terminal, not in a log file.
    let cfg = config::load(root_dir)?;
    config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;

    if let Some(info) = control::info(root_dir).await {
        if info.mode == "runner" {
            return Ok(Detached::AlreadyRunning { pid: info.pid });
        }
        anyhow::bail!(
            "atlas dev is already running in a terminal (pid {}); quit it first, or stop it with atlas down",
            info.pid
        );
    }

    let log_dir = crate::runtime::log_dir(root_dir);
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(RUNNER_LOG);
    let out = std::fs::File::create(&log_path)?;
    let err = out.try_clone()?;

    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("dev")
        .arg("--runner")
        .arg("--dir")
        .arg(root_dir)
        .args(&select.services)
        .args(
            select
                .profiles
                .iter()
                .flat_map(|p| ["--profile", p.as_str()]),
        )
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    if select.no_deps {
        cmd.arg("--no-deps");
    }
    // New session: no controlling terminal, so closing this shell never signals
    // the runner (SIGHUP) and a tty read/write can never stop it (SIGTTIN/OU).
    // SAFETY: setsid is async-signal-safe and touches no Rust state.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    let pid = child.id() as i32;

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait()? {
            let tail = read_log_tail(&log_path)
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::bail!(
                "runner exited during startup ({status}); see {}\n{tail}",
                log_path.display()
            );
        }
        if let Some(info) = control::info(root_dir).await
            && info.pid == i64::from(pid)
        {
            return Ok(Detached::Started { pid });
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "runner (pid {pid}) did not open its control socket within 60s; see {}",
                log_path.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `atlas down`: ask the running atlas to stop everything and wait for it to go.
pub enum DownOutcome {
    NotRunning,
    Stopped {
        pid: i64,
    },
    /// Still alive after the wait.
    Timeout {
        pid: i64,
    },
}

pub async fn down(root_dir: &Path, wait: Duration) -> Result<DownOutcome, String> {
    let Some(info) = control::info(root_dir).await else {
        return Ok(DownOutcome::NotRunning);
    };
    let resp = control::call(root_dir, "shutdown", json!({})).await?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "shutdown refused".into()));
    }
    Ok(wait_gone(root_dir, info.pid, wait).await)
}

/// Wait until the atlas with `pid` has exited (process dead, or its lock free, so a
/// recycled pid cannot fool us).
pub async fn wait_gone(root_dir: &Path, pid: i64, wait: Duration) -> DownOutcome {
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
        if !alive || SingletonLock::existing_pid(&root_dir.join(".logs/.tui.lock")).is_none() {
            return DownOutcome::Stopped { pid };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    DownOutcome::Timeout { pid }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_subscriber_skips_log_lines_and_hears_about_the_gap() {
        let (tx, mut rx) = mpsc::channel(2);
        let mut s = Subscriber { tx, skipped: 0 };
        for i in 0..5 {
            assert!(s.send(&format!("log{i}"), true));
        }
        assert_eq!(s.skipped, 3);
        assert_eq!(rx.try_recv().unwrap(), "log0");
        assert_eq!(rx.try_recv().unwrap(), "log1");
        // Room again: the gap goes first, then the new line.
        assert!(s.send("log5", true));
        let gap: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(gap, json!({"t": "gap", "skipped": 3}));
        assert_eq!(rx.try_recv().unwrap(), "log5");
        // A full buffer on a non-log event still drops the subscriber.
        assert!(s.send("a", true) && s.send("b", true));
        assert!(!s.send("state", false));
        drop(rx);
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        assert!(!Subscriber { tx, skipped: 0 }.send("x", true));
    }

    fn cfg() -> ProjectConfig {
        serde_yaml::from_str(
            r#"
name: demo
services:
  api: { command: "sleep 30", port: 3000 }
actions:
  - { key: m, label: Migrate, command: "true" }
  - { key: x, label: Shell, command: "sh", session: true }
"#,
        )
        .unwrap()
    }

    fn manager(cfg: &ProjectConfig) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            ServiceManager::new(std::env::temp_dir().join("atlas-hub-test"), cfg, tx),
            rx,
        )
    }

    fn log(name: &str, line: &str) -> ManagerEvent {
        ManagerEvent::LogLine {
            name: name.into(),
            line: line.into(),
            stream: LogStream::Stdout,
        }
    }

    #[test]
    fn state_names_roundtrip() {
        for s in [
            ServiceState::Stopped,
            ServiceState::Starting,
            ServiceState::Running,
            ServiceState::Failed,
            ServiceState::Disabled,
            ServiceState::Suspended,
        ] {
            assert_eq!(parse_state(state_name(&s)), Some(s));
        }
        assert_eq!(parse_state("nope"), None);
    }

    #[test]
    fn snapshot_has_tail_then_events_stream_in_order_without_gaps() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        hub.on_event(&m, &log("api", "before-1"));
        hub.on_event(&m, &log("api", "before-2"));

        let (tx, mut rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        let snap = hub.subscribe(&m, tx);
        let lines: Vec<&str> = snap["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["line"].as_str().unwrap())
            .collect();
        assert_eq!(lines, ["before-1", "before-2"]);
        assert_eq!(snap["project"], "demo");
        assert_eq!(snap["services"][0]["name"], "api");

        hub.on_event(&m, &log("api", "after-1"));
        hub.on_event(&m, &log("api", "after-2"));
        let got: Vec<Value> = std::iter::from_fn(|| rx.try_recv().ok())
            .map(|l| serde_json::from_str(&l).unwrap())
            .collect();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0]["line"], "after-1");
        assert_eq!(got[1]["line"], "after-2");
    }

    #[test]
    fn two_clients_each_get_every_event_and_a_dropped_one_is_pruned() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        let (t1, mut r1) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        let (t2, r2) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        hub.subscribe(&m, t1);
        hub.subscribe(&m, t2);
        assert_eq!(hub.clients(), 2);

        drop(r2); // client 2 disconnects
        hub.on_event(&m, &log("api", "x"));
        assert_eq!(hub.clients(), 1, "dead subscriber pruned on next broadcast");
        let first: Value = serde_json::from_str(&r1.try_recv().unwrap()).unwrap();
        assert_eq!(first["line"], "x", "survivor unaffected");
    }

    #[test]
    fn history_is_bounded_and_replayed_to_new_clients() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        for i in 0..(EVENT_HISTORY + 10) {
            hub.on_event(
                &m,
                &ManagerEvent::ServiceNote {
                    name: "api".into(),
                    message: format!("n{i}"),
                },
            );
        }
        let (tx, _rx2) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        let snap = hub.subscribe(&m, tx);
        let ev = snap["events"].as_array().unwrap();
        assert_eq!(ev.len(), EVENT_HISTORY);
        assert_eq!(
            ev.last().unwrap()["message"],
            format!("n{}", EVENT_HISTORY + 9)
        );
    }

    fn progress(label: &str, line: &str) -> ManagerEvent {
        ManagerEvent::ActionProgress {
            label: label.into(),
            line: line.into(),
        }
    }

    fn done(label: &str, ok: bool) -> ManagerEvent {
        ManagerEvent::ActionCompleted {
            label: label.into(),
            ok,
            resume: vec![],
        }
    }

    fn snapshot_runs(hub: &mut Hub, m: &ServiceManager) -> Vec<Value> {
        let (tx, _rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        hub.subscribe(m, tx)["action_runs"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn snapshot_replays_action_runs_with_status_and_tail() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        hub.on_event(&m, &progress("Migrate", "step 1"));
        hub.on_event(&m, &progress("Migrate", "step 2"));
        hub.on_event(&m, &done("Migrate", true));
        hub.on_event(&m, &progress("Seed", "rows"));

        let runs = snapshot_runs(&mut hub, &m);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["label"], "Migrate");
        assert_eq!(runs[0]["status"], "ok");
        assert!(runs[0]["end"].is_u64());
        assert_eq!(runs[0]["lines"], json!(["step 1", "step 2"]));
        assert_eq!(runs[1]["label"], "Seed");
        assert_eq!(runs[1]["status"], "running");
        assert!(runs[1]["end"].is_null(), "no end while running");

        // A second run of the same label is a new run, not an append to the finished one.
        hub.on_event(&m, &progress("Migrate", "again"));
        hub.on_event(&m, &done("Migrate", false));
        let runs = snapshot_runs(&mut hub, &m);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[2]["status"], "failed");
        assert_eq!(runs[2]["lines"], json!(["again"]));
    }

    #[test]
    fn action_run_tail_and_run_count_are_bounded() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        for i in 0..(ACTION_TAIL_LINES + 7) {
            hub.on_event(&m, &progress("Big", &format!("l{i}")));
        }
        let runs = snapshot_runs(&mut hub, &m);
        let lines = runs[0]["lines"].as_array().unwrap();
        assert_eq!(lines.len(), ACTION_TAIL_LINES);
        assert_eq!(lines[0], "l7");
        assert_eq!(runs[0]["skipped"], 7);

        for i in 0..(ACTION_RUNS + 3) {
            let label = format!("run{i}");
            hub.on_event(&m, &progress(&label, "x"));
            hub.on_event(&m, &done(&label, true));
        }
        let runs = snapshot_runs(&mut hub, &m);
        assert_eq!(runs.len(), ACTION_RUNS);
        assert_eq!(
            runs.last().unwrap()["label"],
            format!("run{}", ACTION_RUNS + 2)
        );
    }

    #[test]
    fn session_resume_pings_and_silent_actions_are_handled() {
        let c = cfg();
        let (m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        // Empty label = resume ping: never a run.
        hub.on_event(&m, &done("", true));
        assert!(snapshot_runs(&mut hub, &m).is_empty());
        // An action that printed nothing still shows up once it completes.
        hub.on_event(&m, &done("Quiet", true));
        let runs = snapshot_runs(&mut hub, &m);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["status"], "ok");
        assert_eq!(runs[0]["lines"], json!([]));
    }

    #[test]
    fn rows_only_sent_when_changed() {
        let c = cfg();
        let (mut m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");
        let (tx, mut rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
        hub.subscribe(&m, tx);
        hub.tick(&mut m);
        assert!(
            rx.try_recv().is_ok(),
            "first tick after subscribe sends rows"
        );
        hub.tick(&mut m);
        assert!(rx.try_recv().is_err(), "unchanged rows are not resent");
        m.update_state("api", ServiceState::Running);
        hub.tick(&mut m);
        let v: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(v["t"], "rows");
        assert_eq!(v["services"][0]["state"], "running");
    }

    #[tokio::test]
    async fn runner_refuses_sessions_and_serves_info() {
        let c = cfg();
        let (mut m, _rx) = manager(&c);
        let mut hub = Hub::new("demo");

        let ask = |cmd| {
            let (reply, rx) = tokio::sync::oneshot::channel();
            (
                ControlRequest {
                    cmd,
                    reply,
                    stream: None,
                },
                rx,
            )
        };
        let (req, rx) = ask(ControlCmd::Info);
        assert!(!hub.handle(&mut m, &c, req).await);
        let info = rx.await.unwrap();
        assert_eq!(info.data.unwrap()["mode"], "runner");

        let (req, rx) = ask(ControlCmd::RunAction {
            target: "x".into(),
            confirmed: true,
        });
        hub.handle(&mut m, &c, req).await;
        let r = rx.await.unwrap();
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("owner-only"));

        let (req, _rx) = ask(ControlCmd::Shutdown);
        assert!(
            hub.handle(&mut m, &c, req).await,
            "shutdown asks the loop to stop"
        );
    }

    // ── protocol over a real socket ──────────────────────────────────────────────

    /// A runner loop in miniature: the real `ControlServer` + `Hub::handle`, with the
    /// test pushing manager events through `Hub::on_event` like the real loop does.
    struct MiniRunner {
        dir: tempfile::TempDir,
        events: mpsc::UnboundedSender<ManagerEvent>,
        stopped: tokio::sync::oneshot::Receiver<()>,
        task: tokio::task::JoinHandle<()>,
    }

    fn mini_runner() -> MiniRunner {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg();
        let (server, mut control_rx) = ControlServer::bind(dir.path()).unwrap();
        let (events, mut event_rx) = mpsc::unbounded_channel::<ManagerEvent>();
        let (stop_tx, stopped) = tokio::sync::oneshot::channel();
        let root = dir.path().to_path_buf();
        let task = tokio::spawn(async move {
            let _server = server;
            let (tx, _keep) = mpsc::unbounded_channel();
            let mut m = ServiceManager::new(root, &c, tx);
            let mut hub = Hub::new("demo");
            loop {
                tokio::select! {
                    Some(req) = control_rx.recv() => {
                        if hub.handle(&mut m, &c, req).await {
                            hub.bye();
                            let _ = stop_tx.send(());
                            return;
                        }
                    }
                    Some(ev) = event_rx.recv() => hub.on_event(&m, &ev),
                }
            }
        });
        MiniRunner {
            dir,
            events,
            stopped,
            task,
        }
    }

    #[tokio::test]
    async fn subscribe_over_the_socket_snapshot_then_ordered_stream() {
        let r = mini_runner();
        for i in 0..3 {
            r.events.send(log("api", &format!("old-{i}"))).unwrap();
        }
        // Let the loop apply the three before-events before subscribing.
        let info = loop {
            let i = control::info(r.dir.path()).await.unwrap();
            if i.clients == 0 {
                break i;
            }
        };
        assert_eq!(info.mode, "runner");
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut sub = control::subscribe(r.dir.path()).await.unwrap();
        let backlog: Vec<&str> = sub.snapshot["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["line"].as_str().unwrap())
            .collect();
        assert_eq!(backlog, ["old-0", "old-1", "old-2"]);

        for i in 0..50 {
            r.events.send(log("api", &format!("new-{i}"))).unwrap();
        }
        for i in 0..50 {
            let ev = tokio::time::timeout(Duration::from_secs(5), sub.next())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(ev["t"], "log");
            assert_eq!(
                ev["line"],
                format!("new-{i}"),
                "stream keeps order, no gap/dupe"
            );
        }
        r.task.abort();
    }

    #[tokio::test]
    async fn many_clients_and_a_disconnect_do_not_disturb_the_runner() {
        let r = mini_runner();
        let mut a = control::subscribe(r.dir.path()).await.unwrap();
        let mut b = control::subscribe(r.dir.path()).await.unwrap();
        let c = control::subscribe(r.dir.path()).await.unwrap();
        assert_eq!(control::info(r.dir.path()).await.unwrap().clients, 3);

        drop(c); // a client vanishes mid-session
        r.events.send(log("api", "after-drop")).unwrap();
        for sub in [&mut a, &mut b] {
            let ev = tokio::time::timeout(Duration::from_secs(5), sub.next())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(ev["line"], "after-drop");
        }
        // The runner still answers requests, and the dead client is no longer counted.
        let mut clients = 3;
        for _ in 0..40 {
            r.events.send(log("api", "poke")).unwrap();
            clients = control::info(r.dir.path()).await.unwrap().clients;
            if clients == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(clients, 2);
        r.task.abort();
    }

    #[tokio::test]
    async fn shutdown_says_bye_to_clients_and_stops_the_loop() {
        let r = mini_runner();
        let mut sub = control::subscribe(r.dir.path()).await.unwrap();
        let resp = control::call(r.dir.path(), "shutdown", json!({}))
            .await
            .unwrap();
        assert!(resp.ok);
        r.stopped.await.unwrap();
        let bye = tokio::time::timeout(Duration::from_secs(5), sub.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bye["t"], "bye");
        assert!(sub.next().await.is_none(), "stream closes after bye");
    }

    #[tokio::test]
    async fn sending_garbage_on_a_subscription_does_not_break_other_clients() {
        let r = mini_runner();
        let mut good = control::subscribe(r.dir.path()).await.unwrap();
        // A raw subscriber that writes junk and hangs up.
        {
            use tokio::io::AsyncWriteExt;
            let mut raw = tokio::net::UnixStream::connect(control::socket_path(r.dir.path()))
                .await
                .unwrap();
            raw.write_all(b"{\"cmd\":\"subscribe\"}\n").await.unwrap();
            raw.write_all(b"not json at all\n").await.unwrap();
        }
        r.events.send(log("api", "still-fine")).unwrap();
        let ev = tokio::time::timeout(Duration::from_secs(5), good.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ev["line"], "still-fine");
        r.task.abort();
    }
}
