use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tokio::signal;
use tokio::sync::mpsc;

use crate::config;
use crate::runner::{lock_or_err, term_signals};
use crate::runtime::deps::{DepsOutcome, ensure_deps};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

/// What a run does with the `pre_command` result: `--ci` aborts on failure
/// (returns the reason), headless logs it and carries on.
fn deps_failure(outcome: &DepsOutcome) -> Option<String> {
    match outcome {
        DepsOutcome::Failed(reason) => Some(format!("pre_command failed: {reason}")),
        _ => None,
    }
}

pub async fn run_ci(
    root_dir: &Path,
    timeout_secs: u64,
    select: &config::SelectionRequest,
) -> Result<()> {
    let cfg = config::load(root_dir)?;
    let selection = config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;
    let _lock = lock_or_err(root_dir)?;
    eprintln!(
        "atlas dev --ci  project={}  services={}",
        cfg.name,
        selection.names().map_or(cfg.services.len(), |n| n.len())
    );

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut manager =
        ServiceManager::with_selection(root_dir.to_path_buf(), &cfg, event_tx, &selection);
    let (mut sigterm, mut sighup) = term_signals()?;

    // A failing pre_command fails the run before anything is started.
    let deps = ensure_deps(
        root_dir,
        cfg.pre_command.as_deref(),
        cfg.pre_command_when,
        false,
    )
    .await;
    if let Some(reason) = deps_failure(&deps) {
        eprintln!("[atlas] {reason}");
        let output = serde_json::json!({ "ok": false, "reason": reason, "services": [] });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
        std::process::exit(1);
    }

    manager.start_all().await;

    // Wait at least 10 s for services to start before checking stability,
    // then keep polling until no service is Starting OR deadline is hit.
    let min_wait_until = tokio::time::Instant::now() + Duration::from_secs(10);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    // App services only start after InfraReady; never declare stable before.
    let mut apps_started = false;
    let mut interrupted = false;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => { interrupted = true; break; }
            _ = sigterm.recv() => { interrupted = true; break; }
            _ = sighup.recv() => { interrupted = true; break; }
            _ = tick.tick() => {}
        }
        while let Ok(event) = event_rx.try_recv() {
            match event {
                ManagerEvent::InfraReady { app_names } => {
                    manager.start_app_services(&app_names).await;
                    apps_started = true;
                }
                ManagerEvent::StateChanged { name, state } => {
                    eprintln!("  {name}: {state:?}");
                    manager.update_state(&name, state);
                }
                ManagerEvent::CriticalAlert { message, .. } => {
                    eprintln!("  [!] {message}");
                }
                ManagerEvent::ServiceNote { name, message } => {
                    eprintln!("  {name}: {message}");
                }
                ManagerEvent::LogReady { name } => manager.mark_log_ready(&name),
                ManagerEvent::TunnelState { state } => {
                    eprintln!("  tunnel: {state:?}");
                    manager.write_status_file();
                }
                _ => {}
            }
        }
        manager.check_health().await;
        let now = tokio::time::Instant::now();
        let any_starting = manager
            .services()
            .iter()
            .any(|s| s.state == ServiceState::Starting);
        if now >= deadline {
            eprintln!("[atlas] timeout after {timeout_secs}s");
            break;
        }
        // Only exit early once the minimum stabilization window has passed and
        // the scheduler is idle (nothing waiting on a dependency or a restart).
        if apps_started && !any_starting && !manager.scheduling_pending() && now >= min_wait_until {
            break;
        }
    }

    if interrupted {
        eprintln!("[atlas] interrupted, stopping services");
        manager.stop_all().await;
        std::process::exit(130);
    }

    let statuses = manager.services();
    let ok = statuses.iter().all(|s| {
        matches!(
            s.state,
            ServiceState::Running | ServiceState::Stopped | ServiceState::Disabled
        )
    });

    // A one-shot that exited 0 reports `stopped` + `completed: true` and counts as
    // OK; any Failed service (crashed, gave up, dependency failed) makes ok=false
    // and carries its `reason`.
    let arr: Vec<serde_json::Value> = statuses
        .iter()
        .map(|s| {
            let mut v = serde_json::json!({
                "name": s.name,
                "state": format!("{:?}", s.state).to_lowercase(),
                "port": s.port,
            });
            if manager.is_deselected(&s.name) {
                v["off"] = true.into();
            }
            if manager.completed_ok(&s.name) {
                v["completed"] = true.into();
            }
            if let Some(code) = manager.exit_code(&s.name) {
                v["exit_code"] = code.into();
            }
            if manager.restart_count(&s.name) > 0 {
                v["restarts"] = manager.restart_count(&s.name).into();
            }
            if s.state == ServiceState::Failed
                && let Some(reason) = manager.fail_reason(&s.name)
            {
                v["reason"] = reason.into();
            }
            v
        })
        .collect();
    let output = serde_json::json!({ "ok": ok, "services": arr });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_default()
    );

    manager.stop_all().await;
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headless_requires_config() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(config::load(dir.path()).is_err());
    }

    #[test]
    fn deps_failure_only_for_failed_outcome() {
        assert_eq!(deps_failure(&DepsOutcome::UpToDate), None);
        assert_eq!(deps_failure(&DepsOutcome::Installed), None);
        assert_eq!(
            deps_failure(&DepsOutcome::Failed("exit 2: nope".into())).as_deref(),
            Some("pre_command failed: exit 2: nope")
        );
    }
}
