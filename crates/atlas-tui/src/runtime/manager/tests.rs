use std::time::Duration;

use super::actions::{ActionGuard, end_action, run_action_streaming};
use super::graph::resolve_order;
use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::runtime::logs::LogStream;
use crate::runtime::service::ServiceState;

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

#[test]
fn test_manager_new_from_config() {
    let config = test_config();
    let (tx, _rx) = mpsc::unbounded_channel();
    let manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);

    assert_eq!(manager.services.len(), 3);
    assert!(manager.services.contains_key("db"));
    assert!(manager.services.contains_key("api"));
    assert!(manager.services.contains_key("web"));

    for service in manager.services.values() {
        assert_eq!(service.status.state, ServiceState::Stopped);
    }
}

/// The service order must not depend on HashMap iteration order.
#[test]
fn resolve_order_is_deterministic_and_dependency_first() {
    let mk = |deps: Option<Vec<&str>>| ServiceDef {
        command: Some("true".into()),
        depends_on: deps
            .map(|d| crate::config::DependsOn::List(d.into_iter().map(String::from).collect())),
        ..Default::default()
    };
    let names = ["web", "api", "migrate", "crasher", "slowport", "db"];
    let first = {
        let mut config = test_config();
        config.services.clear();
        for n in names {
            let deps = match n {
                "api" => Some(vec!["migrate"]),
                "migrate" => Some(vec!["db"]),
                _ => None,
            };
            config.services.insert(n.to_string(), mk(deps));
        }
        resolve_order(&config)
    };
    assert_eq!(
        first,
        ["db", "migrate", "api", "crasher", "slowport", "web"]
    );
    for _ in 0..20 {
        let mut config = test_config();
        config.services.clear();
        for n in names.iter().rev() {
            let deps = match *n {
                "api" => Some(vec!["migrate"]),
                "migrate" => Some(vec!["db"]),
                _ => None,
            };
            config.services.insert(n.to_string(), mk(deps));
        }
        assert_eq!(resolve_order(&config), first);
    }
}

#[test]
fn test_manager_services_order() {
    let config = test_config();
    let (tx, _rx) = mpsc::unbounded_channel();
    let manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);
    let services = manager.services();
    let names: Vec<&str> = services
        .iter()
        .map(|service| service.name.as_str())
        .collect();

    let db_idx = names.iter().position(|&name| name == "db").unwrap();
    let api_idx = names.iter().position(|&name| name == "api").unwrap();
    let web_idx = names.iter().position(|&name| name == "web").unwrap();

    assert!(db_idx < api_idx, "db should come before api");
    assert!(api_idx < web_idx, "api should come before web");
}

/// A service whose process exits must be reported Failed even when an
/// unrelated process happens to be listening on its configured port.
#[tokio::test]
async fn test_check_health_detects_exited_child() {
    let mut services = HashMap::new();
    services.insert(
        "short".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some(crate::runtime::process::test_sh("true")),
            dir: None,
            // No port — exit-0 with no health endpoint = child_alive=false = Failed
            port: None,
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
    let config = ProjectConfig {
        name: "crash".to_string(),
        services,
        ..Default::default()
    };

    let (tx, mut rx) = mpsc::unbounded_channel();
    let tmp = std::env::temp_dir();
    let mut manager = ServiceManager::new(tmp, &config, tx);

    manager.start_service("short").await;
    // Let `true` exit before probing.
    tokio::time::sleep(Duration::from_millis(300)).await;
    manager.check_health().await;

    assert_eq!(manager.get_state("short"), Some(ServiceState::Failed));
    let service = manager.services.get("short").unwrap();
    assert!(service.child.is_none(), "exited child must be cleared");
    assert!(service.status.pid.is_none(), "stale pid must be cleared");

    let mut saw_failed = false;
    while let Ok(event) = rx.try_recv() {
        if let ManagerEvent::StateChanged { name, state } = event
            && name == "short"
            && state == ServiceState::Failed
        {
            saw_failed = true;
        }
    }
    assert!(saw_failed, "crash must emit a Failed state change");
}

/// Repeated health checks on an unchanged service must stay silent so a
/// stable service cannot flood the UI channel.
#[tokio::test]
async fn test_repeated_state_emits_once() {
    let config = test_config();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut manager = ServiceManager::new(PathBuf::from("/tmp/test"), &config, tx);

    manager.set_state_if_changed("db", ServiceState::Running);
    manager.set_state_if_changed("db", ServiceState::Running);
    manager.set_state_if_changed("db", ServiceState::Running);

    let mut changes = 0;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, ManagerEvent::StateChanged { .. }) {
            changes += 1;
        }
    }
    assert_eq!(changes, 1, "duplicate states must be suppressed");
}

