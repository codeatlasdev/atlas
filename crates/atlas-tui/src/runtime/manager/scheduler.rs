//! Dependency gating + restart handling. `schedule()` runs from
//! `start_app_services` and then on every `check_health` tick (the one place
//! children are reaped), so restarts and dependency spawns can never fire twice.

use std::time::Instant;

use super::ServiceManager;
use super::policy::{DepFacts, RestartDecision, Verdict, evaluate};
use super::types::ManagerEvent;
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub(super) fn note(&self, name: &str, message: String) {
        let _ = self.event_tx.send(ManagerEvent::ServiceNote {
            name: name.to_string(),
            message,
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
                self.note(&n, "stable for 60s, restart counter reset".into());
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
            self.note(&name, "restarting".into());
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
            let mut waiting_on: Option<String> = None;
            let mut failed_dep: Option<String> = None;
            for (dep, cond) in s.sched.deps.clone() {
                match evaluate(cond, &self.dep_facts(&dep)) {
                    Verdict::Ready => {}
                    Verdict::Waiting => {
                        waiting_on.get_or_insert_with(|| format!("{dep} ({})", cond.as_str()));
                    }
                    Verdict::Failed => {
                        failed_dep = Some(dep);
                        break;
                    }
                }
            }

            if let Some(dep) = failed_dep {
                let reason = format!("dependency {dep} failed");
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
                    .is_some_and(|s| s.sched.waiting.replace(w.clone()).as_deref() != Some(&w));
                if changed {
                    self.note(name, format!("waiting for {w}"));
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
                let code = if exit_ok { "exited" } else { "crashed" };
                self.note(
                    name,
                    format!(
                        "{code}; restarting in {}s ({attempt}/{limit})",
                        delay.as_secs()
                    ),
                );
                // Starting (not Failed): no critical alert, spinner in the sidebar.
                Some(ServiceState::Starting)
            }
            RestartDecision::GiveUp { attempts } => {
                let reason = format!("gave up after {attempts} restarts");
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
                    self.note(name, "completed successfully".into());
                    Some(ServiceState::Stopped)
                } else {
                    let had_policy = policy != crate::config::RestartPolicy::No;
                    if had_policy && exit_ok {
                        // on_failure + clean exit: finished on purpose, not a failure.
                        if let Some(s) = self.services.get_mut(name) {
                            s.sched.completed_ok = true;
                        }
                        self.note(name, "exited cleanly".into());
                        return Some(ServiceState::Stopped);
                    }
                    if let Some(s) = self.services.get_mut(name) {
                        s.sched.fail_reason = Some(if exit_ok {
                            "exited".into()
                        } else {
                            "exited with error".into()
                        });
                    }
                    None
                }
            }
        }
    }

    /// Sidebar annotation for `name`: waiting reason, restart countdown, gave-up.
    pub fn service_note(&self, name: &str) -> Option<String> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.note.clone();
        }
        let s = self.services.get(name)?;
        if let Some(at) = s.sched.tracker.pending_at {
            let secs = at.saturating_duration_since(Instant::now()).as_secs() + 1;
            let max = match s
                .sched
                .max_restarts
                .unwrap_or(super::policy::DEFAULT_MAX_RESTARTS)
            {
                0 => "∞".to_string(),
                m => m.to_string(),
            };
            return Some(format!(
                "restarting in {secs}s ({}/{max})",
                s.sched.tracker.attempts
            ));
        }
        if let Some(w) = &s.sched.waiting {
            return Some(format!("waiting for {w}"));
        }
        if let Some((port, secs)) = self.start_wait(name) {
            let what = port.map_or("ready_log".to_string(), |p| format!(":{p}"));
            return Some(if s.sched.start_warned {
                format!("start timeout: still waiting for {what} ({secs}s)")
            } else {
                format!("waiting for {what} ({secs}s)")
            });
        }
        match s.status.state {
            ServiceState::Failed => s.sched.fail_reason.clone(),
            ServiceState::Stopped if s.sched.completed_ok => Some("done".into()),
            ServiceState::Running if s.sched.tracker.attempts > 0 => {
                Some(format!("restarted ×{}", s.sched.tracker.attempts))
            }
            _ => None,
        }
    }

    /// `(port, seconds)` while a spawned service is Starting and waiting on its
    /// probe: `Some(port)` for a port/health wait, `None` port for `ready_log`.
    /// `None` when nothing is being waited on (including no-probe services,
    /// which settle after `MIN_UPTIME` on their own).
    pub fn start_wait(&self, name: &str) -> Option<(Option<u16>, u64)> {
        let s = self.services.get(name)?;
        if s.status.state != ServiceState::Starting
            || s.child.is_none()
            || s.sched.tracker.pending_at.is_some()
        {
            return None;
        }
        let waits_on_log = s.sched.ready_log.is_some();
        if !waits_on_log && s.status.port.is_none() && s.status.health_url.is_none() {
            return None;
        }
        let secs = s.sched.starting_since?.elapsed().as_secs();
        let port = if waits_on_log { None } else { s.status.port };
        Some((port, secs))
    }

    /// Last exit code of the service's process (`None` while running, never
    /// exited, or killed by a signal).
    pub fn exit_code(&self, name: &str) -> Option<i32> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.exit_code;
        }
        let s = self.services.get(name)?;
        if s.child.is_some() { None } else { s.exit_code }
    }

    /// Restarts attempted in the current streak (0 after 60s of stability).
    pub fn restart_count(&self, name: &str) -> u32 {
        if self.mirror.is_some() {
            return self.mirror_facts(name).map_or(0, |f| f.restarts);
        }
        self.services
            .get(name)
            .map_or(0, |s| s.sched.tracker.attempts)
    }

    /// Whole seconds the service has been Running; `None` when it is not.
    pub fn uptime_secs(&self, name: &str) -> Option<u64> {
        let s = self.services.get(name)?;
        (s.status.state == ServiceState::Running)
            .then(|| s.sched.running_since.map(|t| t.elapsed().as_secs()))
            .flatten()
    }

    /// One-shot task (explicit `oneshot`, or awaited via `completed_successfully`).
    pub fn is_oneshot(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| s.sched.oneshot)
    }

    /// Label of the action that suspended `name`, while it is suspended.
    pub fn paused_by(&self, name: &str) -> Option<String> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.paused_by.clone();
        }
        let s = self.services.get(name)?;
        (s.status.state == ServiceState::Suspended)
            .then(|| s.sched.paused_by.clone())
            .flatten()
    }

    /// Test hook: pretend `name` finished as a one-shot.
    #[cfg(test)]
    pub(crate) fn test_set_completed(&mut self, name: &str) {
        if let Some(s) = self.services.get_mut(name) {
            s.sched.completed_ok = true;
        }
    }

    /// Test hook: pretend the last child of `name` exited with `code`.
    #[cfg(test)]
    pub(crate) fn test_set_exit_code(&mut self, name: &str, code: i32) {
        if let Some(s) = self.services.get_mut(name) {
            s.exit_code = Some(code);
        }
    }

    /// Whether `name` was left out of this run's selection and not started since.
    pub fn is_deselected(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| s.deselected)
    }

    /// Whether `name` is skipped at boot (`autostart: false`) and not yet started.
    pub fn is_manual_start(&self, name: &str) -> bool {
        self.services.get(name).is_some_and(|s| !s.autostart)
    }

    /// Fire the one-time start-timeout warning for services stuck in Starting.
    fn check_start_timeouts(&mut self, now: Instant) {
        let mut due: Vec<(String, u64, String, bool)> = Vec::new();
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
                let what = port.map_or("ready_log".to_string(), |p| format!(":{p}"));
                due.push((name.clone(), waited, what, s.critical));
            }
        }
        for (name, waited, what, critical) in due {
            if let Some(s) = self.services.get_mut(&name) {
                s.sched.start_warned = true;
            }
            let message = format!("still starting after {waited}s: waiting for {what}");
            self.note(&name, message.clone());
            if critical {
                let _ = self.event_tx.send(ManagerEvent::CriticalAlert {
                    service: name.clone(),
                    message,
                });
            }
            let _ = self.event_tx.send(ManagerEvent::StatusChanged);
        }
    }

    /// Fail-reason for CI output.
    pub fn fail_reason(&self, name: &str) -> Option<String> {
        self.services.get(name)?.sched.fail_reason.clone()
    }

    /// True if the service finished successfully (one-shot done).
    pub fn completed_ok(&self, name: &str) -> bool {
        if self.mirror.is_some() {
            return self.mirror_facts(name).is_some_and(|f| f.completed_ok);
        }
        self.services
            .get(name)
            .is_some_and(|s| s.sched.completed_ok)
    }

    /// True while anything is queued, waiting on dependencies, or due to restart.
    pub fn scheduling_pending(&self) -> bool {
        self.services
            .values()
            .any(|s| s.sched.pending_start || s.sched.tracker.pending_at.is_some())
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::super::ServiceManager;
    use super::super::types::ManagerEvent;
    use crate::config::{DependsOn, ProjectConfig, RestartPolicy, ServiceDef};
    use crate::runtime::service::ServiceState;

    fn svc(cmd: &str) -> ServiceDef {
        ServiceDef {
            command: Some(cmd.to_string()),
            ..Default::default()
        }
    }

    fn deps(pairs: &[(&str, &str)]) -> Option<DependsOn> {
        let yaml: String = pairs.iter().map(|(n, c)| format!("{n}: {c}\n")).collect();
        Some(serde_yaml::from_str(&yaml).unwrap())
    }

    fn manager(
        dir: &tempfile::TempDir,
        services: Vec<(&str, ServiceDef)>,
    ) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
        let config = ProjectConfig {
            name: "t".into(),
            services: services
                .into_iter()
                .map(|(n, d)| (n.to_string(), d))
                .collect::<HashMap<_, _>>(),
            ..Default::default()
        };
        let (tx, rx) = mpsc::unbounded_channel();
        (
            ServiceManager::new(PathBuf::from(dir.path()), &config, tx),
            rx,
        )
    }

    /// Drive the same loop headless does: apply watcher/log events and tick
    /// `check_health` until `done` or the deadline.
    async fn drive(
        m: &mut ServiceManager,
        rx: &mut mpsc::UnboundedReceiver<ManagerEvent>,
        secs: u64,
        mut done: impl FnMut(&ServiceManager) -> bool,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        while tokio::time::Instant::now() < deadline {
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    ManagerEvent::StateChanged { name, state } => m.update_state(&name, state),
                    ManagerEvent::LogReady { name } => m.mark_log_ready(&name),
                    _ => {}
                }
            }
            m.check_health().await;
            if done(m) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn dependent_waits_for_oneshot_to_complete_then_spawns() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![
                ("migrate", svc("sleep 1")),
                (
                    "api",
                    ServiceDef {
                        depends_on: deps(&[("migrate", "completed_successfully")]),
                        ..svc("sleep 30")
                    },
                ),
            ],
        );
        m.start_app_services(&names(&["migrate", "api"])).await;
        assert_eq!(m.get_state("api"), Some(ServiceState::Stopped), "gated");
        assert_eq!(
            m.service_note("api").as_deref(),
            Some("waiting for migrate (completed_successfully)")
        );

        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("api") == Some(ServiceState::Running)
        })
        .await;
        assert!(m.completed_ok("migrate"), "implied one-shot completed");
        assert_eq!(m.get_state("migrate"), Some(ServiceState::Stopped));
        assert_eq!(m.get_state("api"), Some(ServiceState::Running));
        m.stop_all().await;
    }

    #[tokio::test]
    async fn failed_dependency_fails_dependents_transitively_with_reason() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![
                ("bad", svc("exit 3")),
                (
                    "mid",
                    ServiceDef {
                        depends_on: deps(&[("bad", "completed_successfully")]),
                        ..svc("sleep 30")
                    },
                ),
                (
                    "leaf",
                    ServiceDef {
                        depends_on: deps(&[("mid", "healthy")]),
                        ..svc("sleep 30")
                    },
                ),
            ],
        );
        m.start_app_services(&names(&["bad", "mid", "leaf"])).await;
        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("leaf") == Some(ServiceState::Failed)
        })
        .await;
        assert_eq!(m.get_state("bad"), Some(ServiceState::Failed));
        assert_eq!(m.get_state("mid"), Some(ServiceState::Failed));
        assert_eq!(m.get_state("leaf"), Some(ServiceState::Failed));
        assert_eq!(
            m.fail_reason("mid").as_deref(),
            Some("dependency bad failed")
        );
        assert_eq!(
            m.fail_reason("leaf").as_deref(),
            Some("dependency mid failed")
        );
        assert!(!m.scheduling_pending(), "nothing may wait forever");
        m.stop_all().await;
    }

    #[tokio::test]
    async fn log_ready_gates_on_matching_line_and_marks_dependency_running() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![
                (
                    "api",
                    ServiceDef {
                        ready_log: Some("listening on \\d+".into()),
                        // No port/health: only the log can make it ready.
                        ..svc("sleep 1; echo listening on 80; sleep 30")
                    },
                ),
                (
                    "web",
                    ServiceDef {
                        depends_on: deps(&[("api", "log_ready")]),
                        ..svc("sleep 30")
                    },
                ),
            ],
        );
        m.start_app_services(&names(&["api", "web"])).await;
        // Child is alive but silent: web must not start on liveness alone.
        tokio::time::sleep(Duration::from_millis(300)).await;
        m.check_health().await;
        assert_eq!(m.get_state("web"), Some(ServiceState::Stopped));

        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("web") == Some(ServiceState::Running)
        })
        .await;
        assert_eq!(m.get_state("api"), Some(ServiceState::Running));
        assert_eq!(m.get_state("web"), Some(ServiceState::Running));
        m.stop_all().await;
    }

    #[tokio::test]
    async fn on_failure_restarts_then_gives_up_and_never_double_spawns() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("spawns");
        let (mut m, mut rx) = manager(
            &dir,
            vec![(
                "crashy",
                ServiceDef {
                    restart: RestartPolicy::OnFailure,
                    max_restarts: Some(2),
                    ..svc(&format!("echo x >> {}; exit 1", log.display()))
                },
            )],
        );
        m.start_app_services(&names(&["crashy"])).await;
        drive(&mut m, &mut rx, 15, |m| {
            m.fail_reason("crashy")
                .is_some_and(|r| r.contains("gave up"))
        })
        .await;
        assert_eq!(m.get_state("crashy"), Some(ServiceState::Failed));
        assert_eq!(
            m.fail_reason("crashy").as_deref(),
            Some("gave up after 2 restarts")
        );
        // Keep ticking: a gave-up service must stay down.
        drive(&mut m, &mut rx, 2, |_| false).await;
        let spawns = std::fs::read_to_string(&log).unwrap().lines().count();
        assert_eq!(spawns, 3, "1 initial + exactly 2 restarts");
        m.stop_all().await;
    }

    #[tokio::test]
    async fn on_failure_leaves_clean_exit_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![(
                "task",
                ServiceDef {
                    restart: RestartPolicy::OnFailure,
                    ..svc("exit 0")
                },
            )],
        );
        m.start_app_services(&names(&["task"])).await;
        drive(&mut m, &mut rx, 5, |m| {
            m.get_state("task") == Some(ServiceState::Stopped)
        })
        .await;
        assert_eq!(m.get_state("task"), Some(ServiceState::Stopped));
        assert!(m.completed_ok("task"));
        assert!(!m.scheduling_pending());
        m.stop_all().await;
    }

    #[tokio::test]
    async fn restart_no_keeps_legacy_failed_on_exit() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(&dir, vec![("svc", svc("exit 1"))]);
        m.start_app_services(&names(&["svc"])).await;
        drive(&mut m, &mut rx, 5, |m| {
            m.get_state("svc") == Some(ServiceState::Failed)
        })
        .await;
        assert_eq!(m.get_state("svc"), Some(ServiceState::Failed));
        assert!(!m.scheduling_pending());
    }

    #[tokio::test]
    async fn stop_and_quit_cancel_scheduled_restart() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("spawns");
        let (mut m, mut rx) = manager(
            &dir,
            vec![(
                "crashy",
                ServiceDef {
                    restart: RestartPolicy::Always,
                    max_restarts: Some(0),
                    ..svc(&format!("echo x >> {}; exit 1", log.display()))
                },
            )],
        );
        m.start_app_services(&names(&["crashy"])).await;
        drive(&mut m, &mut rx, 5, |m| m.scheduling_pending()).await;
        assert!(m.scheduling_pending(), "restart should be scheduled");

        m.stop_service("crashy").await;
        assert!(!m.scheduling_pending(), "user stop cancels the restart");
        let before = std::fs::read_to_string(&log).unwrap().lines().count();
        drive(&mut m, &mut rx, 3, |_| false).await;
        let after = std::fs::read_to_string(&log).unwrap().lines().count();
        assert_eq!(before, after, "no respawn after a user stop");
        assert_eq!(m.get_state("crashy"), Some(ServiceState::Stopped));

        // And quit: restart again, then stop_all mid-backoff.
        m.restart_service("crashy").await;
        drive(&mut m, &mut rx, 5, |m| m.scheduling_pending()).await;
        m.stop_all().await;
        assert!(!m.scheduling_pending(), "stop_all cancels pending restarts");
    }

    /// Audit #1: a `ready_log` service with no port must stay Starting while the
    /// child is alive but silent, and turn Running only on the match.
    #[tokio::test]
    async fn ready_log_service_stays_starting_until_regex_matches() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![(
                "slow",
                ServiceDef {
                    ready_log: Some("Server ready".into()),
                    ..svc("echo compiling; sleep 3; echo Server ready; sleep 30")
                },
            )],
        );
        m.start_app_services(&names(&["slow"])).await;
        // Several health ticks across > MIN_UPTIME while the log is still silent.
        drive(&mut m, &mut rx, 2, |_| false).await;
        assert_eq!(
            m.get_state("slow"),
            Some(ServiceState::Starting),
            "alive child must not promote a ready_log service"
        );
        assert_eq!(
            m.service_note("slow")
                .as_deref()
                .map(|n| n.starts_with("waiting for ready_log (")),
            Some(true),
            "{:?}",
            m.service_note("slow")
        );
        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("slow") == Some(ServiceState::Running)
        })
        .await;
        assert_eq!(m.get_state("slow"), Some(ServiceState::Running));
        assert_eq!(m.start_wait("slow"), None);
        assert!(m.uptime_secs("slow").is_some());
        m.stop_all().await;
    }

    /// Audit #2: a crash-looping port-less service never turns Running and never
    /// satisfies a `healthy` dependent.
    #[tokio::test]
    async fn portless_crash_loop_never_runs_and_does_not_unblock_healthy_dep() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![
                (
                    "worker",
                    ServiceDef {
                        restart: RestartPolicy::OnFailure,
                        max_restarts: Some(2),
                        ..svc("sleep 1; exit 2")
                    },
                ),
                (
                    "dependent",
                    ServiceDef {
                        depends_on: deps(&[("worker", "healthy")]),
                        ..svc("sleep 30")
                    },
                ),
            ],
        );
        m.start_app_services(&names(&["worker", "dependent"])).await;
        let mut ever_running = false;
        drive(&mut m, &mut rx, 15, |m| {
            ever_running |= m.get_state("worker") == Some(ServiceState::Running);
            m.fail_reason("worker")
                .is_some_and(|r| r.contains("gave up"))
        })
        .await;
        assert!(!ever_running, "crash loop must never flash Running");
        assert_eq!(m.get_state("worker"), Some(ServiceState::Failed));
        assert_eq!(m.exit_code("worker"), Some(2));
        assert_eq!(m.restart_count("worker"), 2);
        // Dependent was never spawned on a false "healthy": it fails with the dep.
        drive(&mut m, &mut rx, 3, |m| {
            m.get_state("dependent") == Some(ServiceState::Failed)
        })
        .await;
        assert_eq!(m.get_state("dependent"), Some(ServiceState::Failed));
        assert_eq!(
            m.fail_reason("dependent").as_deref(),
            Some("dependency worker failed")
        );
        m.stop_all().await;
    }

    /// A port-less service that stays alive past MIN_UPTIME does turn Running.
    #[tokio::test]
    async fn portless_service_runs_after_min_uptime() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(&dir, vec![("idle", svc("sleep 30"))]);
        m.start_app_services(&names(&["idle"])).await;
        m.check_health().await;
        assert_eq!(
            m.get_state("idle"),
            Some(ServiceState::Starting),
            "not green at t=0"
        );
        drive(&mut m, &mut rx, 6, |m| {
            m.get_state("idle") == Some(ServiceState::Running)
        })
        .await;
        assert_eq!(m.get_state("idle"), Some(ServiceState::Running));
        assert_eq!(m.exit_code("idle"), None);
        m.stop_all().await;
    }

    /// Item 3: a port that never opens shows `waiting for :PORT (Ns)`, then one
    /// warn note, and a CriticalAlert when the service is `critical`.
    #[tokio::test]
    async fn start_timeout_warns_and_alerts_critical_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![(
                "badport",
                ServiceDef {
                    port: Some(1),
                    critical: Some(true),
                    start_timeout: Some(1),
                    ..svc("sleep 30")
                },
            )],
        );
        m.start_app_services(&names(&["badport"])).await;
        let (mut notes, mut alerts) = (0, 0);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut saw_waiting = false;
        while tokio::time::Instant::now() < deadline && (notes == 0 || alerts == 0) {
            m.check_health().await;
            saw_waiting |= m
                .service_note("badport")
                .is_some_and(|n| n.starts_with("waiting for :1 ("));
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    ManagerEvent::ServiceNote { message, .. }
                        if message.starts_with("still starting after") =>
                    {
                        notes += 1;
                    }
                    ManagerEvent::CriticalAlert { service, .. } if service == "badport" => {
                        alerts += 1;
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        assert!(saw_waiting, "sidebar note must show the wait");
        assert_eq!((notes, alerts), (1, 1));
        assert_eq!(m.get_state("badport"), Some(ServiceState::Starting));
        assert!(
            m.service_note("badport")
                .is_some_and(|n| n.starts_with("start timeout: still waiting for :1 (")),
            "{:?}",
            m.service_note("badport")
        );
        // No second firing.
        m.check_health().await;
        let again = std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|e| matches!(e, ManagerEvent::CriticalAlert { .. }))
            .count();
        assert_eq!(again, 0);
        m.stop_all().await;
    }

    /// Item 6: `autostart: false` is excluded from boot's app list and starts on demand.
    #[tokio::test]
    async fn autostart_false_is_not_booted_but_starts_on_demand() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            &dir,
            vec![
                ("api", svc("sleep 30")),
                (
                    "lazy",
                    ServiceDef {
                        autostart: Some(false),
                        ..svc("sleep 30")
                    },
                ),
            ],
        );
        m.start_all().await;
        let app_names = loop {
            match rx.recv().await {
                Some(ManagerEvent::InfraReady { app_names }) => break app_names,
                Some(_) => {}
                None => panic!("no InfraReady"),
            }
        };
        assert_eq!(app_names, vec!["api".to_string()]);
        assert!(m.is_manual_start("lazy"));
        m.start_app_services(&app_names).await;
        assert_eq!(m.get_state("lazy"), Some(ServiceState::Stopped));
        m.restart_service("lazy").await;
        assert_eq!(m.get_state("lazy"), Some(ServiceState::Starting));
        m.stop_all().await;
    }

    fn manager_selected(
        dir: &tempfile::TempDir,
        services: Vec<(&str, ServiceDef)>,
        select: &[&str],
        no_deps: bool,
    ) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
        let config = ProjectConfig {
            name: "t".into(),
            services: services
                .into_iter()
                .map(|(n, d)| (n.to_string(), d))
                .collect::<HashMap<_, _>>(),
            ..Default::default()
        };
        let req = crate::config::SelectionRequest {
            services: select.iter().map(|s| s.to_string()).collect(),
            profiles: vec![],
            no_deps,
        };
        let selection = crate::config::resolve_selection(&config, &req).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        (
            ServiceManager::with_selection(PathBuf::from(dir.path()), &config, tx, &selection),
            rx,
        )
    }

    /// `atlas dev web`: web + its dependency chain boot; the rest stay `off`
    /// (Disabled), are marked in the status file and start by hand.
    #[tokio::test]
    async fn deselected_services_are_off_marked_and_startable_by_hand() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager_selected(
            &dir,
            vec![
                ("db", svc("sleep 30")),
                (
                    "api",
                    ServiceDef {
                        depends_on: deps(&[("db", "started")]),
                        ..svc("sleep 30")
                    },
                ),
                (
                    "web",
                    ServiceDef {
                        depends_on: deps(&[("api", "started")]),
                        ..svc("sleep 30")
                    },
                ),
                ("worker", svc("sleep 30")),
            ],
            &["web"],
            false,
        );
        assert_eq!(m.get_state("worker"), Some(ServiceState::Disabled));
        assert!(m.is_deselected("worker") && !m.is_deselected("web"));
        m.start_all().await;
        let app_names = loop {
            match rx.recv().await {
                Some(ManagerEvent::InfraReady { app_names }) => break app_names,
                Some(_) => {}
                None => panic!("no InfraReady"),
            }
        };
        assert_eq!(app_names, names(&["db", "api", "web"]), "dependency order");
        m.start_app_services(&app_names).await;
        drive(&mut m, &mut rx, 8, |m| {
            ["db", "api", "web"]
                .iter()
                .all(|n| m.get_state(n) == Some(ServiceState::Running))
        })
        .await;
        assert_eq!(m.get_state("worker"), Some(ServiceState::Disabled));
        assert!(m.services["worker"].child.is_none(), "never spawned");

        m.write_status_file();
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["services"]["worker"]["state"], "disabled");
        assert_eq!(v["services"]["worker"]["deselected"], true);
        assert_eq!(v["services"]["web"]["deselected"], false);

        // Startable by hand (menu / palette / atlas_start_service): joins the run.
        m.restart_service("worker").await;
        assert_eq!(m.get_state("worker"), Some(ServiceState::Starting));
        assert!(!m.is_deselected("worker"));
        m.stop_all().await;
    }

    /// `--no-deps`: the dependency is off and never gates the selected service.
    #[tokio::test]
    async fn no_deps_leaves_dependency_off_and_does_not_gate() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager_selected(
            &dir,
            vec![
                ("db", svc("sleep 30")),
                (
                    "api",
                    ServiceDef {
                        depends_on: deps(&[("db", "healthy")]),
                        ..svc("sleep 30")
                    },
                ),
            ],
            &["api"],
            true,
        );
        assert_eq!(m.get_state("db"), Some(ServiceState::Disabled));
        m.start_app_services(&names(&["api"])).await;
        assert_eq!(m.get_state("api"), Some(ServiceState::Starting));
        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("api") == Some(ServiceState::Running)
        })
        .await;
        assert_eq!(m.get_state("api"), Some(ServiceState::Running));
        assert_eq!(m.get_state("db"), Some(ServiceState::Disabled));
        m.stop_all().await;
    }

    /// Item 5: exit code, restart count and uptime land in the status file.
    #[tokio::test]
    async fn status_file_carries_exit_code_restarts_and_uptime() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(&dir, vec![("boom", svc("exit 7")), ("ok", svc("sleep 30"))]);
        m.start_app_services(&names(&["boom", "ok"])).await;
        drive(&mut m, &mut rx, 8, |m| {
            m.get_state("boom") == Some(ServiceState::Failed)
                && m.get_state("ok") == Some(ServiceState::Running)
        })
        .await;
        m.write_status_file();
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["services"]["boom"]["exit_code"], 7);
        assert!(v["services"]["boom"]["uptime_s"].is_null());
        assert!(v["services"]["ok"]["exit_code"].is_null());
        assert!(v["services"]["ok"]["uptime_s"].as_u64().is_some());
        assert!(v["services"]["ok"]["started_at"].as_u64().is_some());
        assert_eq!(v["services"]["ok"]["restarts"], 0);
        m.stop_all().await;
    }
}
