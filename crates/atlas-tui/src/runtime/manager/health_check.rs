use std::time::Duration;

use super::{ServiceManager, UnhealthyReport};
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub async fn check_health(&mut self) {
        self.reap_sessions().await;
        if self
            .tracked_at
            .is_none_or(|t| t.elapsed() >= super::hygiene::TRACK_EVERY)
        {
            self.tracked_at = Some(std::time::Instant::now());
            self.track_members();
        }
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
            timeout: Duration,
            ready: Option<crate::config::ReadyProbe>,
            dir: std::path::PathBuf,
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
                timeout: svc.probes.timeout,
                ready: svc.probes.ready.clone(),
                dir: svc
                    .dir
                    .as_ref()
                    .map_or_else(|| self.root_dir.clone(), |d| self.root_dir.join(d)),
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
        // Liveness first; the `ready` probe only runs while liveness passes (a hung
        // service's endpoints would just time out too, and `unhealthy` already says so).
        let mut checks = tokio::task::JoinSet::new();
        for p in params {
            checks.spawn(async move {
                let healthy =
                    match crate::runtime::health::probe(p.health_url.as_deref(), p.port, p.timeout)
                        .await
                    {
                        Some(healthy) => healthy,
                        None => match p.pid {
                            Some(pid) => crate::runtime::health::check_process_alive(pid).await,
                            None => p.child_alive,
                        },
                    };
                let ready = match (&p.ready, healthy) {
                    (Some(probe), true) => {
                        Some(crate::runtime::probe::check_ready(probe, p.port, &p.dir).await)
                    }
                    _ => None,
                };
                (p.name, healthy, ready)
            });
        }

        // --- 3. Await all and apply results ---
        while let Some(joined) = checks.join_next().await {
            if let Ok((name, healthy, ready)) = joined {
                self.apply_probe(&name, healthy, ready).await;
            }
        }
        self.refresh_port_conflicts().await;
        self.sample_usage().await;
        // Heartbeat: keeps `updated_at` fresh so `atlas dev --status` can tell a
        // live run from a stale file left by a crash.
        self.write_status_file();
    }

    /// Apply one tick's liveness (`healthy`) and readiness (`ready`, only run while liveness
    /// passed) result for `name`: the single place `running`, `degraded`, `unhealthy` and
    /// `failed` are decided for a probed service.
    async fn apply_probe(&mut self, name: &str, healthy: bool, ready: Option<Result<(), String>>) {
        use crate::runtime::probe::{Inputs, judge};
        let now = std::time::Instant::now();
        let Some(svc) = self.services.get_mut(name) else {
            return;
        };
        let alive = svc.child.is_some();
        // `unhealthy` is for a live process that stopped answering after it had answered.
        // A service in any other state (still starting, already failed, a detached launcher with
        // no child) keeps the old rule: a failed probe is `failed`.
        let hang_tracked = alive
            && matches!(
                svc.status.state,
                ServiceState::Running | ServiceState::Degraded | ServiceState::Unhealthy
            );
        if !healthy && !hang_tracked {
            svc.probes.live.observe(false, svc.probes.threshold);
            self.set_state_if_changed(name, ServiceState::Failed);
            return;
        }
        let live = svc.probes.live.observe(healthy, svc.probes.threshold);
        svc.probes.ready_error = match &ready {
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        };
        let repeated = svc
            .probes
            .ctx
            .lock()
            .ok()
            .and_then(|c| c.repeats.repeated(now).map(str::to_string));
        let memory = svc.mem.warn_reason().or_else(|| svc.mem.over_reason());
        let verdict = judge(Inputs {
            live,
            ready_error: svc.probes.ready_error.as_deref(),
            repeated: repeated.as_deref(),
            memory: memory.as_deref(),
        });
        let Some(verdict) = verdict else {
            // Grace: a dev server's hot restart closes its port for a moment.
            return;
        };
        match verdict.state {
            ServiceState::Unhealthy => self.enter_unhealthy(name).await,
            state => {
                if let Some(svc) = self.services.get_mut(name) {
                    svc.probes.unhealthy = None;
                    // Only the memory finding left the service `degraded`: it clears itself.
                    svc.mem.degrading =
                        memory.is_some() && verdict.reason.as_deref() == memory.as_deref();
                    svc.probes.degraded = verdict.reason;
                    // Up for a full RESET_AFTER: the hang-restart budget starts over.
                    if svc.probes.hang_restarts > 0
                        && svc
                            .sched
                            .running_since
                            .is_some_and(|t| t.elapsed() >= super::policy::RESET_AFTER)
                    {
                        svc.probes.hang_restarts = 0;
                    }
                }
                self.set_state_if_changed(name, state);
            }
        }
    }

    /// The service just missed `failure_threshold` probes in a row. The first time: capture the
    /// log tail, alert, and restart when `on_unhealthy: restart`. Later ticks keep the original
    /// `since`.
    async fn enter_unhealthy(&mut self, name: &str) {
        let Some(svc) = self.services.get_mut(name) else {
            return;
        };
        let first = svc.probes.unhealthy.is_none();
        if first {
            let lines = svc.probes.ctx.lock().map(|c| c.tail()).unwrap_or_default();
            svc.probes.degraded = None;
            svc.probes.unhealthy = Some(super::types::Unhealthy {
                since: std::time::Instant::now(),
                since_epoch: crate::runtime::clock::now_secs(),
                misses: svc.probes.live.misses,
                lines,
            });
        }
        let restart = first && svc.probes.on_unhealthy == crate::config::OnUnhealthy::Restart;
        let (misses, timeout) = (svc.probes.live.misses, svc.probes.timeout);
        self.set_state_if_changed(name, ServiceState::Unhealthy);
        if !first {
            return;
        }
        let message = format!(
            "unhealthy: no answer from the liveness probe ({misses} misses × {}s timeout){}",
            timeout.as_secs(),
            if restart { ", restarting" } else { "" }
        );
        let _ = self.event_tx.send(super::ManagerEvent::CriticalAlert {
            service: name.to_string(),
            message: message.clone(),
        });
        self.note(name, message);
        if restart {
            if self.take_hang_restart(name) {
                self.restart_service(name).await;
            } else {
                self.note(
                    name,
                    "unhealthy again after its restarts: left running".into(),
                );
            }
        }
    }

    /// Spend one `on_unhealthy: restart` from the service's budget (`max_restarts`, default 5,
    /// `0` = unlimited). `false` when it is spent.
    fn take_hang_restart(&mut self, name: &str) -> bool {
        let Some(svc) = self.services.get_mut(name) else {
            return false;
        };
        let max = svc
            .sched
            .max_restarts
            .unwrap_or(super::policy::DEFAULT_MAX_RESTARTS);
        if max != 0 && svc.probes.hang_restarts >= max {
            return false;
        }
        svc.probes.hang_restarts += 1;
        true
    }

    /// What `atlas_status` and the status file say about a hung service: since when, how many
    /// misses and the log tail at that moment. `None` unless the service is `unhealthy`.
    pub fn unhealthy_report(&self, name: &str) -> Option<UnhealthyReport> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.unhealthy.clone();
        }
        let svc = self.services.get(name)?;
        if svc.status.state != ServiceState::Unhealthy {
            return None;
        }
        let u = svc.probes.unhealthy.as_ref()?;
        Some(UnhealthyReport {
            since_secs: u.since.elapsed().as_secs(),
            since_epoch: u.since_epoch,
            misses: u.misses,
            lines: u.lines.clone(),
        })
    }

    /// Why the service is `degraded` (the failing `ready` probe's error or the repeated line);
    /// `None` in every other state.
    pub fn probe_error(&self, name: &str) -> Option<String> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.probe_error.clone();
        }
        let svc = self.services.get(name)?;
        if svc.status.state != ServiceState::Degraded {
            return None;
        }
        svc.probes.degraded.clone()
    }

    /// The readiness/hang facts every consumer reads, under one set of keys: the status file
    /// (`atlas dev --status`, MCP, the desktop's poll) and the runner wire (`state`/`rows`)
    /// both merge this object, so they can never drift apart.
    pub(crate) fn probe_facts_json(&self, name: &str) -> serde_json::Value {
        let report = self.unhealthy_report(name);
        let mut v = serde_json::json!({
            // `null` = no `ready` probe configured (or the service is not up).
            "ready": self.ready_ok(name),
            "probe_error": self.probe_error(name),
            "unhealthy_since": report.as_ref().map(|r| r.since_epoch),
            "unhealthy_misses": report.as_ref().map(|r| r.misses),
            "unhealthy_lines": report.map(|r| r.lines),
        });
        // `memory_rss`, `memory_trend`, `budget_state`, …: same producer for file and wire.
        if let (Some(o), serde_json::Value::Object(mem)) =
            (v.as_object_mut(), self.memory_facts_json(name))
        {
            o.extend(mem);
        }
        v
    }

    /// The readiness column: `None` without a `ready` probe (or while the service is not up),
    /// `Some(true)` while it passes, `Some(false)` while it fails.
    pub fn ready_ok(&self, name: &str) -> Option<bool> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.ready;
        }
        let svc = self.services.get(name)?;
        svc.probes.ready.as_ref()?;
        match svc.status.state {
            ServiceState::Running => Some(true),
            ServiceState::Degraded if svc.probes.ready_error.is_some() => Some(false),
            _ => None,
        }
    }

    /// Every `usage_every`: memory/CPU per service tree, fed to each service's memory watch
    /// (series, `memory_warn` / `max_memory` budget, leak detector) and acted on: `degraded` over
    /// the soft budget, and over `max_memory` (two samples in a row) restart or notify.
    pub(crate) async fn sample_usage(&mut self) {
        if self
            .sampled_at
            .is_some_and(|t| t.elapsed() < self.usage_every)
        {
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
        let t = crate::runtime::clock::now_secs();
        let mut outcomes = Vec::new();
        for (name, svc) in self.services.iter_mut() {
            svc.usage = usage.remove(name);
            let Some(u) = svc.usage else { continue };
            let reloads = svc
                .probes
                .ctx
                .lock()
                .map(|mut c| c.take_reloads())
                .unwrap_or_default();
            let o = svc.mem.on_sample(t, u.mem, &reloads);
            outcomes.push((name.clone(), u.mem, o));
        }
        for (name, used, o) in outcomes {
            self.apply_memory_outcome(&name, used, &o).await;
        }
        self.write_metrics_file();
    }

    /// The service stayed over `max_memory` for two samples: restart it (default, bounded by
    /// [`MEM_RESTARTS`]) or only say so (`on_memory_max: notify`, it turns `degraded`).
    pub(super) async fn on_over_max(&mut self, name: &str, used: u64) {
        use crate::config::OnMemoryMax;
        let Some(svc) = self.services.get_mut(name) else {
            return;
        };
        let (Some(max), on_max) = (svc.mem.limits.max, svc.mem.on_max) else {
            return;
        };
        let fmt = crate::runtime::memory::fmt_size;
        if on_max == OnMemoryMax::Notify {
            let message = format!(
                "using {} (max_memory {}), left running (on_memory_max: notify)",
                fmt(used),
                fmt(max)
            );
            let _ = self.event_tx.send(super::ManagerEvent::CriticalAlert {
                service: name.to_string(),
                message: message.clone(),
            });
            self.note(name, message);
            return;
        }
        svc.mem.restarts += 1;
        let tries = svc.mem.restarts;
        let give_up = tries > MEM_RESTARTS;
        let message = if give_up {
            format!(
                "using {} (max_memory {}) after {MEM_RESTARTS} restarts, stopped",
                fmt(used),
                fmt(max)
            )
        } else {
            format!(
                "using {} (max_memory {}), restarting ({tries}/{MEM_RESTARTS})",
                fmt(used),
                fmt(max)
            )
        };
        let _ = self.event_tx.send(super::ManagerEvent::CriticalAlert {
            service: name.to_string(),
            message: message.clone(),
        });
        if give_up {
            // Stop for good: an endless kill/respawn cycle is the loop this guards against.
            self.stop_service(name).await;
            self.set_state_if_changed(name, ServiceState::Failed);
        } else {
            self.restart_service(name).await;
        }
        self.note(name, message);
    }

    /// Make the state follow the budget right away instead of waiting for the next probe tick:
    /// `running` ⇄ `degraded` while the service has nothing else wrong with it.
    pub(super) fn refresh_memory_state(&mut self, name: &str) {
        let Some(svc) = self.services.get_mut(name) else {
            return;
        };
        let reason = svc.mem.warn_reason().or_else(|| svc.mem.over_reason());
        let next = match (&svc.status.state, reason) {
            (ServiceState::Running, Some(r)) => {
                svc.probes.degraded = Some(r);
                svc.mem.degrading = true;
                ServiceState::Degraded
            }
            // Still over budget and degraded only for it: the reason follows the live number.
            (ServiceState::Degraded, Some(r)) if svc.mem.degrading => {
                svc.probes.degraded = Some(r);
                return;
            }
            // Back under budget, and the degradation was only the memory's (a failing `ready`
            // probe or a repeating error would have replaced the reason and cleared the flag).
            (ServiceState::Degraded, None) if svc.mem.degrading => {
                svc.probes.degraded = None;
                svc.mem.degrading = false;
                ServiceState::Running
            }
            _ => return,
        };
        self.set_state_if_changed(name, next);
    }

    /// Latest memory/CPU of a running service.
    pub fn usage(&self, name: &str) -> Option<crate::runtime::usage::Usage> {
        self.services.get(name)?.usage
    }
}

/// `max_memory` restarts per run before the service is stopped and marked Failed.
pub(super) const MEM_RESTARTS: u32 = 3;

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
                let mut obj = serde_json::json!({
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
                    "port_conflict": self.port_conflict(name),
                    "mem_mb": service.usage.map(|u| u.mem >> 20),
                    "cpu": service.usage.map(|u| u.cpu.round() as u32),
                });
                if let (Some(o), serde_json::Value::Object(extra)) =
                    (obj.as_object_mut(), self.probe_facts_json(name))
                {
                    o.extend(extra);
                }
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
    write_atomic(dir, ".tui-status.json", text);
}

/// The same tmp-then-rename write for any file of the log dir (`.tui-metrics.json` too).
pub(super) fn write_atomic(dir: &std::path::Path, file: &str, text: &str) {
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!("{file}.tmp"));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(file));
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