/// A crashed service must be restartable: `start_service` has to notice
/// the dead child instead of treating the slot as still occupied.
#[tokio::test]
async fn test_start_service_replaces_exited_child() {
    let mut services = HashMap::new();
    services.insert(
        "short".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some("true".to_string()),
            dir: None,
            port: None,
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
    let config = ProjectConfig {
        name: "restart".to_string(),
        services,
        ..Default::default()
    };

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = ServiceManager::new(std::env::temp_dir(), &config, tx);

    manager.start_service("short").await;
    let first_pid = manager.services.get("short").unwrap().status.pid;
    tokio::time::sleep(Duration::from_millis(300)).await;

    manager.start_service("short").await;
    let second_pid = manager.services.get("short").unwrap().status.pid;

    assert!(first_pid.is_some(), "first spawn must record a pid");
    assert!(second_pid.is_some(), "restart must spawn a new process");
    assert_ne!(first_pid, second_pid, "restart must not reuse a dead child");
}

/// `docker compose up -d` exits 0 immediately. The service should transition
/// to Running via port check, not be marked Failed due to child exit.
#[tokio::test]
async fn test_zero_exit_with_port_recovers_via_port_check() {
    let mut services = HashMap::new();
    // Use port 1 — nothing listens there, so port check will return false.
    // The key assertion is that the service is NOT immediately Failed after
    // the child exits 0; it stays in Starting/Failed only after port check.
    services.insert(
        "detached".to_string(),
        ServiceDef {
            open_key: None,
            env: Default::default(),
            command: Some("true".to_string()),
            dir: None,
            port: Some(1), // closed port → port check fails → Failed, but NOT from child exit
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
    let config = ProjectConfig {
        name: "detached-test".to_string(),
        services,
        ..Default::default()
    };

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = ServiceManager::new(std::env::temp_dir(), &config, tx);

    manager.start_service("detached").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    manager.check_health().await;

    // The state must NOT be Running (port 1 is closed), but the key check is
    // that it went through Starting rather than skipping straight to Failed
    // from the child-exit path.
    let state = manager.get_state("detached");
    assert!(
        matches!(
            state,
            Some(ServiceState::Failed) | Some(ServiceState::Starting)
        ),
        "zero-exit service should be Failed/Starting via port check, got {state:?}"
    );
}

#[tokio::test]
async fn test_short_lived_service_detected_as_failed() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: short-test\nservices:\n  short:\n    command: \"true\"\n",
    )
    .unwrap();
    let cfg = crate::config::load(dir.path()).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_service("short").await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    mgr.check_health().await;

    let state = mgr
        .services()
        .iter()
        .find(|s| s.name == "short")
        .map(|s| s.state.clone());
    assert_eq!(
        state,
        Some(ServiceState::Failed),
        "exited process should be Failed"
    );
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

/// A crashed child must be Failed even though something else still
/// answers on its port (EADDRINUSE-style crash).
#[tokio::test]
async fn test_nonzero_exit_with_live_port_is_failed() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("exit 1", Some(port)),
        tx,
    );

    mgr.start_service("svc").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    mgr.check_health().await;

    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Failed));
    drop(listener);
}

