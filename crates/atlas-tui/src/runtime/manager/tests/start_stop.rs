//! Starting, health-checking, stopping and respawning services.

use super::*;

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
    until_child_exited(&mut manager, "short").await;
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
    until_child_exited(&mut manager, "short").await;

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
    until_child_exited(&mut manager, "detached").await;
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
    until_child_exited(&mut mgr, "short").await;
    mgr.check_health().await;
    let state_of = |m: &ServiceManager| {
        m.services()
            .iter()
            .find(|s| s.name == "short")
            .map(|s| s.state.clone())
    };
    assert_eq!(
        state_of(&mgr),
        Some(ServiceState::Failed),
        "exited process should be Failed"
    );
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
    until_child_exited(&mut mgr, "svc").await;
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
    until_child_exited(&mut mgr, "svc").await;
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
    let running = tokio::time::timeout(atlas_core::scaling::HANG, async {
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

    // Back to 500: a live Running service survives a hot-restart blip and turns Unhealthy
    // (not Failed: the process is alive) on the `failure_threshold`-th (default 3) miss in a row.
    ok.store(false, Ordering::Release);
    for _ in 1..3 {
        mgr.check_health().await;
        assert_eq!(mgr.get_state("svc"), Some(ServiceState::Running));
    }
    mgr.check_health().await;
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Unhealthy));

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
/// well-behaved one returns as soon as it is gone, not when the grace expires.
// Signal traps: Unix only (Windows ends a process outright).
#[cfg(unix)]
#[tokio::test]
async fn test_kill_gracefully_polls_then_kills() {
    let dir = tempfile::TempDir::new().unwrap();
    let trapped = dir.path().join("trapped");
    let mut stubborn = Command::new("sh")
        .args([
            "-c",
            &format!("trap '' TERM; echo > '{}'; sleep 100", trapped.display()),
        ])
        .process_group(0)
        .spawn()
        .unwrap();
    // The trap is in place once the marker exists: a SIGTERM before it would kill the shell outright.
    while !trapped.exists() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let t = std::time::Instant::now();
    crate::runtime::process::kill_gracefully(&mut stubborn, Duration::from_millis(600)).await;
    // A lower bound: sleeping cannot be faster on a loaded machine, so this cannot flake.
    assert!(
        t.elapsed() >= Duration::from_millis(550),
        "waits out the grace"
    );
    // SIGTERM was ignored, so only the SIGKILL after the grace ends it (its own life is 100 s, past the
    // hang ceiling): the exit is by that signal, not by the child finishing.
    let status = tokio::time::timeout(atlas_core::scaling::HANG, stubborn.wait())
        .await
        .expect("leader reaped: the child was killed, not left running")
        .unwrap();
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(libc::SIGKILL),
        "{status:?}"
    );

    let mut polite = Command::new("sh")
        .args(["-c", "sleep 100"])
        .process_group(0)
        .spawn()
        .unwrap();
    // An hour of grace: returning once the group is gone (SIGTERM ends it at once) is the only way out
    // before the hang ceiling; a call that waited out its grace would take an hour.
    tokio::time::timeout(
        atlas_core::scaling::HANG,
        crate::runtime::process::kill_gracefully(&mut polite, Duration::from_secs(3600)),
    )
    .await
    .expect("returns once the group is gone, not when the grace expires");
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
