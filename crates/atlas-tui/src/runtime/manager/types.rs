use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::ChildStdin;

use crate::runtime::logs::LogStream;
use crate::runtime::service::{ServiceState, ServiceStatus};

use crate::config::{Condition, RestartPolicy};

use super::policy::RestartTracker;
use crate::runtime::memory::{Budget, BudgetMachine, LeakDetector, Limits, Series};

/// A service's memory bookkeeping: its budget, the RSS series of its process tree and the leak
/// watch. The rules live in `runtime::memory`; `mem_watch.rs` applies them to a sample.
pub(crate) struct MemWatch {
    pub(crate) limits: Limits,
    pub(crate) on_max: crate::config::OnMemoryMax,
    pub(crate) series: Series,
    pub(crate) budget: BudgetMachine,
    pub(crate) leak: LeakDetector,
    /// `max_memory` restarts this run; past [`super::health_check::MEM_RESTARTS`] the service stops.
    pub(crate) restarts: u32,
    /// The last RSS reading of the previous process, kept for the cycle a respawn closes.
    pub(crate) last_rss: Option<u64>,
    /// Cycles the last leak alert counted: the next one waits for `LEAK_REALERT_CYCLES` more.
    pub(crate) alerted_cycles: Option<usize>,
    /// The service is `degraded` only because of the memory budget (so it clears on its own).
    pub(crate) degrading: bool,
}

/// Gap between two resource samples (memory/CPU of every service tree).
pub(crate) const USAGE_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

impl MemWatch {
    pub(crate) fn from_def(def: &crate::config::ServiceDef) -> Self {
        let size = |v: &Option<String>| v.as_deref().and_then(crate::runtime::usage::parse_size);
        Self {
            limits: Limits {
                warn: size(&def.memory_warn),
                max: size(&def.max_memory),
            },
            on_max: def.on_memory_max,
            series: Series::default(),
            budget: BudgetMachine::default(),
            leak: LeakDetector::default(),
            restarts: 0,
            last_rss: None,
            alerted_cycles: None,
            degrading: false,
        }
    }

    /// The budget standing, `ok` while nothing is known.
    pub(crate) fn budget(&self) -> Budget {
        self.budget.state()
    }
}

/// Current phase of a mobile build/deploy pipeline.
/// Emitted by the log-parsing task inside run_session_action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MobileBuildPhase {
    /// Device detection running (pre-spawn).
    Detecting,
    /// cargo build --target <mobile-triple> in progress.
    CompilingRust,
    /// Gradle assembleDebug (Android).
    GradleBuild,
    /// xcodebuild (iOS).
    XcodeBuild,
    /// APK/IPA installing on device.
    Installing,
    /// App launching (adb am start / Xcode install).
    Launching,
    /// App running — log stream active.
    Running,
    /// Build or deploy failed.
    Failed(String),
}

impl MobileBuildPhase {
    /// Short label shown in the sidebar badge.
    pub fn label(&self) -> &str {
        match self {
            Self::Detecting => "detecting",
            Self::CompilingRust => "rustc",
            Self::GradleBuild => "gradle",
            Self::XcodeBuild => "xcode",
            Self::Installing => "install",
            Self::Launching => "launch",
            Self::Running => "running",
            Self::Failed(_) => "failed",
        }
    }