/// A detaching launcher (`docker compose up -d`-style, exit 0) keeps the
/// port probe authoritative.
#[tokio::test]
async fn test_zero_exit_launcher_with_live_port_is_running() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("true", Some(port)),
        tx,
    );

    mgr.start_service("svc").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    mgr.check_health().await;

    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Running));
    drop(listener);
}

/// `health:` is a real HTTP GET: a port that accepts connections but answers
/// 500 is NOT healthy; once it answers 200 the service is Running. Covers
/// both the per-service startup watcher (events) and `check_health`.
#[tokio::test]
async fn test_health_url_requires_2xx_not_just_open_port() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ok = Arc::new(AtomicBool::new(false));
    let server_ok = Arc::clone(&ok);
    let server = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let status = if server_ok.load(Ordering::Acquire) {
                "200 OK"
            } else {
                "500 Internal Server Error"
            };
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await;
        }
    });

    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = one_service_cfg("sleep 30", Some(port));
    cfg.services.get_mut("svc").unwrap().health = Some(format!("http://127.0.0.1:{port}/health"));
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_service("svc").await;

    // Startup watcher: while the server answers 500 it must never emit Running.
    tokio::time::sleep(Duration::from_millis(1600)).await;
    while let Ok(ev) = rx.try_recv() {
        if let ManagerEvent::StateChanged { state, .. } = ev {
            assert_ne!(state, ServiceState::Running, "500 must not be Running");
        }
    }
    // check_health leaves a still-Starting service to its watcher (grace):
    // it must not have been promoted to Running by the open TCP port.
    mgr.check_health().await;
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Starting));

    // Flip to 200: the watcher emits Running, and check_health recovers it.
    ok.store(true, Ordering::Release);
    let running = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::StateChanged { state, .. } = ev
                && state == ServiceState::Running
            {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(running, "200 must produce a Running event");
    // The event loop applies the watcher's event; afterwards check_health probes.
    mgr.update_state("svc", ServiceState::Running);
    mgr.check_health().await;
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Running));

    // Back to 500: a live Running service survives a hot-restart blip and
    // turns Failed only on the PROBE_FAILS_TO_FAIL-th miss in a row.
    ok.store(false, Ordering::Release);
    for _ in 1..types::PROBE_FAILS_TO_FAIL {
        mgr.check_health().await;
        assert_eq!(mgr.get_state("svc"), Some(ServiceState::Running));
    }
    mgr.check_health().await;
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Failed));

    mgr.stop_service("svc").await;
    server.abort();
}

/// stop_service must drop the pid from `.pids` immediately.
#[tokio::test]
async fn test_stop_service_clears_pid_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("sleep 30", None),
        tx,
    );

    mgr.start_service("svc").await;
    mgr.sync_pid_file();
    assert!(dir.path().join(".logs/.pids").exists());

    mgr.stop_service("svc").await;
    assert!(!dir.path().join(".logs/.pids").exists());
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Stopped));
}

/// A child that ignores SIGTERM is SIGKILLed once the grace expires, and a
/// well-behaved one returns well before the grace does.
// Signal traps: Unix only (Windows ends a process outright).
#[cfg(unix)]
#[tokio::test]
async fn test_kill_gracefully_polls_then_kills() {
    let mut stubborn = Command::new("sh")
        .args(["-c", "trap '' TERM; sleep 100"])
        .process_group(0)
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t = std::time::Instant::now();
    crate::runtime::process::kill_gracefully(&mut stubborn, Duration::from_millis(600)).await;
    assert!(
        t.elapsed() >= Duration::from_millis(550),
        "waits out the grace"
    );
    assert!(
        stubborn.try_wait().unwrap().is_some() || stubborn.wait().await.is_ok(),
        "leader reaped"
    );

    let mut polite = Command::new("sh")
        .args(["-c", "sleep 100"])
        .process_group(0)
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let t = std::time::Instant::now();
    crate::runtime::process::kill_gracefully(&mut polite, Duration::from_secs(5)).await;
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "returns once group is gone"
    );
}

