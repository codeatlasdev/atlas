use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::process::Command;
use tokio::sync::mpsc;

use crate::config::{ActionConfig, ProjectConfig, Selection};
use crate::runtime::service::{ServiceState, ServiceStatus};

mod actions;
mod devices;
mod disk_log;
mod facts;
mod graph;
mod health_check;
mod lifecycle;
mod mirror;
mod mobile;
pub mod policy;
mod scheduler;
#[cfg(test)]
mod scheduler_tests;
mod sessions;
mod spawner;
mod tunnel_ctl;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use disk_log::{log_file_stem, open_stream_log, rotate_stream_log};

pub(crate) use devices::{parse_devicectl_json, parse_simctl_json};
pub use mirror::MirrorFacts;
use types::{ManagedService, ManagedSession};
pub use types::{ManagerEvent, MobileBuildPhase};

pub struct ServiceManager {
    pub(crate) root_dir: PathBuf,
    pub(crate) log_dir: PathBuf,
    /// Unique id for this atlas run (pid + start epoch), written to the status file.
    pub(crate) session: String,
    /// Unix seconds at manager creation; immutable for the whole run.
    pub(crate) started_at: u64,
    /// How long a service with no port, health URL or `ready_log` must stay alive before it
    /// counts as Running ([`types::MIN_UPTIME`]). A field so tests can pick a window that no
    /// machine load can blur.
    pub(crate) min_uptime: std::time::Duration,
    pub(crate) services: HashMap<String, ManagedService>,
    pub(crate) order: Vec<String>,
    pub(crate) event_tx: mpsc::UnboundedSender<ManagerEvent>,
    pub(crate) actions: Vec<ActionConfig>,
    /// `tunnel:` block from atlas.yaml (None = no tunnel configured).
    pub(crate) tunnel_cfg: Option<crate::config::TunnelConfig>,
    /// Running tunnel supervisor; created at boot when `tunnel.enabled`, or on
    /// demand by an `open_tunnel: true` action.
    pub(crate) tunnel: Option<crate::runtime::tunnel::TunnelHandle>,
    /// Running `open_tunnel` actions holding the tunnel; the last one to finish
    /// closes a tunnel it opened on demand.
    pub(crate) tunnel_leases: Arc<Mutex<tunnel_ctl::TunnelLeases>>,
    /// Exit policy: SIGTERM our ssh on stop_all (true) or leave it for reuse.
    pub(crate) tunnel_kill_on_exit: bool,
    pub(crate) docker: Option<crate::runtime::docker::DockerManager>,
    /// Active interactive sessions (session: true actions).
    pub(crate) sessions: HashMap<String, ManagedSession>,
    /// Mobile config from atlas.local.yaml (team_id, bundle_id, etc.)
    pub(crate) config_mobile: crate::config::MobileConfig,
    /// Variables loaded from `env_files` (later files win); applied to every spawn.
    pub(crate) file_env: HashMap<String, String>,
    /// Labels of non-session actions currently running; blocks concurrent re-runs.
    pub(crate) actions_in_flight: Arc<Mutex<HashSet<String>>>,
    /// Set on an attached client's manager: follows a remote runner, supervises nothing.
    pub(crate) mirror: Option<mirror::MirrorState>,
    /// `mode` written to the status file: `tui` (owner TUI) or `runner` (headless/--detach).
    pub(crate) mode: &'static str,
    /// Attached clients, written to the status file (runner only).
    pub(crate) clients: usize,
    /// Last resource sample (`usage.rs`), taken every few seconds.
    pub(crate) sampled_at: Option<std::time::Instant>,
}

impl ServiceManager {
    pub fn new(
        root_dir: PathBuf,
        config: &ProjectConfig,
        event_tx: mpsc::UnboundedSender<ManagerEvent>,
    ) -> Self {
        Self::with_selection(root_dir, config, event_tx, &Selection::all())
    }

    /// Who owns this run, for `atlas dev --status` (`tui` or `runner`).
    pub fn set_mode(&mut self, mode: &'static str) {
        self.mode = mode;
    }

    /// Attached client count for `atlas dev --status`; refreshes the status file.
    pub fn set_clients(&mut self, clients: usize) {
        if self.clients != clients {
            self.clients = clients;
            self.write_status_file();
        }
    }

    pub fn services(&self) -> Vec<&ServiceStatus> {
        self.iter_svcs().map(|(_, s)| &s.status).collect()
    }

    /// True if any service is mid-transition (Starting) — drives spinner
    /// animation, so the TUI keeps redrawing on tick while this is true.
    pub fn any_service_transitioning(&self) -> bool {
        self.iter_svcs()
            .any(|(_, s)| s.status.state == ServiceState::Starting)
            || self.scheduling_pending()
            || self.tunnel_transitioning()
    }

