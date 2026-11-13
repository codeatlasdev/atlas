//! Liveness (`unhealthy`) and readiness (`degraded`) against REAL local HTTP servers
//! and real child processes: a server that accepts and never answers is the hung
//! SSR service of incident #132; one that answers 500 on `/ready` while `/health`
//! is 200 is the half-broken service of #136.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::*;
use crate::config::{OnUnhealthy, ProjectConfig, ReadyProbe, ServiceDef};
use crate::runtime::service::ServiceState;

/// 0 = accept and never answer (hung event loop); otherwise the status code to send.
/// `ready_status` answers `/ready`, `status` everything else.
struct Server {
    port: u16,
    status: Arc<AtomicU16>,
    ready_status: Arc<AtomicU16>,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(status: u16, ready_status: u16) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let status = Arc::new(AtomicU16::new(status));
        let ready_status = Arc::new(AtomicU16::new(ready_status));
        let hits = Arc::new(AtomicUsize::new(0));
        let (s, r, h) = (status.clone(), ready_status.clone(), hits.clone());
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let (s, r, h) = (s.clone(), r.clone(), h.clone());
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let code = if req.starts_with("GET /ready") {
                        r.load(Ordering::Acquire)
                    } else {
                        s.load(Ordering::Acquire)
                    };
                    h.fetch_add(1, Ordering::AcqRel);
                    if code == 0 {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        return;
                    }
                    let _ = sock
                        .write_all(
                            format!(
                                "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .await;
                });
            }
        });
        Self {
            port,
            status,
            ready_status,
            hits,
            task,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn cfg(command: &str, port: u16, tweak: impl FnOnce(&mut ServiceDef)) -> ProjectConfig {
    let mut def = ServiceDef {
        command: Some(crate::runtime::process::test_sh(command)),
        port: Some(port),
        health: Some(format!("http://127.0.0.1:{port}/health")),
        // Sub-second probes keep the tests quick without racing: the server answers at once or never.
        ..Default::default()
    };
    tweak(&mut def);
    ProjectConfig {
        name: "t".into(),
        services: std::collections::HashMap::from([("svc".to_string(), def)]),
        ..Default::default()
    }
}

fn manager(
    dir: &std::path::Path,
    cfg: &ProjectConfig,
) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (ServiceManager::new(dir.to_path_buf(), cfg, tx), rx)
}

/// Spawn the service, let the watcher/tick promote it once, like the event loop does.
async fn up(m: &mut ServiceManager) {
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;
    assert_eq!(
        m.get_state("svc"),
        Some(ServiceState::Running),
        "precondition: healthy"
    );
}

fn state(m: &ServiceManager) -> ServiceState {
    m.get_state("svc").unwrap()
}

/// Incident #132: the process lives, the event loop is blocked, status stayed `running`.
/// Acceptance: `unhealthy` (not `failed`) after `failure_threshold` misses, with the context.
#[tokio::test]
async fn a_hung_live_service_turns_unhealthy_not_failed_after_the_threshold() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("echo booting; sleep 30", srv.port, |d| {
            d.health_timeout = Some(1);
            d.failure_threshold = Some(2);
        }),
    );
    up(&mut m).await;

    srv.status.store(0, Ordering::Release); // hang
    m.check_health().await;
    assert_eq!(
        state(&m),
        ServiceState::Running,
        "1 miss < threshold 2: still grace"
    );
    let t0 = std::time::Instant::now();
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Unhealthy, "threshold reached");
    assert_ne!(state(&m), ServiceState::Failed);
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "bounded by timeout (1s), not by the OS connect/read default"
    );

    let rep = m.unhealthy_report("svc").expect("captured context");
    assert_eq!(rep.misses, 2);
    assert!(rep.since_epoch > 0);
    assert!(
        rep.lines.iter().any(|l| l.contains("booting")),
        "the last log lines at the moment of the hang: {:?}",
        rep.lines
    );
    let note = m.service_note("svc").unwrap();
    assert!(note.contains("unhealthy") && note.contains("2"), "{note}");
    m.stop_service("svc").await;
}

#[tokio::test]
async fn the_hang_clears_when_the_service_answers_again() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.health_timeout = Some(1);
            d.failure_threshold = Some(1);
        }),
    );
    up(&mut m).await;
    srv.status.store(0, Ordering::Release);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Unhealthy);

    srv.status.store(200, Ordering::Release);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running);
    assert!(
        m.unhealthy_report("svc").is_none(),
        "report cleared on recovery"
    );
    assert!(
        m.service_note("svc")
            .is_none_or(|n| !n.contains("unhealthy"))
    );
    m.stop_service("svc").await;
}

/// A dead process is still `failed`: `unhealthy` is only for a LIVE child.
#[tokio::test]
async fn a_dead_child_is_failed_never_unhealthy() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(dir.path(), &cfg("exit 1", srv.port, |_| {}));
    m.start_service("svc").await;
    super::tests::until_child_exited(&mut m, "svc").await;
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Failed);
}