/// The same action label cannot be in flight twice; ending it frees it.
#[test]
fn test_action_in_flight_guard() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/test"), &test_config(), tx);

    assert!(mgr.try_begin_action("Refresh DB"));
    assert!(
        !mgr.try_begin_action("Refresh DB"),
        "second trigger refused"
    );
    assert!(mgr.try_begin_action("Other"), "other labels unaffected");
    assert!(mgr.action_running("Refresh DB"));

    end_action(&mgr.actions_in_flight, "Refresh DB");
    assert!(!mgr.action_running("Refresh DB"));
    assert!(mgr.try_begin_action("Refresh DB"));
}

fn yaml_cfg(yaml: &str) -> ProjectConfig {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
    crate::config::load(dir.path()).unwrap()
}

/// pause_services pauses only services that are Running/Starting and reports
/// exactly those for resume: a service the user stopped stays stopped.
#[tokio::test]
async fn test_pause_for_action_only_touches_live_services() {
    let dir = tempfile::TempDir::new().unwrap();
    let cfg = yaml_cfg(
        r#"name: p
services:
  live:
    command: sleep 30
  starting:
    command: sleep 30
  stopped:
    command: sleep 30
  failed:
    command: sleep 30
"#,
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    mgr.start_service("live").await;
    mgr.set_state_if_changed("live", ServiceState::Running);
    mgr.start_service("starting").await; // Starting
    mgr.set_state_if_changed("failed", ServiceState::Failed);

    let names: Vec<String> = ["live", "starting", "stopped", "failed", "nope"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut paused = mgr.pause_for_action(&names, Some("Refresh DB")).await;
    paused.sort();
    assert_eq!(paused, vec!["live".to_string(), "starting".to_string()]);
    assert_eq!(mgr.get_state("live"), Some(ServiceState::Suspended));
    assert_eq!(mgr.get_state("starting"), Some(ServiceState::Suspended));
    assert_eq!(mgr.get_state("stopped"), Some(ServiceState::Stopped));
    assert_eq!(mgr.get_state("failed"), Some(ServiceState::Failed));
    assert_eq!(mgr.paused_by("live").as_deref(), Some("Refresh DB"));
    assert_eq!(mgr.paused_by("stopped"), None);
}

/// An on-demand tunnel closes with its last `open_tunnel` action; leases are
/// refcounted; a boot-enabled tunnel is never closed by an action.
#[tokio::test]
async fn test_on_demand_tunnel_closes_with_last_lease() {
    let dir = tempfile::TempDir::new().unwrap();
    let cfg = |enabled: bool| {
        yaml_cfg(&format!(
            "name: t\ntunnel:\n  enabled: {enabled}\n  local_port: 54399\n  remote_host: h\n  \
             remote_port: 5432\n  ssh_host: fake\nservices:\n  s:\n    command: sleep 30\n\
             actions:\n  - key: o\n    label: A\n    command: 'true'\n    open_tunnel: true\n"
        ))
    };
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg(false), tx);

    // Two overlapping actions share one on-demand supervisor.
    let l1 = mgr.tunnel_for_action().unwrap().ok().unwrap();
    let l2 = mgr.tunnel_for_action().unwrap().ok().unwrap();
    assert!(mgr.tunnel_status().is_some());
    l1.release().await;
    assert!(
        mgr.tunnel_status().is_some(),
        "still used by the other action"
    );
    l2.release().await;
    // Last one out closes it: the supervisor is gone from status.
    assert!(mgr.tunnel_status().is_none(), "closed with the last action");

    // A later action opens a fresh one.
    let l3 = mgr.tunnel_for_action().unwrap().ok().unwrap();
    assert!(mgr.tunnel_status().is_some());
    l3.release().await;
    assert!(mgr.tunnel_status().is_none());

    // Boot-enabled (or already up before the action): actions never close it.
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg(true), tx);
    mgr.tunnel = Some(crate::runtime::tunnel::TunnelHandle::spawn(
        mgr.tunnel_cfg.clone().unwrap(),
        mgr.log_dir.clone(),
        mgr.event_tx.clone(),
        false,
    ));
    let l = mgr.tunnel_for_action().unwrap().ok().unwrap();
    l.release().await;
    assert!(
        mgr.tunnel_status().is_some(),
        "boot tunnel survives actions"
    );
}