    pub fn service_names(&self) -> Vec<&str> {
        self.iter_svcs().map(|(n, _)| n).collect()
    }

    /// Names of services that should have a dedicated log tab (hide_logs == false).
    pub fn app_service_names(&self) -> Vec<&str> {
        self.iter_svcs()
            .filter(|(_, s)| !s.hide_logs)
            .map(|(n, _)| n)
            .collect()
    }

    /// Names of services with hide_logs=true (excluded from the All tab).
    pub fn hidden_service_names(&self) -> std::collections::HashSet<&str> {
        self.iter_svcs()
            .filter(|(_, s)| s.hide_logs)
            .map(|(n, _)| n)
            .collect()
    }

    /// Ordered iterator over (name, service) pairs for all known services.
    fn iter_svcs(&self) -> impl Iterator<Item = (&str, &types::ManagedService)> {
        self.order
            .iter()
            .filter_map(|n| self.services.get(n.as_str()).map(|s| (n.as_str(), s)))
    }

    pub fn update_state(&mut self, name: &str, state: ServiceState) {
        if let Some(service) = self.services.get_mut(name) {
            // A pending restart/gave-up service must not be revived or flipped by
            // a late event from a retired watcher.
            if service.sched.tracker.pending_at.is_some() || service.sched.gave_up {
                return;
            }
            // Watchers send Running without looking at the child (the no-probe one
            // after MIN_UPTIME). Refuse it when the child already exited (not yet
            // reaped by check_health, which decides that exit), and, for a service with
            // no probe, before MIN_UPTIME of the CURRENT spawn: a watcher of the previous
            // spawn can fire right after a restart.
            if state == ServiceState::Running {
                let exited = service
                    .child
                    .as_mut()
                    .is_some_and(|c| !matches!(c.try_wait(), Ok(None)));
                let too_young = service.sched.no_probe
                    && !service
                        .sched
                        .spawned_at
                        .is_some_and(|t| t.elapsed() >= self.min_uptime);
                if exited || too_young {
                    return;
                }
            }
            service.sched.note_state(&state);
            service.status.state = state;
        }
    }

    pub(crate) fn set_state_if_changed(&mut self, name: &str, state: ServiceState) {
        let changed = match self.services.get_mut(name) {
            Some(service) if service.status.state != state => {
                service.sched.note_state(&state);
                service.status.state = state.clone();
                true
            }
            _ => false,
        };

        if changed {
            let _ = self.event_tx.send(ManagerEvent::StateChanged {
                name: name.to_string(),
                state,
            });
            self.write_status_file();
            let _ = self.event_tx.send(ManagerEvent::StatusChanged);
        }
    }

    pub fn get_state(&self, name: &str) -> Option<ServiceState> {
        self.services
            .get(name)
            .map(|service| service.status.state.clone())
    }

    /// Returns services that have an `open_key` configured, in order.
    /// Each entry is (key, service_name, port).
    pub fn open_services(&self) -> Vec<(char, String, Option<u16>)> {
        self.order
            .iter()
            .filter_map(|name| {
                let svc = self.services.get(name)?;
                let key = svc.open_key?;
                Some((key, name.clone(), svc.status.port))
            })
            .collect()
    }

    /// Whether a `tunnel:` block is configured (enabled at boot or on demand).
    pub fn has_tunnel(&self) -> bool {
        self.tunnel_cfg.is_some()
    }

    /// Whether the tunnel is started at boot (`tunnel.enabled`, default true).
    pub fn tunnel_starts_at_boot(&self) -> bool {
        self.tunnel_cfg.as_ref().is_some_and(|t| t.is_enabled())
    }

    /// `<root>/.logs` — where service, action and UI-state files live.
    pub fn log_dir(&self) -> &std::path::Path {
        &self.log_dir
    }

    /// Append a user mark / note line to a service's disk log (best-effort, no
    /// rotation). `action:<label>` maps to `action-<slug>.log`; names that could
    /// escape `.logs/` are ignored.
    pub fn append_log_mark(&self, service: &str, line: &str) {
        let plain = !service.starts_with(disk_log::ACTION_PREFIX);
        if service.is_empty() || (plain && service.contains(['/', '\\'])) {
            return;
        }
        disk_log::append_mark(&self.log_dir, service, line);
    }

    /// Open a URL in the system default browser. Fire-and-forget.
    pub fn open_url(url: &str) {
        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(not(target_os = "macos"))]
        let opener = "xdg-open";
        if let Ok(mut child) = Command::new(opener).arg(url).spawn() {
            // Reap the launcher so it never lingers as a zombie.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }
    }
}