    /// True while a long build task is in progress — drives spinner.
    pub fn is_building(&self) -> bool {
        matches!(
            self,
            Self::CompilingRust | Self::GradleBuild | Self::XcodeBuild | Self::Installing
        )
    }
}

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    StateChanged {
        name: String,
        state: ServiceState,
    },
    AllStarted,
    AllStopped,
    LogLine {
        name: String,
        line: String,
        stream: LogStream,
    },
    CriticalAlert {
        service: String,
        message: String,
    },
    /// A service process was (re)spawned: its log view gets a separator when
    /// the service already printed something.
    ServiceRestarted {
        name: String,
    },
    StatusChanged,
    ActionCompleted {
        label: String,
        ok: bool,
        resume: Vec<String>,
    },
    /// One non-empty output line (ANSI stripped) from a running non-session action.
    ActionProgress {
        label: String,
        line: String,
    },
    /// A session tab was opened (session: true action started).
    SessionStarted {
        label: String,
    },
    /// A session tab ended (process exited or was killed).
    SessionEnded {
        label: String,
        ok: bool,
    },
    /// Fired when ensure_deps finishes (only if it actually ran an install).
    DepsInstalled,
    /// Fired when ensure_deps ran the install/pre_command and it failed.
    DepsFailed {
        reason: String,
    },
    /// Infra port waits done; TUI should now start app services.
    InfraReady {
        app_names: Vec<String>,
    },
    /// Docker/compose could not bring infra up, or an infra port never opened: these
    /// command-less services are Failed with `reason` (card note, service log, View logs).
    /// Applied by every event loop through `ServiceManager::record_failure`.
    InfraFailed {
        names: Vec<String>,
        reason: String,
    },
    /// Signal from OS (SIGTERM/SIGHUP) — triggers graceful quit.
    Quit,
    /// Mobile build pipeline phase changed — drives the sidebar badge.
    MobileSessionPhaseChanged {
        label: String,
        phase: MobileBuildPhase,
    },
    /// A mobile device disconnected during an active session.
    DeviceDisconnected {
        label: String,
        device: String,
    },
    /// Scheduler/restart narration for the event log and headless output
    /// (`api restarting in 2s (1/5)`, `web waiting for api (healthy)`, …).
    ServiceNote {
        name: String,
        message: String,
    },
    /// A service's `ready_log` regex matched a line.
    LogReady {
        name: String,
    },
    /// The SSH tunnel changed state (drives the sidebar row + status file).
    TunnelState {
        state: ServiceState,
    },
}

pub(crate) struct ManagedService {
    pub(crate) command: Option<String>,
    pub(crate) dir: Option<String>,
    pub(crate) env: HashMap<String, String>,
    pub(crate) lock_file: Option<String>,
    pub(crate) status: ServiceStatus,
    pub(crate) child: Option<Child>,
    pub(crate) critical: bool,
    pub(crate) open_key: Option<char>,
    pub(crate) last_log: Arc<Mutex<String>>,
    pub(crate) hide_logs: bool,
    /// Set to true when the service is stopped — health watcher checks this
    /// before emitting StateChanged to avoid flipping Suspended → Failed.
    pub(crate) stopped: Arc<AtomicBool>,
    /// stdout/stderr line pumps of the current spawn; aborted on stop/restart so
    /// a grandchild holding the pipes open cannot leak them.
    pub(crate) pumps: Vec<tokio::task::JoinHandle<()>>,
    /// The last child exited with status 0 (a detaching launcher such as
    /// `docker compose up -d`): with a port/health URL the probe stays
    /// authoritative. Any other child exit is Failed without probing.
    pub(crate) exited_ok: bool,
    /// Exit code of the last reaped child (`None` while running / never exited;
    /// a signal death has no code).
    pub(crate) exit_code: Option<i32>,
    /// Liveness/readiness bookkeeping: misses, since-when, probe errors, log context.
    pub(crate) probes: Probes,
    /// Memory/CPU of the process tree, refreshed every few seconds while alive.
    pub(crate) usage: Option<crate::runtime::usage::Usage>,
    /// Memory budget, time series and leak watch (see `runtime::memory`).
    pub(crate) mem: MemWatch,
    /// `autostart: false`: not queued at boot, started on demand.
    pub(crate) autostart: bool,
    /// Not part of this run's selection (`atlas dev SERVICE…` / `--profile`):
    /// shown `disabled` (`off`), never started at boot, startable by hand.
    /// Cleared when the user starts it.
    pub(crate) deselected: bool,
    /// Scheduling/restart policy and bookkeeping (see `policy.rs`).
    pub(crate) sched: Sched,
}