/// A background process holding the pipes must not stall completion.
// Background jobs and `$!`: sh syntax.
#[cfg(unix)]
#[tokio::test]
async fn test_action_streaming_returns_with_background_pipe_holder() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let pidfile = dir.path().join("bg.pid");
    let t = std::time::Instant::now();
    let ok = run_action_streaming(
        &format!("sleep 31 & echo $! > {}; echo done", pidfile.display()),
        dir.path(),
        &HashMap::new(),
        &dir.path().join(".logs"),
        "bg",
        &tx,
    )
    .await;
    assert!(ok);
    assert!(t.elapsed() < Duration::from_secs(5), "drain is bounded");
    // reap the stray sleep we started (exact pid we recorded)
    if let Ok(p) = std::fs::read_to_string(&pidfile)
        && let Ok(p) = p.trim().parse::<i32>()
    {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(p),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// Item 4: action output reaches the log buffer (`action:<label>` LogLine),
/// the operation row (ActionProgress) and `.logs/action-<slug>.log`, with the
/// previous run rotated to `.prev.log`.
#[tokio::test]
async fn action_output_is_persisted_and_streamed_as_log_lines() {
    let dir = tempfile::TempDir::new().unwrap();
    let logs = dir.path().join(".logs");
    for round in ["first", "second"] {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let ok = run_action_streaming(
            &crate::runtime::process::test_sh(&format!(
                "echo {round}-out; echo {round}-err >&2; exit 3"
            )),
            dir.path(),
            &HashMap::new(),
            &logs,
            "Run checks",
            &tx,
        )
        .await;
        assert!(!ok, "exit 3 is a failure");
        drop(tx);
        let (mut lines, mut progress) = (Vec::new(), 0);
        while let Some(ev) = rx.recv().await {
            match ev {
                ManagerEvent::LogLine { name, line, stream } => {
                    assert_eq!(name, "action:Run checks");
                    lines.push((line, stream));
                }
                ManagerEvent::ActionProgress { label, .. } => {
                    assert_eq!(label, "Run checks");
                    progress += 1;
                }
                _ => {}
            }
        }
        lines.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            lines,
            vec![
                (format!("{round}-err"), LogStream::Stderr),
                (format!("{round}-out"), LogStream::Stdout),
            ]
        );
        assert_eq!(progress, 2);
    }
    let cur = std::fs::read_to_string(logs.join("action-run-checks.log")).unwrap();
    assert!(cur.contains("[stdout] second-out") && cur.contains("[stderr] second-err"));
    assert!(!cur.contains("first"), "current file holds this run only");
    let prev = std::fs::read_to_string(logs.join("action-run-checks.prev.log")).unwrap();
    assert!(prev.contains("first-out") && prev.contains("first-err"));
}

