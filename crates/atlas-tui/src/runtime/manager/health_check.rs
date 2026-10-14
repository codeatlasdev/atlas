use std::time::Duration;

use super::ServiceManager;
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub async fn check_health(&mut self) {
        self.reap_sessions().await;
        let timeout = Duration::from_secs(2);
        let names = self.order.clone();

        // Restarts due, dependency gates opened, stable counters reset. Runs
        // before the probes so a service spawned now is probed on the next tick.
        self.schedule().await;

        // --- 1. Snapshot check params (immutable borrow ends before mutation) ---
        struct CheckParams {
            name: String,
            health_url: Option<String>,
            port: Option<u16>,
            pid: Option<u32>,
            child_alive: bool,
        }

        let mut params: Vec<CheckParams> = Vec::new();
        let mut dead: Vec<String> = Vec::new();
        let mut reaped_any = false;
        for name in &names {
            // Reap exited children first
            let Some(svc) = self.services.get_mut(name.as_str()) else {
                continue;
            };
            let mut reaped_now = false;
            if let Some(child) = svc.child.as_mut() {
                let exited = match child.try_wait() {
                    Ok(None) => None,
                    Ok(Some(status)) => Some((status.success(), status.code())),
                    Err(_) => Some((false, None)),
                };
                if let Some((ok, code)) = exited {
                    svc.child = None;
                    svc.status.pid = None;
                    svc.exited_ok = ok;
                    svc.exit_code = code;
                    reaped_any = true;
                    reaped_now = true;
                }
            }
            // Restart pending or budget spent: the exit was already decided when
            // the child was reaped; nothing to probe and nothing to re-decide.
            if svc.sched.tracker.pending_at.is_some() || svc.sched.gave_up {
                continue;
            }
            let svc = &*svc;
            let state = svc.status.state.clone();
            // Skip Stopped/Disabled always. Skip Starting only when the child is
            // still alive — a dead child in Starting state must still be caught.
            if state == ServiceState::Stopped
                || state == ServiceState::Disabled
                || state == ServiceState::Suspended
            {
                continue;
            }
            let child_alive = svc.child.is_some();
            // A service with a command we spawned must have a live child. If it
            // died, the port may still answer (an orphan or another tool holds
            // it) but that is not this service: mark Failed, never probe. The one
            // exception is a launcher that exited 0 (`docker compose up -d`) on a
            // service that has a port/health URL — there the probe is the truth.
            let detached_ok =
                svc.exited_ok && (svc.status.port.is_some() || svc.status.health_url.is_some());
            if svc.command.is_some() && !child_alive && !detached_ok {
                // Retire this spawn's port watcher too, or it keeps emitting
                // Running for whatever else answers on the port.
                svc.stopped
                    .store(true, std::sync::atomic::Ordering::Release);
                // Decide an exit once: when reaped now, or for a spawn we never
                // decided (state still live). Already Failed/completed stays put.
                if reaped_now || state != ServiceState::Failed {
                    dead.push(name.clone());
                }
                continue;
            }
            if state == ServiceState::Starting
                && child_alive
                && (svc.status.port.is_some() || svc.status.health_url.is_some())
            {
                continue;
            }
            // `ready_log` is the only readiness signal: a Starting service stays
            // Starting (no pid-alive promotion) until the regex matched once.
            if state == ServiceState::Starting
                && child_alive
                && svc.sched.ready_log.is_some()
                && !svc
                    .sched
                    .log_ready
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                continue;
            }
            // No port/health/ready_log: alive is the only evidence, so demand a
            // minimum uptime before turning green (a crash loop never does).
            if state == ServiceState::Starting
                && child_alive
                && svc.sched.no_probe
                && !svc
                    .sched
                    .spawned_at
                    .is_some_and(|t| t.elapsed() >= self.min_uptime)
            {
                continue;
            }
            let pid = svc.status.pid;
            if state == ServiceState::Failed
                && !child_alive
                && pid.is_none()
                && svc.status.port.is_none()
            {
                continue;
            }
            // `ready_log` replaces the probe: the log reader promotes Starting →
            // Running, and a live child is healthy. Never flip it on a port check.
            let log_driven = svc.sched.ready_log.is_some() && child_alive;
            params.push(CheckParams {
                name: name.clone(),
                health_url: if log_driven {
                    None
                } else {
                    svc.status.health_url.clone()
                },
                port: if log_driven { None } else { svc.status.port },
                pid,
                child_alive,
            });
        }
        if reaped_any {
            self.sync_pid_file();
        }
        // The one place a spawned child's exit becomes a decision: restart,
        // finished one-shot, give-up, or plain Failed.
        for name in dead {
            let exit_ok = self.services.get(&name).is_some_and(|s| s.exited_ok);
            let state = self
                .on_child_exit(&name, exit_ok)
                .unwrap_or(ServiceState::Failed);
            self.set_state_if_changed(&name, state);
        }

        // --- 2. Run all checks in parallel ---
        let mut checks = tokio::task::JoinSet::new();
        for p in params {
            checks.spawn(async move {
                let healthy =
                    match crate::runtime::health::probe(p.health_url.as_deref(), p.port, timeout)
                        .await
                    {
                        Some(healthy) => healthy,
                        None => match p.pid {
                            Some(pid) => crate::runtime::health::check_process_alive(pid).await,
                            None => p.child_alive,
                        },
                    };
                (p.name, healthy)
            });
        }

        // --- 3. Await all and apply results ---
        while let Some(joined) = checks.join_next().await {
            if let Ok((name, healthy)) = joined {
                let Some(svc) = self.services.get_mut(&name) else {
                    continue;
                };
                if healthy {
                    svc.probe_fails = 0;
                    self.set_state_if_changed(&name, ServiceState::Running);
                    continue;
                }
                svc.probe_fails += 1;
                // A live child of a Running service gets a grace period: dev
                // servers close their port while they hot-restart.
                let grace = svc.child.is_some()
                    && svc.status.state == ServiceState::Running
                    && svc.probe_fails < super::types::PROBE_FAILS_TO_FAIL;
                if !grace {
                    self.set_state_if_changed(&name, ServiceState::Failed);
                }
            }
        }
        self.sample_usage().await;
        // Heartbeat: keeps `updated_at` fresh so `atlas dev --status` can tell a
        // live run from a stale file left by a crash.
        self.write_status_file();
    }

    /// Every [`USAGE_EVERY`]: memory/CPU per service tree, and the `max_memory`
    /// guard (two samples over the ceiling → alert + restart).
    async fn sample_usage(&mut self) {
        if self.sampled_at.is_some_and(|t| t.elapsed() < USAGE_EVERY) {
            return;
        }
        self.sampled_at = Some(std::time::Instant::now());
        let roots: Vec<(String, u32)> = self
            .services
            .iter()
            .filter(|(_, s)| s.child.is_some())
            .filter_map(|(n, s)| Some((n.clone(), s.status.pid?)))
            .collect();
        let mut usage = crate::runtime::usage::sample(&roots).await;
        let mut over = Vec::new();
        for (name, svc) in self.services.iter_mut() {
            svc.usage = usage.remove(name);
            match (svc.usage, svc.max_memory) {
                (Some(u), Some(max)) if u.mem > max => {
                    svc.mem_over += 1;
                    if svc.mem_over >= 2 {
                        svc.mem_over = 0;
                        over.push((name.clone(), u.mem, max));
                    }
                }
                _ => svc.mem_over = 0,
            }
        }
        for (name, used, max) in over {
            let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
            let tries = self.services.get_mut(&name).map_or(0, |s| {
                s.mem_restarts += 1;
                s.mem_restarts
            });
            let give_up = tries > MEM_RESTARTS;
            let message = if give_up {
                format!(
                    "using {:.1} GB (max_memory {:.1} GB) after {MEM_RESTARTS} restarts, stopped",
                    gb(used),
                    gb(max)
                )
            } else {
                format!(
                    "using {:.1} GB (max_memory {:.1} GB), restarting ({tries}/{MEM_RESTARTS})",
                    gb(used),
                    gb(max)
                )
            };
            let _ = self.event_tx.send(super::ManagerEvent::CriticalAlert {
                service: name.clone(),
                message: message.clone(),
            });
            if give_up {
                // Stop for good: an endless kill/respawn cycle is the loop this guards against.
                self.stop_service(&name).await;
                self.set_state_if_changed(&name, ServiceState::Failed);
            } else {
                self.restart_service(&name).await;
            }
            self.note(&name, message);
        }
    }

    /// Latest memory/CPU of a running service.
    pub fn usage(&self, name: &str) -> Option<crate::runtime::usage::Usage> {
        self.services.get(name)?.usage
    }
}

