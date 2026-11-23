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

/// Environment variable that sets the least seconds `--ci` waits for services before it may call them stable.
const CI_MIN_WAIT_ENV: &str = "ATLAS_CI_MIN_WAIT_SECS";

/// How long `--ci` waits before it may declare the services stable: 10 s, so a service that starts and then dies
/// within a few seconds is seen dying. `value` is [`CI_MIN_WAIT_ENV`]; a number of seconds replaces the default (the
/// test suites of this repo, which start `sleep` loops, set it low), anything else leaves it.
fn min_wait(value: Option<&str>) -> Duration {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(Duration::from_secs(10), Duration::from_secs)
}

pub async fn run_ci(
    root_dir: &Path,
    timeout_secs: u64,
    select: &config::SelectionRequest,
) -> Result<()> {
    let (cfg, source) = config::load_or_detect(root_dir)?;
    if source == config::ConfigSource::Detected {
        eprintln!(
            "{}",
            config::detected_notice_in(&cfg, atlas_core::lang::DEFAULT)
        );
    }
    let selection = config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;
    let _lock = lock_or_err(root_dir)?;
    // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
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
        // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
        eprintln!("[atlas] {reason}");
        let output = serde_json::json!({ "ok": false, "reason": reason, "services": [] });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
        std::process::exit(1);
    }

    manager.start_all().await;

    // Wait at least `min_wait` (10 s by default) for services to start before checking stability,
    // then keep polling until no service is Starting OR deadline is hit.
    let min_wait_until = tokio::time::Instant::now()
        + min_wait(std::env::var(CI_MIN_WAIT_ENV).ok().as_deref());
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
                ManagerEvent::CriticalAlert { note, .. } => {
                    eprintln!("  [!] {}", note.message);
                }
                ManagerEvent::ServiceNote { name, note } => {
                    eprintln!("  {name}: {}", note.message);
                }
                ManagerEvent::InfraFailed { names, reason } => {
                    // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
                    eprintln!("  infra {}: {}", names.join(", "), reason.message);
                    manager.record_failure(&names, &reason);
                }
                ManagerEvent::LogReady { name } => manager.mark_log_ready(&name),
                ManagerEvent::TunnelState { state } => {
                    // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
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
            // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
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
        // i18n-ignore: `--ci` output is read by pipelines and agents, never translated
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
                "state": s.state.name(),
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
            if matches!(s.state, ServiceState::Unhealthy | ServiceState::Degraded)
                && let Some(reason) = manager.service_note(&s.name)
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
    fn the_ci_stabilization_window_is_ten_seconds_unless_a_number_overrides_it() {
        assert_eq!(min_wait(None), Duration::from_secs(10));
        assert_eq!(min_wait(Some("1")), Duration::from_secs(1));
        assert_eq!(min_wait(Some(" 0 ")), Duration::ZERO);
        // Not a number of seconds: the default, never a zero that would skip the window.
        assert_eq!(min_wait(Some("")), Duration::from_secs(10));
        assert_eq!(min_wait(Some("soon")), Duration::from_secs(10));
        assert_eq!(min_wait(Some("-3")), Duration::from_secs(10));
    }

    #[test]
    fn deps_failure_only_for_failed_outcome() {
        assert_eq!(deps_failure(&DepsOutcome::UpToDate), None);
        assert_eq!(deps_failure(&DepsOutcome::Installed), None);
        assert_eq!(
            deps_failure(&DepsOutcome::Failed(crate::runtime::note::Note::new(
                "deps.exit",
                serde_json::json!({"code": 2, "tail": "nope"})
            )))
            .as_deref(),
            Some("pre_command failed: exit 2: nope")
        );
    }
}