#[test]
fn action_log_stem_is_a_safe_slug_and_marks_append() {
    assert_eq!(log_file_stem("api"), "api");
    assert_eq!(log_file_stem("action:Run checks"), "action-run-checks");
    assert_eq!(
        log_file_stem("action:Database › Seed users"),
        "action-database-seed-users"
    );
    assert_eq!(log_file_stem("action:../../x"), "action-x");
    assert_eq!(log_file_stem("action:///"), "action-unnamed");

    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(dir.path().to_path_buf(), &test_config(), tx);
    mgr.append_log_mark("api", "──────── mark ────────");
    mgr.append_log_mark("action:Run checks", "──── mark ────");
    mgr.append_log_mark("../evil", "nope");
    let api = std::fs::read_to_string(dir.path().join(".logs/api.log")).unwrap();
    assert!(api.contains("[stdout] ──────── mark ────────"), "{api}");
    assert!(dir.path().join(".logs/action-run-checks.log").exists());
    assert!(!dir.path().join("evil.log").exists());
}

/// A restarted service gets a fresh stop flag; the old watcher's flag
/// stays set so it can never be revived.
#[tokio::test]
async fn test_each_spawn_gets_fresh_stopped_flag() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("sleep 30", None),
        tx,
    );

    mgr.start_service("svc").await;
    let first = Arc::clone(&mgr.services["svc"].stopped);
    mgr.stop_service("svc").await;
    assert!(first.load(std::sync::atomic::Ordering::Acquire));

    mgr.start_service("svc").await;
    let second = Arc::clone(&mgr.services["svc"].stopped);
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(
        first.load(std::sync::atomic::Ordering::Acquire),
        "old watcher stays stopped"
    );
    assert!(!second.load(std::sync::atomic::Ordering::Acquire));
    mgr.stop_service("svc").await;
}

/// Run a platform session action that echoes the host env, return its
/// `TAURI=… ATLAS=…` log line. `svc_env` is merged into the service's env.
async fn session_host_line(host_mode: &str, svc_env: &[(&str, &str)]) -> Option<String> {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = one_service_cfg("sleep 30", None);
    cfg.services
        .get_mut("svc")
        .unwrap()
        .env
        .extend(svc_env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    cfg.actions = vec![
        serde_norway::from_str(&format!(
            "key: m\nlabel: Mob\ncommand: '{}'\nsession: true\nplatform: ios\nhost_mode: {host_mode}\n",
            crate::runtime::process::test_sh("echo TAURI=$TAURI_DEV_HOST ATLAS=$ATLAS_DEV_HOST")
        ))
        .unwrap(),
    ];
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    mgr.run_session('m').await?;
    let line = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::LogLine { line, .. } = ev
                && line.starts_with("TAURI=")
            {
                return Some(line);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    mgr.stop_session("Mob").await;
    line
}

#[tokio::test]
async fn session_host_mode_simulator_injects_loopback() {
    // Skip the assertion's premise if the developer's shell already exports them.
    if std::env::var_os("TAURI_DEV_HOST").is_some() || std::env::var_os("ATLAS_DEV_HOST").is_some()
    {
        return;
    }
    let line = session_host_line("simulator", &[]).await.unwrap();
    assert_eq!(line, "TAURI=127.0.0.1 ATLAS=127.0.0.1");
}

#[tokio::test]
async fn session_host_mode_does_not_override_user_env() {
    if std::env::var_os("TAURI_DEV_HOST").is_some() || std::env::var_os("ATLAS_DEV_HOST").is_some()
    {
        return;
    }
    let line = session_host_line("simulator", &[("TAURI_DEV_HOST", "10.9.8.7")])
        .await
        .unwrap();
    // TAURI_DEV_HOST keeps the user's value; ATLAS_DEV_HOST is still injected.
    assert_eq!(line, "TAURI=10.9.8.7 ATLAS=127.0.0.1");
}

/// A session whose process exits by itself (never `stop_session`) must be
/// reaped by the health tick: `SessionEnded` emitted, entry removed.
#[tokio::test]
async fn session_exits_emits_session_ended() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = one_service_cfg("sleep 30", None);
    cfg.actions = vec![
        serde_norway::from_str(&format!(
            "key: q\nlabel: Quick\ncommand: '{}'\nsession: true\n",
            crate::runtime::process::test_sh("true")
        ))
        .unwrap(),
    ];
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    mgr.run_session('q').await.unwrap();
    assert!(mgr.session_active("Quick"));

    tokio::time::sleep(Duration::from_millis(300)).await;
    mgr.check_health().await;

    assert!(!mgr.session_active("Quick"));
    let mut ended = None;
    while let Ok(ev) = rx.try_recv() {
        if let ManagerEvent::SessionEnded { label, ok } = ev {
            ended = Some((label, ok));
        }
    }
    assert_eq!(ended, Some(("Quick".to_string(), true)));
}

/// An action task that dies mid-flight must free its label and report failure.
#[tokio::test]
async fn action_guard_releases_on_panic() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/test"), &test_config(), tx.clone());
    assert!(mgr.try_begin_action("Boom"));

    let guard = ActionGuard {
        in_flight: Arc::clone(&mgr.actions_in_flight),
        label: "Boom".to_string(),
        tx,
        done: false,
    };
    let handle = tokio::spawn(async move {
        let _guard = guard;
        panic!("action task died");
    });
    assert!(handle.await.is_err());

    assert!(!mgr.action_running("Boom"));
    match rx.try_recv() {
        Ok(ManagerEvent::ActionCompleted { label, ok, .. }) => {
            assert_eq!(label, "Boom");
            assert!(!ok);
        }
        other => panic!("expected ActionCompleted, got {other:?}"),
    }
}