const USAGE_EVERY: Duration = Duration::from_secs(5);
/// `max_memory` restarts per run before the service is stopped and marked Failed.
const MEM_RESTARTS: u32 = 3;

impl ServiceManager {
    /// Write .logs/.tui-status.json with current service states.
    pub(crate) fn write_status_file(&self) {
        // An attached client mirrors the runner; the status file is the runner's.
        if self.mirror.is_some() {
            return;
        }
        let own_pid = std::process::id();

        let services_map: serde_json::Map<String, serde_json::Value> = self
            .order
            .iter()
            .filter_map(|name| {
                let service = self.services.get(name)?;
                let obj = serde_json::json!({
                    "state": service.status.state,
                    "port": service.status.port,
                    "health": service.status.health_url,
                    "exit_code": self.exit_code(name),
                    "restarts": service.sched.tracker.attempts,
                    "uptime_s": self.uptime_secs(name),
                    "started_at": service.sched.spawned_at.and_then(|t| {
                        let ago = t.elapsed().as_secs();
                        crate::runtime::clock::now_secs().checked_sub(ago)
                    }),
                    "note": self.service_note(name),
                    "autostart": service.autostart,
                    "deselected": service.deselected,
                    "mem_mb": service.usage.map(|u| u.mem >> 20),
                    "cpu": service.usage.map(|u| u.cpu.round() as u32),
                });
                Some((name.clone(), obj))
            })
            .collect();

        let updated_at = crate::runtime::clock::now_secs();
        let payload = serde_json::json!({
            "pid": own_pid,
            "mode": self.mode,
            "clients": self.clients,
            "session": self.session,
            "started_at": self.started_at,
            "updated_at": updated_at,
            "log_dir": ".logs",
            "services": services_map,
            "tunnel": self.tunnel_json(),
        });

        // Written inline (small file): every caller serializes on the manager's
        // `&self`, so snapshots can never land out of order, and the tmp+rename
        // keeps `--status` readers from ever seeing a truncated file.
        if let Ok(text) = serde_json::to_string(&payload) {
            write_status_atomic(&self.log_dir, &text);
        }
    }

