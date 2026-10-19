//! The headless run itself: singleton lock, signals, dependency step, the
//! single loop that owns the manager, and what it prints per event.

use std::path::Path;
use std::time::Duration;

use crate::sig::Signal;
use anyhow::Result;
use tokio::signal;
use tokio::sync::mpsc;

use super::{Hub, RunKind};
use crate::config::{self, ProjectConfig};
use crate::control::{ControlRequest, ControlServer};
use crate::runtime::SingletonLock;
use crate::runtime::deps::{DepsOutcome, ensure_deps};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

/// Refuse to start when another atlas owns the project (the CLI prints the error and exits 1).
pub(crate) fn lock_or_err(root_dir: &Path) -> Result<SingletonLock> {
    let log_dir = crate::runtime::log_dir(root_dir);
    let lock_path = log_dir.join(".tui.lock");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| anyhow::anyhow!("atlas dev: cannot create {}: {e}", log_dir.display()))?;
    SingletonLock::acquire(&lock_path).map_err(|_| match SingletonLock::existing_pid(&lock_path) {
        Some(pid) => {
            anyhow::anyhow!("atlas dev is already running (pid {pid}); use 'atlas dev --status'")
        }
        None => anyhow::anyhow!("atlas dev is already running; use 'atlas dev --status'"),
    })
}

pub(crate) fn term_signals() -> Result<(Signal, Signal)> {
    Ok((crate::sig::terminate()?, crate::sig::hangup()?))
}

/// Same `pre_command` staleness logic as the TUI, but before any service starts.
async fn run_deps(root_dir: &Path, cfg: &ProjectConfig) {
    match ensure_deps(
        root_dir,
        cfg.pre_command.as_deref(),
        cfg.pre_command_when,
        false,
    )
    .await
    {
        DepsOutcome::UpToDate => {}
        DepsOutcome::Installed => eprintln!("\x1b[32m✓\x1b[0m dependencies installed"),
        DepsOutcome::Failed(reason) => {
            eprintln!("\x1b[31m✗\x1b[0m pre_command failed: {reason} (continuing)")
        }
    }
}

/// The headless engine's state: services, health, control socket, status heartbeat,
/// disk logs, and the attach stream. Built by `RunnerLoop::start`, consumed by `run`.
struct RunnerLoop {
    cfg: ProjectConfig,
    foreground: bool,
    manager: ServiceManager,
    hub: Hub,
    event_rx: mpsc::UnboundedReceiver<ManagerEvent>,
    control_rx: Option<mpsc::UnboundedReceiver<ControlRequest>>,
    sigterm: Signal,
    sighup: Signal,
    /// Watches the docker daemon and alerts when it dies or recovers; aborted on shutdown.
    watchdog: tokio::task::JoinHandle<()>,
}

impl RunnerLoop {
    /// Print the banner, build the manager and start every selected service.
    async fn start(
        root_dir: &Path,
        cfg: ProjectConfig,
        selection: &config::Selection,
        foreground: bool,
        control_rx: Option<mpsc::UnboundedReceiver<ControlRequest>>,
    ) -> Result<Self> {
        println!(
            "\x1b[1;34m●\x1b[0m atlas dev ({})",
            if foreground { "headless" } else { "runner" }
        );
        println!("  project: {}", cfg.name);
        match selection.names() {
            Some(names) => println!(
                "  services: {} of {} ({})",
                names.len(),
                cfg.services.len(),
                names
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => println!("  services: {}", cfg.services.len()),
        }
        println!();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut manager =
            ServiceManager::with_selection(root_dir.to_path_buf(), &cfg, event_tx, selection);
        manager.set_mode("runner");
        let hub = Hub::new(&cfg.name);
        let (sigterm, sighup) = term_signals()?;

        run_deps(root_dir, &cfg).await;

        manager.start_all().await;

        let watchdog = tokio::spawn(crate::runtime::docker::watch_daemon(
            manager.event_tx.clone(),
        ));

        if foreground {
            println!("\x1b[1;32m✓\x1b[0m Starting services. Press Ctrl+C to stop.");
        } else {
            println!("\x1b[1;32m✓\x1b[0m Starting services. Stop with: atlas down");
        }
        println!();

        Ok(Self {
            cfg,
            foreground,
            manager,
            hub,
            event_rx,
            control_rx,
            sigterm,
            sighup,
            watchdog,
        })
    }

    /// Single loop owns the manager: forwards logs and, on InfraReady, starts
    /// the app services exactly like the TUI effect loop does. The 1s tick runs
    /// check_health (heartbeat for `--status`, reaping of exited children).
    async fn run(mut self) -> Result<()> {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = signal::ctrl_c(), if self.foreground => break,
                _ = self.sigterm.recv() => break,
                _ = self.sighup.recv() => break,
                Some(req) = async { self.control_rx.as_mut().unwrap().recv().await }, if self.control_rx.is_some() => {
                    if self.hub.handle(&mut self.manager, &self.cfg, req).await {
                        break;
                    }
                }
                _ = tick.tick() => {
                    self.manager.check_health().await;
                    self.hub.tick(&mut self.manager);
                }
                Some(event) = self.event_rx.recv() => {
                    self.hub.on_event(&self.manager, &event);
                    handle_event(&mut self.manager, event, self.foreground).await;
                }
            }
        }