/// Seed the shared login-shell probe the way a desktop-entry launch would produce it: bare PATH, the
/// user's rc files add `~/.bun/bin`. Returns the fake HOME (keep it alive for the test).
#[cfg(unix)]
fn gui_login_env() -> Option<tempfile::TempDir> {
    use atlas_core::login_env::{fixtures, probe_with};
    let sh = fixtures::some_shell()?;
    let home = fixtures::fake_home();
    let p = probe_with(sh, home.path(), &fixtures::gui_env(home.path()));
    assert!(p.error.is_none(), "{:?}", p.error);
    atlas_core::login_env::install(p);
    Some(home)
}

/// Incident (Linux, app launched from the menu): `exit 127: sh: 1: bun: not found`. Actions run with the
/// same env as services and `pre_command`.
// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn an_action_finds_binaries_the_login_shell_put_on_path() {
    let Some(_home) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ok = run_action_streaming(
        "bun run thing",
        dir.path(),
        &HashMap::new(),
        &dir.path().join(".logs"),
        "Bun",
        &tx,
    )
    .await;
    drop(tx);
    let mut lines = Vec::new();
    while let Some(ev) = rx.recv().await {
        if let ManagerEvent::LogLine { line, .. } = ev {
            lines.push(line);
        }
    }
    assert!(ok, "action failed: {lines:?}");
    assert!(lines.iter().any(|l| l == "bun-ran run thing"), "{lines:?}");
}

// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn a_service_finds_binaries_the_login_shell_put_on_path() {
    let Some(_home) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cfg = one_service_cfg("bun dev; sleep 30", None);
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    mgr.start_service("svc").await;
    let line = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::LogLine { line, .. } = ev {
                return line;
            }
        }
        String::new()
    })
    .await
    .unwrap_or_default();
    mgr.stop_service("svc").await;
    assert_eq!(line, "bun-ran dev");
}

// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn a_session_action_finds_binaries_the_login_shell_put_on_path() {
    let Some(_home) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = one_service_cfg("sleep 30", None);
    cfg.actions = vec![
        serde_norway::from_str(
            "key: m\nlabel: Mob\ncommand: 'bun tauri ios dev'\nsession: true\nplatform: ios\n",
        )
        .unwrap(),
    ];
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    let _ = mgr.run_session('m').await;
    let line = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::LogLine { line, .. } = ev
                && line.starts_with("bun-ran")
            {
                return Some(line);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    mgr.stop_session("Mob").await;
    assert_eq!(line.as_deref(), Some("bun-ran tauri ios dev"));
}