/// Per-service orchestration state: restart policy, dependency gate, readiness.
pub(crate) struct Sched {
    pub(crate) restart: RestartPolicy,
    pub(crate) max_restarts: Option<u32>,
    pub(crate) oneshot: bool,
    pub(crate) deps: Vec<(String, Condition)>,
    pub(crate) ready_log: Option<regex::Regex>,
    pub(crate) tracker: RestartTracker,
    /// Shared with this spawn's log readers; set when `ready_log` matched.
    pub(crate) log_ready: Arc<AtomicBool>,
    /// atlas has spawned a child for this service at least once this run.
    pub(crate) spawned: bool,
    /// Waiting on dependencies — the reason shown in the sidebar.
    pub(crate) waiting: Option<String>,
    /// Queued by `start_app_services`; cleared when the child is spawned or the
    /// service is failed by the scheduler.
    pub(crate) pending_start: bool,
    /// Child exited 0 and no restart follows (one-shot finished, or `restart: no`).
    pub(crate) completed_ok: bool,
    /// Restart budget spent.
    pub(crate) gave_up: bool,
    /// Why the service is Failed when atlas decided it (shown in sidebar/CI JSON).
    pub(crate) fail_reason: Option<String>,
    /// The foreign process that held this service's port when it was last started (or
    /// re-checked while the conflict stood): `atlas_status` `port_conflict`, the sidebar note.
    pub(crate) port_conflict: Option<crate::runtime::port_conflict::PortConflict>,
    /// `start_timeout` in seconds: how long Starting may last before a warning.
    pub(crate) start_timeout: u64,
    /// The start-timeout warning (and critical alert) already fired this spawn.
    pub(crate) start_warned: bool,
    /// When the current spawn entered Starting (waiting note + timeout clock).
    pub(crate) starting_since: Option<std::time::Instant>,
    /// When the child was spawned (min-uptime gate for services with no probe).
    pub(crate) spawned_at: Option<std::time::Instant>,
    /// Entered Running (uptime); cleared when it leaves Running.
    pub(crate) running_since: Option<std::time::Instant>,
    /// Services with no port/health/ready_log: Running only after `MIN_UPTIME`.
    pub(crate) no_probe: bool,
    /// Label of the action that suspended this service.
    pub(crate) paused_by: Option<String>,
}

/// A service with nothing to probe must stay alive this long before it counts
/// as Running, so a crash-looping process never flashes green.
pub(crate) const MIN_UPTIME: std::time::Duration = std::time::Duration::from_secs(2);

/// Per-service liveness/readiness settings and live bookkeeping (see `runtime::probe`).
pub(crate) struct Probes {
    /// One liveness probe's timeout (`health_timeout`).
    pub(crate) timeout: std::time::Duration,
    /// Misses in a row before `unhealthy` (`failure_threshold`).
    pub(crate) threshold: u32,
    pub(crate) ready: Option<crate::config::ReadyProbe>,
    pub(crate) on_unhealthy: crate::config::OnUnhealthy,
    pub(crate) live: crate::runtime::probe::Liveness,
    /// Shared with this spawn's log pumps: the last lines and the repeated-error detector.
    pub(crate) ctx: crate::runtime::probe::SharedContext,
    /// When the service turned `unhealthy` (monotonic + epoch seconds) and the log tail then.
    pub(crate) unhealthy: Option<Unhealthy>,
    /// Why the service is `degraded` (the `ready` probe's error, or the repeated line).
    pub(crate) degraded: Option<String>,
    /// Last `ready` probe failure; `None` when it passes or there is no probe.
    pub(crate) ready_error: Option<String>,
    /// Restarts `on_unhealthy: restart` made since the service last stayed up for
    /// [`super::policy::RESET_AFTER`]; survives respawns (`restart_service` wipes the tracker),
    /// so a service that hangs again after each restart is not restarted forever.
    pub(crate) hang_restarts: u32,
}