        println!();
        println!("\x1b[33m●\x1b[0m Shutting down...");

        self.watchdog.abort();
        self.hub.bye();
        self.manager.stop_all().await;

        println!("\x1b[32m✓\x1b[0m Done.");
        Ok(())
    }
}

/// The headless engine: services, health, control socket, status heartbeat, disk
/// logs, and the attach stream. Runs until SIGTERM/SIGHUP/Ctrl-C or `shutdown`.
pub async fn run(root_dir: &Path, select: &config::SelectionRequest, kind: RunKind) -> Result<()> {
    let cfg = config::load(root_dir)?;
    let selection = config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;
    // Held for the whole run: the singleton lock and the control socket.
    let _lock = lock_or_err(root_dir)?;
    let (_control, control_rx) = match ControlServer::bind(root_dir) {
        Ok((s, rx)) => (Some(s), Some(rx)),
        Err(_) => (None, None),
    };
    RunnerLoop::start(
        root_dir,
        cfg,
        &selection,
        kind == RunKind::Foreground,
        control_rx,
    )
    .await?
    .run()
    .await
}

/// What the runner does with one manager event (printing + the scheduler hooks the
/// TUI effect loop would otherwise run).
async fn handle_event(manager: &mut ServiceManager, event: ManagerEvent, foreground: bool) {
    match event {
        ManagerEvent::InfraReady { app_names } => {
            manager.start_app_services(&app_names).await;
        }
        // Action output is printed from ActionProgress below.
        ManagerEvent::LogLine { name, .. } if name.starts_with("action:") => {}
        // Service output has its own disk log; only a foreground run echoes it.
        ManagerEvent::LogLine { name, line, .. } => {
            if foreground {
                let prefix: String = name.chars().take(5).collect();
                println!("[{prefix:<5}] {line}");
            }
        }
        ManagerEvent::StateChanged { name, state } => {
            let icon = match state {
                ServiceState::Running => "\x1b[32m●\x1b[0m",
                ServiceState::Starting => "\x1b[33m◌\x1b[0m",
                ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                ServiceState::Stopped => "\x1b[90m○\x1b[0m",
                ServiceState::Disabled => "\x1b[90m⊘\x1b[0m",
                ServiceState::Suspended => "\x1b[90m⏸\x1b[0m",
            };
            eprintln!("{icon} {name}: {state:?}");
            manager.update_state(&name, state);
        }
        ManagerEvent::CriticalAlert { message, .. } => {
            eprintln!("  [!] {message}");
        }
        ManagerEvent::ServiceNote { name, message } => {
            eprintln!("\x1b[36m↻\x1b[0m {name}: {message}");
        }
        ManagerEvent::InfraFailed { names, reason } => {
            eprintln!("\x1b[31m✗\x1b[0m infra {}: {reason}", names.join(", "));
            manager.record_failure(&names, &reason);
        }
        ManagerEvent::LogReady { name } => manager.mark_log_ready(&name),
        ManagerEvent::TunnelState { state } => {
            eprintln!("\x1b[36m⇄\x1b[0m tunnel: {state:?}");
            manager.write_status_file();
        }
        ManagerEvent::ActionProgress { label, line } => {
            println!("[action {label}] {line}");
        }
        ManagerEvent::ActionCompleted { label, ok, resume } => {
            let (icon, word) = if ok {
                ("\x1b[32m✓\x1b[0m", "done")
            } else {
                ("\x1b[31m✗\x1b[0m", "failed")
            };
            eprintln!("{icon} action {label}: {word}");
            // Same as the TUI: services paused for the action come back afterwards.
            for name in resume {
                manager.restart_service(&name).await;
            }
        }
        _ => {}
    }
}