/// Without any new config, a 3-miss hang is detected as before (threshold 3, timeout 2 s),
/// but a blip shorter than the threshold never changes the state.
#[tokio::test]
async fn defaults_keep_the_three_miss_grace() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(dir.path(), &cfg("sleep 30", srv.port, |_| {}));
    up(&mut m).await;
    srv.status.store(500, Ordering::Release);
    m.check_health().await;
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running, "2 of 3 misses");
    srv.status.store(200, Ordering::Release);
    m.check_health().await;
    srv.status.store(500, Ordering::Release);
    m.check_health().await;
    m.check_health().await;
    assert_eq!(
        state(&m),
        ServiceState::Running,
        "streak was reset by the answer"
    );
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Unhealthy, "3 in a row");
    m.stop_service("svc").await;
}

/// Incident #136: `/health` is 200, `/ready` 500 (schema lagged the code) → `degraded`
/// with the probe's own error; fixed → `running`.
#[tokio::test]
async fn ready_failing_while_liveness_passes_is_degraded_with_the_probe_error() {
    let srv = Server::start(200, 500).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.ready = Some(ReadyProbe {
                http: Some("/ready".into()),
                status: None,
                command: None,
                timeout: Some(1),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;

    assert_eq!(state(&m), ServiceState::Degraded);
    let err = m.probe_error("svc").expect("probe error");
    assert!(err.contains("/ready") && err.contains("500"), "{err}");
    let note = m.service_note("svc").unwrap();
    assert!(note.contains("500"), "the card says why: {note}");

    srv.ready_status.store(200, Ordering::Release);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running);
    assert!(m.probe_error("svc").is_none());
    m.stop_service("svc").await;
}

#[tokio::test]
async fn ready_expected_status_is_exact_when_given() {
    let srv = Server::start(200, 204).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.ready = Some(ReadyProbe {
                http: Some("/ready".into()),
                status: Some(200),
                command: None,
                timeout: Some(1),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;
    assert_eq!(
        state(&m),
        ServiceState::Degraded,
        "204 is 2xx but 200 was required"
    );
    assert!(m.probe_error("svc").unwrap().contains("expected 200"));
    srv.ready_status.store(200, Ordering::Release);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running);
    m.stop_service("svc").await;
}

#[tokio::test]
async fn a_hung_ready_endpoint_is_bounded_by_its_timeout_and_degrades() {
    let srv = Server::start(200, 0).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.ready = Some(ReadyProbe {
                http: Some("/ready".into()),
                status: None,
                command: None,
                timeout: Some(1),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    let t0 = std::time::Instant::now();
    m.check_health().await;
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(state(&m), ServiceState::Degraded);
    assert!(m.probe_error("svc").unwrap().contains("timed out"));
    m.stop_service("svc").await;
}

#[tokio::test]
async fn a_command_ready_probe_degrades_on_nonzero_exit_with_its_stderr() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.ready = Some(ReadyProbe {
                http: None,
                status: None,
                command: Some(crate::runtime::process::test_sh(
                    "echo schema out of date >&2; exit 3",
                )),
                timeout: Some(5),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Degraded);
    let err = m.probe_error("svc").unwrap();
    assert!(
        err.contains("exited 3") && err.contains("schema out of date"),
        "{err}"
    );
    m.stop_service("svc").await;
}

/// A service with no `ready` behaves as today: a 500 on `/ready` is nobody's business.
#[tokio::test]
async fn without_a_ready_probe_nothing_is_degraded() {
    let srv = Server::start(200, 500).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(dir.path(), &cfg("sleep 30", srv.port, |_| {}));
    up(&mut m).await;
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running);
    assert!(m.probe_error("svc").is_none());
    m.stop_service("svc").await;
}

/// Liveness lost beats a failing ready probe.
#[tokio::test]
async fn unhealthy_wins_over_a_failing_ready_probe() {
    let srv = Server::start(200, 500).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", srv.port, |d| {
            d.health_timeout = Some(1);
            d.failure_threshold = Some(1);
            d.ready = Some(ReadyProbe {
                http: Some("/ready".into()),
                status: None,
                command: None,
                timeout: Some(1),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Degraded);
    srv.status.store(0, Ordering::Release);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Unhealthy);
    m.stop_service("svc").await;
}

/// `on_unhealthy: restart` restarts a hung service (new pid); the default only notifies.
#[tokio::test]
async fn on_unhealthy_restart_respawns_the_hung_process_and_notify_does_not() {
    for (policy, restarts) in [(OnUnhealthy::Restart, true), (OnUnhealthy::Notify, false)] {
        let srv = Server::start(200, 200).await;
        let dir = tempfile::tempdir().unwrap();
        let (mut m, mut rx) = manager(
            dir.path(),
            &cfg("sleep 30", srv.port, |d| {
                d.health_timeout = Some(1);
                d.failure_threshold = Some(1);
                d.on_unhealthy = policy;
            }),
        );
        up(&mut m).await;
        let pid = m.services["svc"].status.pid.unwrap();
        srv.status.store(0, Ordering::Release);
        m.check_health().await;

        let new_pid = m.services["svc"].status.pid;
        if restarts {
            assert_ne!(new_pid, Some(pid), "{policy:?}: respawned");
            assert!(m.services["svc"].child.is_some());
        } else {
            assert_eq!(new_pid, Some(pid), "{policy:?}: left alone");
            assert_eq!(state(&m), ServiceState::Unhealthy);
        }
        // Either way the user was told.
        let mut alerted = false;
        while let Ok(ev) = rx.try_recv() {
            if let ManagerEvent::CriticalAlert { message, .. } = ev {
                alerted |= message.contains("unhealthy");
            }
        }
        assert!(
            alerted,
            "{policy:?}: an alert says the service is unhealthy"
        );
        m.stop_service("svc").await;
    }
}

// ── repeated error lines ──────────────────────────────────────────────

fn feed(m: &ServiceManager, line: &str, error: bool) {
    let ctx = m.services["svc"].probes.ctx.clone();
    ctx.lock()
        .unwrap()
        .push(line, error, std::time::Instant::now());
}

/// #136: a background loop logged the same DB timeout again and again while liveness stayed green.
#[tokio::test]
async fn the_same_error_three_times_degrades_with_the_sample_line_and_clears() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(dir.path(), &cfg("sleep 30", srv.port, |_| {}));
    up(&mut m).await;

    feed(&m, "ERROR 10:00:01 db connect timeout after 5001ms", true);
    feed(&m, "ERROR 10:00:07 db connect timeout after 5003ms", true);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running, "two is not a pattern");

    feed(&m, "ERROR 10:00:13 db connect timeout after 5002ms", true);
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Degraded);
    let why = m.probe_error("svc").unwrap();
    assert!(why.contains("db connect timeout"), "sample line: {why}");

    // They stop: make the hits age out (the detector is clock-injected, so move them).
    m.services["svc"].probes.ctx.lock().unwrap().repeats.clear();
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running, "clears when they stop");
    assert!(m.probe_error("svc").is_none());
    m.stop_service("svc").await;
}

