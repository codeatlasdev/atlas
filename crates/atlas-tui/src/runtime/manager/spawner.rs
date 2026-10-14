use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use super::ServiceManager;
use super::types::{
    MIN_UPTIME, ManagerEvent, PROBE_FAILS_TO_FAIL, WATCH_HEALTHY_INTERVAL, WATCH_PROBE_TIMEOUT,
    WATCH_RETRY_INTERVAL, WATCH_STARTUP_INTERVAL,
};
use crate::runtime::logs::{LogStream, spawn_line_pump};
use crate::runtime::service::ServiceState;

impl ServiceManager {
    pub(crate) async fn start_service(&mut self, name: &str) {
        let can_start = match self.services.get_mut(name) {
            Some(service) => match service.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(None) => false,
                    Ok(Some(_)) | Err(_) => {
                        service.child = None;
                        service.status.pid = None;
                        true
                    }
                },
                None => true,
            },
            None => false,
        };

        if !can_start {
            return;
        }
        // Started by hand (menu/palette/MCP): it joins the run from now on.
        if let Some(service) = self.services.get_mut(name) {
            service.deselected = false;
        }

        let (command, dir, port, extra_env, last_log, lock_file, ready_re) =
            match self.services.get(name) {
                Some(service) => (
                    service.command.clone(),
                    service.dir.clone(),
                    service.status.port,
                    service.env.clone(),
                    Arc::clone(&service.last_log),
                    service.lock_file.clone(),
                    service.sched.ready_log.clone(),
                ),
                None => return,
            };
        // Externally managed (no command) — state was already set by wait_for_port.
        // Only set Starting if never probed (Stopped) or switched off (Disabled):
        // check_health then settles it from its port.
        let Some(command) = command else {
            if let Some(svc) = self.services.get(name)
                && matches!(
                    svc.status.state,
                    ServiceState::Stopped | ServiceState::Disabled
                )
            {
                self.set_state_if_changed(name, ServiceState::Starting);
            }
            return;
        };
        if command.trim().is_empty() {
            self.set_state_if_changed(name, ServiceState::Failed);
            return;
        }

        // Fresh readiness flag per spawn: a restarted service must log "ready" again.
        let log_ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(svc) = self.services.get_mut(name) {
            svc.sched.log_ready = Arc::clone(&log_ready);
            svc.sched.spawned = true;
            svc.sched.spawned_at = Some(std::time::Instant::now());
            // A restart goes Starting → Starting: the wait clock restarts too.
            svc.sched.starting_since = Some(std::time::Instant::now());
            svc.sched.start_warned = false;
            svc.sched.completed_ok = false;
            svc.sched.fail_reason = None;
            svc.exit_code = None;
        }

        self.set_state_if_changed(name, ServiceState::Starting);

        if let Some(lock) = &lock_file {
            crate::runtime::envfile::cleanup_lock(&self.root_dir, lock).await;
        }

        let mut cmd = crate::runtime::process::task_command(&command);
        let work_dir = dir
            .as_ref()
            .map(|d| self.root_dir.join(d))
            .unwrap_or_else(|| self.root_dir.clone());
        cmd.current_dir(&work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Env precedence (low→high): login shell < env_files < service `env:` <
        // atlas-injected (PORT, FORCE_COLOR, BROWSER below).
        cmd.envs(atlas_core::login_env::vars());
        cmd.envs(crate::runtime::envfile::layered_env(
            &self.file_env,
            &extra_env,
        ));

        // Inject the configured port so services that default to their own port
        // (e.g. `process.env.PORT || 3001`) respect atlas.yaml.
        if let Some(p) = port {
            cmd.env("PORT", p.to_string());
        }

        // Prevent interactive TTY detection in node/bun children — avoids
        // SIGTTOU when vite/bun tries to write to a terminal it doesn't own.
        cmd.env("FORCE_COLOR", "1")
            .env_remove("NO_COLOR")
            .env("BROWSER", "none");

        // Inject docker socket for commands that use docker/docker compose
        if command.starts_with("docker") {
            crate::runtime::docker::apply_docker_env(&mut cmd);
        }

        let result = cmd.spawn();

        match result {
            Ok(mut child) => {
                let pid = child.id();
                if let Some(service) = self.services.get_mut(name) {
                    service.status.pid = pid;
                }

                // Disk log: rotate on (re)start; stdout/stderr readers each own an
                // append handle so no lock is shared across tasks or services.
                let (mut log_out, mut log_err) =
                    super::disk_log::open_service_writers(&self.log_dir, name);

                let _ = self.event_tx.send(ManagerEvent::ServiceRestarted {
                    name: name.to_string(),
                });
                let mut pumps = Vec::with_capacity(2);
                if let Some(stdout) = child.stdout.take() {
                    let tx = self.event_tx.clone();
                    let service_name = name.to_string();
                    let last_log_stdout = Arc::clone(&last_log);
                    let ready = ready_re.clone();
                    let flag = Arc::clone(&log_ready);
                    pumps.push(spawn_line_pump(stdout, move |line| {
                        if let Some(w) = log_out.as_mut() {
                            w.write_line(LogStream::Stdout, &line);
                        }
                        detect_ready(&ready, &flag, &line, &service_name, &tx);
                        if let Ok(mut guard) = last_log_stdout.lock() {
                            *guard = line.clone();
                        }
                        let _ = tx.send(ManagerEvent::LogLine {
                            name: service_name.clone(),
                            line,
                            stream: LogStream::Stdout,
                        });
                    }));
                }

                if let Some(stderr) = child.stderr.take() {
                    let tx = self.event_tx.clone();
                    let service_name = name.to_string();
                    let last_log_stderr = Arc::clone(&last_log);
                    let ready = ready_re.clone();
                    let flag = Arc::clone(&log_ready);
                    pumps.push(spawn_line_pump(stderr, move |line| {
                        if let Some(w) = log_err.as_mut() {
                            w.write_line(LogStream::Stderr, &line);
                        }
                        detect_ready(&ready, &flag, &line, &service_name, &tx);
                        if let Ok(mut guard) = last_log_stderr.lock() {
                            *guard = line.clone();
                        }
                        let _ = tx.send(ManagerEvent::LogLine {
                            name: service_name.clone(),
                            line,
                            stream: LogStream::Stderr,
                        });
                    }));
                }

                if let Some(service) = self.services.get_mut(name) {
                    service.child = Some(child);
                    // The previous spawn's pumps (if it crashed on its own) are
                    // retired together with its watcher.
                    for old in std::mem::replace(&mut service.pumps, pumps) {
                        old.abort();
                    }
                    service.exited_ok = false;
                    // Retire the previous spawn's watcher (it may still be running
                    // if the service crashed on its own), then give this spawn a
                    // fresh flag so the old watcher can never be revived.
                    service
                        .stopped
                        .store(true, std::sync::atomic::Ordering::Release);
                    service.stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
                }

                // Per-service health watcher — runs independently of the periodic
                // tick. Polls at 500ms while Starting, 3s once Running.
                let tx_watch = self.event_tx.clone();
                let svc_name = name.to_string();
                let health_url = self
                    .services
                    .get(name)
                    .and_then(|s| s.status.health_url.clone());
                let watch_port = self.services.get(name).and_then(|s| s.status.port);
                let log_driven = ready_re.is_some();
                // This spawn's stop flag: the watcher bails out when it is set.
                let stopped = self
                    .services
                    .get(name)
                    .map(|s| Arc::clone(&s.stopped))
                    .unwrap_or_default();
                tokio::spawn(async move {
                    let mut interval = WATCH_STARTUP_INTERVAL;
                    let mut ever_healthy = false;
                    let mut fails = 0u32;
                    let mut last: Option<ServiceState> = None;
                    loop {
                        tokio::time::sleep(interval).await;
                        // Service was stopped intentionally — don't emit any state.
                        if stopped.load(std::sync::atomic::Ordering::Acquire) {
                            break;
                        }
                        if log_driven {
                            // `ready_log` is this service's readiness signal: the log
                            // reader emits Running; exit detection is check_health's job.
                            break;
                        }
                        let Some(healthy) = crate::runtime::health::probe(
                            health_url.as_deref(),
                            watch_port,
                            WATCH_PROBE_TIMEOUT,
                        )
                        .await
                        else {
                            // No probe available: alive for MIN_UPTIME = running.
                            // A crash inside the window is decided by check_health,
                            // which also retires this watcher via `stopped`.
                            tokio::time::sleep(MIN_UPTIME).await;
                            if !stopped.load(std::sync::atomic::Ordering::Acquire) {
                                let _ = tx_watch.send(ManagerEvent::StateChanged {
                                    name: svc_name.clone(),
                                    state: ServiceState::Running,
                                });
                            }
                            break;
                        };
                        if healthy {
                            ever_healthy = true;
                            fails = 0;
                            interval = WATCH_HEALTHY_INTERVAL;
                        } else {
                            fails += 1;
                            // Startup polls fast; once it was healthy, space the
                            // retries so 3 misses span ~6s (a hot restart fits).
                            interval = if ever_healthy {
                                WATCH_RETRY_INTERVAL
                            } else {
                                WATCH_STARTUP_INTERVAL
                            };
                        }
                        // No Failed before the first healthy probe (startup grace),
                        // and only after PROBE_FAILS_TO_FAIL misses in a row: dev
                        // servers close their port while they hot-restart.
                        let next_state = if healthy {
                            ServiceState::Running
                        } else if ever_healthy && fails >= PROBE_FAILS_TO_FAIL {
                            ServiceState::Failed
                        } else {
                            continue;
                        };
                        // Emit transitions only: re-sending Running every tick floods
                        // the runner log, subscribers and the desktop.
                        if last.as_ref() == Some(&next_state) {
                            continue;
                        }
                        last = Some(next_state.clone());
                        if tx_watch
                            .send(ManagerEvent::StateChanged {
                                name: svc_name.clone(),
                                state: next_state,
                            })
                            .is_err()
                        {
                            break; // channel closed, manager shut down
                        }
                    }
                });
                self.sync_pid_file();
            }
            Err(_) => self.set_state_if_changed(name, ServiceState::Failed),
        }
    }

    pub(crate) async fn stop_service(&mut self, name: &str) {
        self.stop_services(&[name.to_string()]).await;
    }

    /// Stop a service and mark it Disabled so health checks skip it.
    pub async fn stop_service_disabled(&mut self, name: &str) {
        self.stop_service(name).await;
        self.set_state_if_changed(name, ServiceState::Disabled);
    }

    /// Kill any leftover processes from a previous atlas run.
    pub(crate) async fn kill_orphans(&mut self) {
        let locks: Vec<String> = self
            .services
            .values()
            .filter_map(|s| s.lock_file.clone())
            .collect();
        for lock in &locks {
            crate::runtime::envfile::cleanup_lock(&self.root_dir, lock).await;
        }
        let pid_path = crate::runtime::pid_file(&self.root_dir);
        if let Ok(contents) = tokio::fs::read_to_string(&pid_path).await {
            let our_pid = std::process::id() as i32;
            let our_ppid = atlas_core::sys::parent_pid() as i32;

            let pids: Vec<i32> = contents
                .lines()
                .filter_map(|l| l.trim().parse::<i32>().ok())
                // Never kill ourselves, our parent, or system processes
                .filter(|&pid| pid > 1 && pid != our_pid && pid != our_ppid)
                // Safety check: only kill processes that look like service
                // runtimes (bun, node, turbo, sh, python, deno …).
                // This prevents a recycled PID from wiping fish/OrbStack/etc.
                .filter(|&pid| crate::runtime::process::pid_is_service_runtime(pid))
                .collect();

            // Verified roots only; their descendants (turbo tasks in new process
            // groups) are collected from one process-table snapshot. Never by port.
            crate::runtime::process::kill_stale_tree(&pids, Duration::from_millis(300)).await;
            let _ = tokio::fs::remove_file(&pid_path).await;
        }
    }

    /// Rewrite .logs/.pids with every currently-running child PID.
    /// Called at the end of every start/stop so orphan cleanup is always current.
    pub(crate) fn sync_pid_file(&self) {
        let pid_path = crate::runtime::pid_file(&self.root_dir);
        let pids: Vec<String> = self
            .order
            .iter()
            .filter_map(|n| {
                self.services
                    .get(n)
                    .and_then(|s| s.status.pid)
                    .map(|p| p.to_string())
            })
            .collect();
        let _ = std::fs::create_dir_all(&self.log_dir);
        if pids.is_empty() {
            let _ = std::fs::remove_file(&pid_path);
        } else {
            let _ = std::fs::write(&pid_path, pids.join("\n"));
        }
    }
}

/// Mark `flag` and tell the manager once when `line` matches the service's `ready_log`.
fn detect_ready(
    re: &Option<regex::Regex>,
    flag: &std::sync::atomic::AtomicBool,
    line: &str,
    name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<ManagerEvent>,
) {
    use std::sync::atomic::Ordering;
    if let Some(re) = re
        && !flag.load(Ordering::Acquire)
        && re.is_match(&crate::runtime::text::strip_ansi(line))
    {
        flag.store(true, Ordering::Release);
        let _ = tx.send(ManagerEvent::LogReady {
            name: name.to_string(),
        });
    }
}
