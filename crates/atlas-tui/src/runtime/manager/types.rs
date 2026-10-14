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
    /// Consecutive failed health probes while the child is alive. A running
    /// service only turns Failed after [`PROBE_FAILS_TO_FAIL`] in a row, so a
    /// dev server's own hot restart (port closed for a second or two) is not a
    /// failure.
    pub(crate) probe_fails: u32,
    /// Memory/CPU of the process tree, refreshed every few seconds while alive.
    pub(crate) usage: Option<crate::runtime::usage::Usage>,
    /// `max_memory:` in bytes: above it (two samples in a row) the service restarts.
    pub(crate) max_memory: Option<u64>,
    pub(crate) mem_over: u32,
    /// Restarts caused by `max_memory` this run; past the cap the service stops.
    pub(crate) mem_restarts: u32,
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

/// Failed probes in a row (with a live child) before Running turns Failed.
pub(crate) const PROBE_FAILS_TO_FAIL: u32 = 3;

/// Health-watcher poll interval while a service is still coming up.
pub(crate) const WATCH_STARTUP_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);
/// Poll interval once a service has been seen healthy.
pub(crate) const WATCH_HEALTHY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
/// Retry spacing after a miss on a previously healthy service (3 misses ≈ 6s).
pub(crate) const WATCH_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
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
