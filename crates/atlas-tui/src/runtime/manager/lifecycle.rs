use std::time::Duration;

use super::ServiceManager;
use super::types::{ManagerEvent, STOP_GRACE};
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub async fn start_all(&mut self) {
        let _ = std::fs::create_dir_all(&self.log_dir);

        // Tunnel (enabled only): supervised on its own task, never blocks startup.
        // Idempotent, so `restart_all` does not tear it down and respawn it.
        self.start_tunnel_if_enabled();

        // Phase 1 (sync, fast): kill orphans from a crashed previous run.
        // Must run before docker/app startup so stale pids never shadow new ones.
        self.kill_orphans().await;

        // Collect infra services (command-less, port-based) — mark Starting
        let infra_checks: Vec<(String, u16)> = self
            .order
            .iter()
            .filter_map(|name| {
                let svc = self.services.get(name)?;
                if svc.command.is_none() && svc.status.port.is_some() && !svc.deselected {
                    Some((name.clone(), svc.status.port?))
                } else {
                    None
                }
            })
            .collect();

        for (name, _) in &infra_checks {
            self.set_state_if_changed(name, ServiceState::Starting);
        }

        // App services (have a command) start after infra is ready
        let app_names: Vec<String> = self
            .order
            .iter()
            .filter(|n| {
                self.services
                    .get(*n)
                    .is_some_and(|s| s.command.is_some() && s.autostart && !s.deselected)
            })
            .cloned()
            .collect();

        let tx = self.event_tx.clone();
        // Narrowed run: compose is brought up only when a selected service needs
        // infra. A project whose compose services are not modelled as atlas
        // services at all (no command-less service exists) keeps the old
        // behavior, since nothing says who needs it.
        let narrowed = self.services.values().any(|s| s.deselected);
        let infra_modelled = self.services.values().any(|s| s.command.is_none());
        let mut docker = self
            .docker
            .clone()
            .filter(|_| !narrowed || !infra_modelled || !infra_checks.is_empty());
        if infra_checks.is_empty() && docker.is_none() {
            // No infra to wait for — signal immediately
            let _ = tx.send(ManagerEvent::InfraReady { app_names });
            return;
        }

        // Phase 2 (background): docker compose up, wait for all infra ports, then
        // signal InfraReady. start_all returns immediately; the event loop keeps
        // drawing frames and handling input while docker pulls/starts.
        tokio::spawn(async move {
            if let Some(docker) = &mut docker
                && let Err(reason) = docker.start(&[]).await
            {
                let _ = tx.send(ManagerEvent::CriticalAlert {
                    service: "docker".to_string(),
                    message: format!(
                        "Docker daemon not available or compose failed (is OrbStack / Docker \
                         Desktop running?): {reason}"
                    ),
                });
                for (name, _) in &infra_checks {
                    let _ = tx.send(ManagerEvent::StateChanged {
                        name: name.clone(),
                        state: ServiceState::Failed,
                    });
                }
                // No infra to wait for — start app services like the success path.
                let _ = tx.send(ManagerEvent::InfraReady { app_names });
                return;
            }

            let handles: Vec<_> = infra_checks
                .iter()
                .map(|(name, port)| {
                    let name = name.clone();
                    let port = *port;
                    tokio::spawn(async move {
                        let ok = crate::runtime::health::wait_for_port(
                            "127.0.0.1",
                            port,
                            Duration::from_secs(30),
                        )
                        .await;
                        (name, ok)
                    })
                })
                .collect();

            for handle in handles {
                if let Ok((name, ok)) = handle.await {
                    let state = if ok {
                        ServiceState::Running
                    } else {
                        ServiceState::Failed
                    };
                    let _ = tx.send(ManagerEvent::StateChanged { name, state });
                }
            }
            let _ = tx.send(ManagerEvent::InfraReady { app_names });
        });
    }

    /// Phase 3: start app services once infra is ready (`ManagerEvent::InfraReady`).
    /// Shared by the TUI effect loop and the headless/CI runners.
    pub async fn start_app_services(&mut self, names: &[String]) {
        // Queue everything; `schedule` spawns each service as soon as its
        // dependency conditions hold (immediately for services without any).
        for name in names {
            self.queue_start(name);
        }
        self.schedule().await;
        self.write_status_file();
        let _ = self.event_tx.send(ManagerEvent::AllStarted);
    }

    /// Quit: stop every service, then apply the tunnel exit policy
    /// (`tunnel_kill_on_exit`: SIGTERM our ssh, or leave it + `.tunnel.pid`).
    pub async fn stop_all(&mut self) {
        self.stop_everything().await;
        self.shutdown_tunnel().await;
    }

    /// Stop sessions + services; the tunnel keeps running (used by restart_all).
    async fn stop_everything(&mut self) {
        // Interactive sessions (tauri/gradle/xcode) are not in `.pids`; stop them here.
        let labels: Vec<String> = self.sessions.keys().cloned().collect();
        for label in labels {
            self.stop_session(&label).await;
        }
        let names: Vec<String> = self.order.iter().rev().cloned().collect();
        self.stop_services(&names).await;
        let _ = self.event_tx.send(ManagerEvent::AllStopped);
    }

    /// Stop `names`: tell their health watchers to stand down, cancel scheduled
    /// restarts / queued starts (a stop must never respawn), then kill every
    /// process tree together and mark each service Stopped.
    pub(crate) async fn stop_services(&mut self, names: &[String]) {
        for name in names {
            if let Some(svc) = self.services.get_mut(name) {
                svc.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
                svc.sched.reset_for_user_action();
            }
        }

        // One process-table snapshot while every tree is still connected: turbo-style
        // tasks live in new process groups, so group signals alone would miss them.
        let table = crate::runtime::process::process_table_async().await;
        let mut tree = crate::runtime::process::KillTree::default();
        for name in names {
            if let Some(pid) = self
                .services
                .get(name)
                .and_then(|s| s.child.as_ref())
                .and_then(|c| c.id())
            {
                tree.merge(crate::runtime::process::KillTree::collect(
                    pid as i32,
                    Some(pid as i32),
                    &table,
                ));
            }
        }

        // SIGTERM everything simultaneously, shared grace, SIGKILL survivors.
        if !tree.is_empty() {
            let services = &mut self.services;
            crate::runtime::process::kill_tree_gracefully(&tree, STOP_GRACE, || {
                for name in names {
                    if let Some(c) = services.get_mut(name).and_then(|s| s.child.as_mut()) {
                        let _ = c.try_wait();
                    }
                }
            })
            .await;
        }

        // Clear handles, mark Stopped. Pumps end on pipe EOF, but a grandchild
        // holding the pipes would keep them (and their disk-log handles) alive
        // forever, so abort them.
        for name in names {
            if let Some(svc) = self.services.get_mut(name) {
                svc.child = None;
                svc.status.pid = None;
                for pump in svc.pumps.drain(..) {
                    pump.abort();
                }
            }
            self.set_state_if_changed(name, ServiceState::Stopped);
        }
        // Note: docker compose containers are intentionally left running.
        // `docker compose down` can take the container runtime down with it (seen with OrbStack).
        // The infra containers have restart:unless-stopped and survive fine.
        self.sync_pid_file();
    }

    pub async fn restart_all(&mut self) {
        self.stop_everything().await;
        self.start_all().await;
    }

    pub async fn restart_service(&mut self, name: &str) {
        if !self.services.contains_key(name) {
            return;
        }
        self.stop_service(name).await;
        self.start_service(name).await;
    }
}