/// The capture taken at the moment a service turned `unhealthy`.
#[derive(Debug, Clone)]
pub(crate) struct Unhealthy {
    pub(crate) since: std::time::Instant,
    /// Unix seconds, for the status file.
    pub(crate) since_epoch: u64,
    pub(crate) misses: u32,
    /// The last [`crate::runtime::probe::CONTEXT_LINES`] output lines.
    pub(crate) lines: Vec<String>,
}

impl Probes {
    pub(crate) fn from_def(def: &crate::config::ServiceDef) -> Self {
        Self {
            timeout: def.health_timeout(),
            threshold: def.failure_threshold(),
            ready: def.ready.clone(),
            on_unhealthy: def.on_unhealthy,
            live: Default::default(),
            ctx: Default::default(),
            unhealthy: None,
            degraded: None,
            ready_error: None,
            hang_restarts: 0,
        }
    }

    /// New spawn: nothing carries over from the previous process.
    pub(crate) fn reset_for_spawn(&mut self) {
        self.live.reset();
        self.ctx = Default::default();
        self.unhealthy = None;
        self.degraded = None;
        self.ready_error = None;
    }
}

/// Health-watcher poll interval while a service is still coming up.
pub(crate) const WATCH_STARTUP_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);
/// Poll interval once a service has been seen healthy.
pub(crate) const WATCH_HEALTHY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
/// Per-probe timeout used by the watcher.
pub(crate) const WATCH_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// Shared SIGTERM→SIGKILL grace when stopping service trees.
pub(crate) const STOP_GRACE: std::time::Duration = std::time::Duration::from_millis(800);
/// How long an infra service's port may stay closed after `docker compose up -d` before it is Failed.
pub(crate) const INFRA_PORT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

impl Sched {
    /// Feed a state transition into the stability clock (restart counter reset).
    pub(crate) fn note_state(&mut self, state: &ServiceState) {
        let now = std::time::Instant::now();
        if *state == ServiceState::Running {
            self.tracker.mark_running(now);
            self.running_since.get_or_insert(now);
        } else {
            self.tracker.mark_not_running();
            self.running_since = None;
        }
        if *state == ServiceState::Starting {
            self.starting_since.get_or_insert(now);
        } else {
            self.starting_since = None;
        }
    }

    /// User stop/restart/quit: forget restart history and queued starts.
    pub(crate) fn reset_for_user_action(&mut self) {
        self.tracker = RestartTracker::default();
        self.pending_start = false;
        self.waiting = None;
        self.gave_up = false;
        self.completed_ok = false;
        self.fail_reason = None;
        self.start_warned = false;
        self.paused_by = None;
    }

    pub(crate) fn from_def(def: &crate::config::ServiceDef) -> Self {
        Self {
            restart: def.restart,
            max_restarts: def.max_restarts,
            oneshot: def.oneshot,
            deps: def.deps(),
            // Validated at load; an invalid regex here simply disables ready_log.
            ready_log: def
                .ready_log
                .as_deref()
                .and_then(|r| regex::Regex::new(r).ok()),
            tracker: RestartTracker::default(),
            log_ready: Arc::new(AtomicBool::new(false)),
            spawned: false,
            waiting: None,
            pending_start: false,
            completed_ok: false,
            gave_up: false,
            fail_reason: None,
            port_conflict: None,
            start_timeout: def.start_timeout_secs(),
            start_warned: false,
            starting_since: None,
            spawned_at: None,
            running_since: None,
            no_probe: def.command.is_some() && def.has_no_probe(),
            paused_by: None,
        }
    }
}

/// A running interactive session spawned by a `session: true` action.
pub(crate) struct ManagedSession {
    pub(crate) child: Child,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) resume: Vec<String>,
    /// stdout/stderr line pumps; aborted when the session ends.
    pub(crate) pumps: Vec<tokio::task::JoinHandle<()>>,
}

impl ManagedSession {
    pub(crate) async fn send_input(&mut self, bytes: &[u8]) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = stdin.write_all(bytes).await;
            let _ = stdin.flush().await;
        }
    }
}