    /// Returns (name, last_log_snippet) for critical services that are Failed.
    pub fn critical_services_failed(&self) -> Vec<(String, String)> {
        self.order
            .iter()
            .filter_map(|name| {
                let service = self.services.get(name)?;
                if service.critical && service.status.state == ServiceState::Failed {
                    let snippet = service
                        .last_log
                        .lock()
                        .ok()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    Some((name.clone(), snippet))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Write `text` to `<dir>/.tui-status.json` atomically: write a sibling tmp
/// file, then rename over the target, so readers see the old or new snapshot
/// in full, never a truncated one. Best-effort.
fn write_status_atomic(dir: &std::path::Path, text: &str) {
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(".tui-status.json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(".tui-status.json"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_write_is_atomic_under_concurrent_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let big = |n: usize| format!("{{\"n\":{n},\"pad\":\"{}\"}}", "x".repeat(64 * 1024));
        write_status_atomic(&path, &big(0));

        let reader_path = path.join(".tui-status.json");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_r = std::sync::Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let mut reads = 0u32;
            while !stop_r.load(std::sync::atomic::Ordering::Relaxed) {
                let text = std::fs::read_to_string(&reader_path).unwrap();
                serde_json::from_str::<serde_json::Value>(&text)
                    .expect("reader must never see a partial file");
                reads += 1;
            }
            reads
        });
        for n in 1..300 {
            write_status_atomic(&path, &big(n));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }
}
