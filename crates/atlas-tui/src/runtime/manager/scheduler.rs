//! Dependency gating + restart handling. `schedule()` runs from
//! `start_app_services` and then on every `check_health` tick (the one place
//! children are reaped), so restarts and dependency spawns can never fire twice.

use std::time::Instant;

use serde_json::json;

use super::ServiceManager;
use super::policy::{DepFacts, RestartDecision, Verdict, evaluate};
use super::types::ManagerEvent;
use crate::runtime::note::Note;
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub(super) fn note(&self, name: &str, note: Note) {
        let _ = self.event_tx.send(ManagerEvent::ServiceNote {
            name: name.to_string(),
            note,
        });
    }

    /// Facts the scheduler needs about one service.
    fn dep_facts(&self, name: &str) -> DepFacts {
        let Some(s) = self.services.get(name).filter(|s| !s.deselected) else {
            // Disabled / deselected (`--no-deps`) / unknown services never gate anyone.
            return DepFacts {
                spawned: true,
                healthy: true,
                log_ready: true,
                completed_ok: true,
                failed: false,
            };
        };
        let retrying = s.sched.tracker.pending_at.is_some();
        // Command-less (infra) services are never spawned by atlas: "spawned" means
        // the port wait already finished one way or the other.
        let spawned = s.sched.spawned
            || (s.command.is_none()
                && matches!(s.status.state, ServiceState::Running | ServiceState::Failed));
        DepFacts {
            spawned,
            healthy: s.status.state == ServiceState::Running,
            log_ready: s.sched.log_ready.load(std::sync::atomic::Ordering::Acquire),
            completed_ok: s.sched.completed_ok,
            failed: !retrying && s.status.state == ServiceState::Failed,
        }
    }

    /// Queue `name` to start once its dependencies allow it.
    pub(crate) fn queue_start(&mut self, name: &str) {
        if let Some(s) = self.services.get_mut(name) {
            s.sched.pending_start = true;
        }
    }

    /// Spawn queued services whose dependencies are satisfied, fail those whose
    /// dependencies can never be, fire due restarts, and reset stable counters.
    pub(crate) async fn schedule(&mut self) {
        let now = Instant::now();
        let names = self.order.clone();

        // 1. Restart counters: Running for 60s earns a clean slate.
        for name in &names {
            if let Some(s) = self.services.get_mut(name)
                && s.sched.tracker.reset_if_stable(now)
            {
                let n = name.clone();
                self.note(
                    &n,
                    Note::new(
                        "sched.stable",
                        json!({"secs": super::policy::RESET_AFTER.as_secs()}),
                    ),
                );
            }
        }

        // 1b. Start timeout: Starting too long → one warn note (+ critical alert).
        self.check_start_timeouts(now);

        // 2. Due restarts.
        let due: Vec<String> = names
            .iter()
            .filter(|n| {
                self.services
                    .get_mut(n.as_str())
                    .is_some_and(|s| s.sched.tracker.take_due(now))
            })
            .cloned()
            .collect();
        for name in due {
            self.note(&name, Note::new("sched.restarting", json!({})));
            self.start_service(&name).await;
        }

        // 3. Dependency gate, in topological order so a failure cascades down a
        // chain within a single pass.
        for name in &names {
            let Some(s) = self.services.get(name) else {
                continue;
            };
            if !s.sched.pending_start {
                continue;
            }
            let mut waiting_on: Option<Note> = None;
            let mut failed_dep: Option<String> = None;
            for (dep, cond) in s.sched.deps.clone() {
                match evaluate(cond, &self.dep_facts(&dep)) {
                    Verdict::Ready => {}
                    Verdict::Waiting => {
                        waiting_on.get_or_insert_with(|| {
                            Note::new(
                                "sched.waiting_dep",
                                json!({"dep": dep, "condition": cond.as_str()}),
                            )
                        });
                    }
                    Verdict::Failed => {
                        failed_dep = Some(dep);
                        break;
                    }
                }
            }

            if let Some(dep) = failed_dep {
                let reason = Note::new("sched.dependency_failed", json!({"dep": dep}));
                if let Some(s) = self.services.get_mut(name) {
                    s.sched.pending_start = false;
                    s.sched.waiting = None;
                    s.sched.fail_reason = Some(reason.clone());
                }
                self.note(name, reason);
                self.set_state_if_changed(name, ServiceState::Failed);
            } else if let Some(w) = waiting_on {
                let changed = self
                    .services
                    .get_mut(name)
                    .is_some_and(|s| s.sched.waiting.replace(w.clone()).as_ref() != Some(&w));
                if changed {
                    self.note(name, w);
                    let _ = self.event_tx.send(ManagerEvent::StatusChanged);
                }
            } else {
                if let Some(s) = self.services.get_mut(name) {
                    s.sched.pending_start = false;
                    s.sched.waiting = None;
                }
                self.start_service(name).await;
            }
        }
    }

    /// A spawned child was reaped. Decide: finished one-shot, restart, give up,
    /// or plain failure. Returns the state the service should settle in, or
    /// `None` when the caller should fall through to its default handling.
    pub(crate) fn on_child_exit(&mut self, name: &str, exit_ok: bool) -> Option<ServiceState> {
        let now = Instant::now();
        let s = self.services.get_mut(name)?;
        // User stop / quit / action pause own the lifecycle: never auto-restart.
        if matches!(
            s.status.state,
            ServiceState::Stopped | ServiceState::Disabled | ServiceState::Suspended
        ) {
            return None;
        }
        let (policy, max) = (s.sched.restart, s.sched.max_restarts);
        let decision = s.sched.tracker.on_exit(policy, max, exit_ok, now);
        match decision {
            RestartDecision::Restart { attempt, delay } => {
                let limit = match max.unwrap_or(super::policy::DEFAULT_MAX_RESTARTS) {
                    0 => "∞".to_string(),
                    m => m.to_string(),
                };
                let code = if exit_ok {
                    "sched.exited_restart"
                } else {
                    "sched.crashed_restart"
                };
                self.note(
                    name,
                    Note::new(
                        code,
                        json!({"secs": delay.as_secs(), "attempt": attempt, "limit": limit}),
                    ),
                );
                // Starting (not Failed): no critical alert, spinner in the sidebar.
                Some(ServiceState::Starting)
            }
            RestartDecision::GiveUp { attempts } => {
                let reason = Note::new("sched.gave_up", json!({"attempts": attempts}));
                if let Some(s) = self.services.get_mut(name) {
                    s.sched.gave_up = true;
                    s.sched.fail_reason = Some(reason.clone());
                }
                self.note(name, reason);
                Some(ServiceState::Failed)
            }
            RestartDecision::No => {
                if exit_ok && s.sched.oneshot {
                    s.sched.completed_ok = true;
                    self.note(name, Note::new("sched.completed", json!({})));
                    Some(ServiceState::Stopped)
                } else {
                    let had_policy = policy != crate::config::RestartPolicy::No;
                    if had_policy && exit_ok {
                        // on_failure + clean exit: finished on purpose, not a failure.
                        if let Some(s) = self.services.get_mut(name) {
                            s.sched.completed_ok = true;
                        }
                        self.note(name, Note::new("sched.exited_cleanly", json!({})));
                        return Some(ServiceState::Stopped);
                    }
                    if let Some(s) = self.services.get_mut(name) {
                        s.sched.fail_reason = Some(Note::new(
                            if exit_ok {
                                "sched.exited"
                            } else {
                                "sched.exited_error"
                            },
                            json!({}),
                        ));
                    }
                    None
                }
            }
        }
    }

    /// Fire the one-time start-timeout warning for services stuck in Starting.
    fn check_start_timeouts(&mut self, now: Instant) {
        let mut due: Vec<(String, Note, bool)> = Vec::new();
        for name in &self.order {
            let Some(s) = self.services.get(name) else {
                continue;
            };
            if s.sched.start_warned || s.sched.start_timeout == 0 {
                continue;
            }
            let Some((port, secs)) = self.start_wait(name) else {
                continue;
            };
            let waited = s
                .sched
                .starting_since
                .map_or(secs, |t| now.saturating_duration_since(t).as_secs());
            if waited >= s.sched.start_timeout {
                let note = match port {
                    Some(p) => Note::new(
                        "sched.still_starting_port",
                        json!({"waited": waited, "port": p}),
                    ),
                    None => Note::new("sched.still_starting_log", json!({"waited": waited})),
                };
                due.push((name.clone(), note, s.critical));
            }
        }
        for (name, note, critical) in due {
            if let Some(s) = self.services.get_mut(&name) {
                s.sched.start_warned = true;
            }
            self.note(&name, note.clone());
            if critical {
                let _ = self.event_tx.send(ManagerEvent::CriticalAlert {
                    service: name.clone(),
                    note,
                });
            }
            let _ = self.event_tx.send(ManagerEvent::StatusChanged);
        }
    }

    /// A `ready_log` line matched for `name`: mark ready and Running.
    pub fn mark_log_ready(&mut self, name: &str) {
        if let Some(s) = self.services.get_mut(name) {
            s.sched
                .log_ready
                .store(true, std::sync::atomic::Ordering::Release);
            if matches!(
                s.status.state,
                ServiceState::Starting | ServiceState::Failed
            ) && s.child.is_some()
            {
                self.set_state_if_changed(name, ServiceState::Running);
            }
        }
    }
}
