use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use super::types::{self, ManagedService};
use super::{ManagerEvent, ServiceManager};
use crate::config::{ProjectConfig, Selection};
use crate::runtime::service::{ServiceState, ServiceStatus};

impl ServiceManager {
    /// Like `new`, but only services in `selection` start at boot. The rest
    /// are tracked as `Disabled` (`off`): visible and startable by hand.
    pub fn with_selection(
        root_dir: PathBuf,
        config: &ProjectConfig,
        event_tx: mpsc::UnboundedSender<ManagerEvent>,
        selection: &Selection,
    ) -> Self {
        let log_dir = crate::runtime::log_dir(&root_dir);
        let started_at = crate::runtime::clock::now_secs();
        let session = format!("{}-{}", std::process::id(), started_at);
        let order = resolve_order(config);

        let mut services = HashMap::new();
        for name in &order {
            if let Some(def) = config.services.get(name) {
                if !def.is_enabled() {
                    continue;
                }
                services.insert(
                    name.clone(),
                    managed_service(
                        name,
                        def,
                        selection.contains(name),
                        selection.requested(name),
                    ),
                );
            }
        }
        // A service some dependent waits on with `completed_successfully` is a
        // one-shot by definition, even without an explicit `oneshot: true`.
        let awaited_to_completion: HashSet<String> = config
            .services
            .values()
            .filter(|d| d.is_enabled())
            .flat_map(|d| d.deps())
            .filter(|(_, c)| *c == crate::config::Condition::CompletedSuccessfully)
            .map(|(n, _)| n)
            .collect();
        for (name, svc) in services.iter_mut() {
            if awaited_to_completion.contains(name) {
                svc.sched.oneshot = true;
            }
        }
        Self {
            root_dir: root_dir.clone(),
            log_dir,
            session,
            started_at,
            min_uptime: super::types::MIN_UPTIME,
            services,
            order,
            event_tx,
            actions: config.actions.clone(),
            tunnel_cfg: config.tunnel.clone(),
            tunnel: None,
            tunnel_leases: Arc::default(),
            tunnel_kill_on_exit: true,
            docker: config.infra.as_ref().map(|infra| {
                crate::runtime::docker::DockerManager::new(&infra.compose_file, &root_dir)
            }),
            sessions: HashMap::new(),
            config_mobile: config.mobile.clone(),
            file_env: crate::runtime::envfile::load_env_files(&root_dir, &config.env_files),
            actions_in_flight: Arc::default(),
            mirror: None,
            mode: "tui",
            clients: 0,
            adhoc: HashSet::new(),
            sampled_at: None,
        }
    }

    /// Register `def` as a new service of this live run and queue it to start (ad-hoc
    /// services: `atlas -- <cmd>`). `false` when the name is taken.
    pub async fn add_service(&mut self, name: &str, def: &crate::config::ServiceDef) -> bool {
        if self.services.contains_key(name) {
            return false;
        }
        self.services
            .insert(name.to_string(), managed_service(name, def, true, true));
        self.order.push(name.to_string());
        self.adhoc.insert(name.to_string());
        self.queue_start(name);
        self.schedule().await;
        self.write_status_file();
        let _ = self.event_tx.send(ManagerEvent::StatusChanged);
        true
    }

    /// Mark boot-time services as ad-hoc (the runner started with `--adhoc`).
    pub fn mark_adhoc(&mut self, names: impl IntoIterator<Item = String>) {
        self.adhoc.extend(names);
    }

    /// The command of an ad-hoc service (`atlas -- <cmd>`), `None` for configured ones.
    pub fn adhoc_command(&self, name: &str) -> Option<String> {
        if !self.adhoc.contains(name) {
            return None;
        }
        self.services.get(name)?.command.clone()
    }
}

/// The runtime record of one configured service. `selected`: part of this run's selection
/// (else it starts `Disabled`); `requested`: named explicitly, so it starts even with
/// `autostart: false`.
fn managed_service(
    name: &str,
    def: &crate::config::ServiceDef,
    selected: bool,
    requested: bool,
) -> ManagedService {
    ManagedService {
        // Services without a command are externally managed (docker-compose, etc.).
        // We track their port for health display but never spawn a child.
        command: def.command.clone(),
        dir: def.dir.clone(),
        env: def.env.clone(),
        lock_file: def.lock_file.clone(),
        status: ServiceStatus {
            name: name.to_string(),
            state: if selected {
                ServiceState::Stopped
            } else {
                ServiceState::Disabled
            },
            port: def.port,
            health_url: def.health.clone().filter(|h| !h.is_empty()),
            pid: None,
        },
        child: None,
        critical: def.critical.unwrap_or(false),
        open_key: def.open_key,
        last_log: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
        hide_logs: def.hide_logs,
        stopped: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        pumps: Vec::new(),
        exited_ok: false,
        probe_fails: 0,
        usage: None,
        max_memory: def
            .max_memory
            .as_deref()
            .and_then(crate::runtime::usage::parse_size),
        mem_over: 0,
        mem_restarts: 0,
        exit_code: None,
        autostart: def.is_autostart() || requested,
        deselected: !selected,
        sched: types::Sched::from_def(def),
    }
}

/// Topological sort of services based on depends_on.
pub(super) fn resolve_order(config: &ProjectConfig) -> Vec<String> {
    let mut order = Vec::new();
    let mut visited = std::collections::HashSet::new();

    fn visit(
        name: &str,
        config: &ProjectConfig,
        order: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        if let Some(def) = config.services.get(name) {
            for (dep, _) in def.deps() {
                visit(&dep, config, order, visited);
            }
        }
        order.push(name.to_string());
    }

    // HashMap iteration is random per run; sort the roots so tab order, digit
    // jump keys and the sidebar are identical on every start. Dependencies are
    // still emitted before their dependents.
    let mut roots: Vec<&String> = config.services.keys().collect();
    roots.sort();
    for name in roots {
        visit(name, config, &mut order, &mut visited);
    }

    order
}
