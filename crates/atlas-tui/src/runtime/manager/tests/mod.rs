use std::time::Duration;

use super::actions::{ActionGuard, end_action, run_action_streaming};
use super::graph::resolve_order;
use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::runtime::logs::LogStream;
use crate::runtime::service::ServiceState;

mod action_runs;
mod infra_start;
#[cfg(unix)]
mod login_path;
mod port_conflict;
mod session_env;
mod start_stop;

fn test_config() -> ProjectConfig {
    let mut services = HashMap::new();
    services.insert(
        "db".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some("docker compose up postgres".to_string()),
            dir: None,
            port: Some(5432),
            health: None,
            critical: Some(true),
            depends_on: None,
            enabled: None,
            hide_logs: false,
            lock_file: None,
            open_url: None,
            critical_help: None,
            ..Default::default()
        },
    );
    services.insert(
        "api".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some("cargo run --bin api".to_string()),
            dir: None,
            port: Some(3000),
            health: Some("http://localhost:3000/health".to_string()),
            critical: Some(true),
            depends_on: Some(crate::config::DependsOn::List(vec!["db".to_string()])),
            enabled: None,
            hide_logs: false,
            lock_file: None,
            open_url: None,
            critical_help: None,
            ..Default::default()
        },
    );
    services.insert(
        "web".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some("bun run dev".to_string()),
            dir: None,
            port: Some(5173),
            health: None,
            critical: None,
            depends_on: Some(crate::config::DependsOn::List(vec!["api".to_string()])),
            enabled: None,
            hide_logs: false,
            lock_file: None,
            open_url: None,
            critical_help: None,
            ..Default::default()
        },
    );

    ProjectConfig {
        name: "test-project".to_string(),
        services,
        ..Default::default()
    }
}

fn one_service_cfg(command: &str, port: Option<u16>) -> ProjectConfig {
    let mut services = HashMap::new();
    services.insert(
        "svc".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some(crate::runtime::process::test_sh(command)),
            dir: None,
            port,
            health: None,
            critical: None,
            depends_on: None,
            enabled: None,
            hide_logs: false,
            lock_file: None,
            open_url: None,
            critical_help: None,
            ..Default::default()
        },
    );
    ProjectConfig {
        name: "t".to_string(),
        services,
        ..Default::default()
    }
}

fn yaml_cfg(yaml: &str) -> ProjectConfig {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
    crate::config::load(dir.path()).unwrap()
}

/// Wait (bounded) until a service's child has exited. Fixed sleeps raced slow process startup
/// (`cmd /d /s /c` on Windows CI saw `Starting` after 300 ms, run 39356068863). tokio caches the
/// exit status, so `check_health`'s own `try_wait` still sees it afterward.
pub(super) async fn until_child_exited(m: &mut ServiceManager, name: &str) {
    let deadline = std::time::Instant::now() + atlas_core::scaling::HANG;
    while std::time::Instant::now() < deadline {
        let running = m
            .services
            .get_mut(name)
            .and_then(|s| s.child.as_mut())
            .is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !running {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{name}: child still running after the hang ceiling");
}