/// Real output of a real child: the pump → detector → state path, nothing fed by hand.
#[tokio::test]
async fn a_child_that_prints_the_same_error_repeatedly_becomes_degraded() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let script = "for i in 1 2 3 4; do echo \"ERROR db connect timeout after ${i}000ms\" >&2; done; sleep 30";
    let (mut m, _rx) = manager(dir.path(), &cfg(script, srv.port, |_| {}));
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while state(&m) != ServiceState::Degraded && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        m.check_health().await;
    }
    assert_eq!(state(&m), ServiceState::Degraded);
    assert!(m.probe_error("svc").unwrap().contains("db connect timeout"));
    m.stop_service("svc").await;
}

/// Non-error chatter never degrades, however often it repeats.
#[tokio::test]
async fn repeated_info_lines_do_not_degrade() {
    let srv = Server::start(200, 200).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(dir.path(), &cfg("sleep 30", srv.port, |_| {}));
    up(&mut m).await;
    for _ in 0..10 {
        feed(&m, "GET /health 200 2ms", false);
    }
    m.check_health().await;
    assert_eq!(state(&m), ServiceState::Running);
    m.stop_service("svc").await;
}

// ── status file ───────────────────────────────────────────────────────

fn status_row(dir: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join(".logs/.tui-status.json")).unwrap();
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["services"]["svc"].clone()
}

/// The status file is what MCP, `atlas dev --status` and the desktop read.
#[tokio::test]
async fn the_status_file_carries_state_since_and_probe_error() {
    let srv = Server::start(200, 500).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("echo hello; sleep 30", srv.port, |d| {
            d.health_timeout = Some(1);
            d.failure_threshold = Some(1);
            d.ready = Some(ReadyProbe {
                http: Some("/ready".into()),
                status: None,
                command: None,
                timeout: Some(1),
            });
        }),
    );
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
    m.check_health().await;
    let row = status_row(dir.path());
    assert_eq!(row["state"], "degraded");
    assert!(
        row["probe_error"].as_str().unwrap().contains("500"),
        "{row}"
    );
    assert!(row["unhealthy_since"].is_null());

    srv.status.store(0, Ordering::Release);
    m.check_health().await;
    let row = status_row(dir.path());
    assert_eq!(row["state"], "unhealthy");
    assert!(row["unhealthy_since"].as_u64().unwrap() > 0, "{row}");
    assert!(
        row["unhealthy_lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("hello")),
        "{row}"
    );
    m.stop_service("svc").await;
    let _ = srv.hits.load(Ordering::Acquire);
}
