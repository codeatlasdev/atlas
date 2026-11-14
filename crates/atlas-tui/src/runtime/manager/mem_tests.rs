//! Memory budgets, trend and leak detection applied by the manager (#131): the soft budget
//! turns a service `degraded` with the reason, `max_memory` restarts or only notifies, the
//! series reaches the status file and `.tui-metrics.json`, and a real child that grows its
//! memory stepwise (a re-run of this test binary, like `port_holder_helper`) is measured by
//! the real sampler.

use std::time::Duration;

use tokio::sync::mpsc;

use super::*;
use crate::config::{OnMemoryMax, ProjectConfig, ReadyProbe, ServiceDef};
use crate::runtime::memory::{Budget, Direction};
use crate::runtime::service::ServiceState;

const MB: u64 = 1 << 20;

fn cfg(command: &str, tweak: impl FnOnce(&mut ServiceDef)) -> ProjectConfig {
    let mut def = ServiceDef {
        command: Some(crate::runtime::process::test_sh(command)),
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
    let mut m = ServiceManager::new(dir.to_path_buf(), cfg, tx);
    // A portless service counts as running after MIN_UPTIME; these tests promote it by hand.
    m.min_uptime = Duration::ZERO;
    // Readings come from `test_sample`; a health tick must not add a real one of its own.
    m.usage_every = Duration::from_secs(3600);
    m.sampled_at = Some(std::time::Instant::now());
    (m, rx)
}

/// Spawn and promote to `Running` like the event loop does (no probes configured: it stays).
async fn up(m: &mut ServiceManager) {
    m.start_service("svc").await;
    m.update_state("svc", ServiceState::Running);
}

fn state(m: &ServiceManager) -> ServiceState {
    m.get_state("svc").unwrap()
}

/// Feed `mbs` readings 5 s apart starting at `t0`, through the same path a real sample takes.
async fn feed(m: &mut ServiceManager, t0: u64, mbs: &[u64]) {
    for (i, mb) in mbs.iter().enumerate() {
        m.test_sample("svc", t0 + 5 * i as u64, mb * MB, &[]).await;
    }
}

fn alerts(rx: &mut mpsc::UnboundedReceiver<ManagerEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let ManagerEvent::CriticalAlert { message, .. } = ev {
            out.push(message);
        }
    }
    out
}

// ── memory_warn → degraded ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn over_memory_warn_for_two_samples_is_degraded_with_the_reason_and_clears_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.memory_warn = Some("1G".into())),
    );
    up(&mut m).await;

    feed(&mut m, 1000, &[500]).await;
    assert_eq!(state(&m), ServiceState::Running);
    feed(&mut m, 1005, &[1200]).await;
    assert_eq!(
        state(&m),
        ServiceState::Running,
        "one sample over is a spike, not a state"
    );
    feed(&mut m, 1010, &[1300]).await;
    assert_eq!(state(&m), ServiceState::Degraded);
    assert_eq!(
        m.probe_error("svc").as_deref(),
        Some("using 1.3 GB, over memory_warn 1.0 GB")
    );
    assert_eq!(m.memory_facts("svc").unwrap().budget, Budget::Warn);

    feed(&mut m, 1015, &[400]).await;
    assert_eq!(
        state(&m),
        ServiceState::Degraded,
        "one sample under: no flapping"
    );
    feed(&mut m, 1020, &[400]).await;
    assert_eq!(state(&m), ServiceState::Running, "cleared on its own");
    assert!(m.probe_error("svc").is_none());
    assert_eq!(m.memory_facts("svc").unwrap().budget, Budget::Ok);
    m.stop_service("svc").await;
}

/// Precedence with #184: a failing `ready` probe is the reason a service is degraded; memory
/// never replaces it, and when the probe recovers the memory reason is what remains.
#[tokio::test]
async fn a_ready_probe_error_outranks_the_memory_reason_and_memory_stays_after_it_clears() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| {
            d.memory_warn = Some("1G".into());
            d.ready = Some(ReadyProbe {
                command: Some(crate::runtime::process::test_sh("false")),
                http: None,
                status: None,
                timeout: Some(2),
            });
        }),
    );
    up(&mut m).await;
    feed(&mut m, 1000, &[1300, 1300]).await;
    assert_eq!(state(&m), ServiceState::Degraded);
    assert!(
        m.probe_error("svc").unwrap().contains("memory_warn"),
        "memory is the only finding so far"
    );

    // The ready probe fails on the next tick: its error wins.
    m.check_health().await;
    let reason = m.probe_error("svc").unwrap();
    assert!(reason.contains("ready probe failed"), "{reason}");
    assert!(!reason.contains("memory_warn"), "{reason}");
    assert_eq!(state(&m), ServiceState::Degraded);

    // Memory dropping must not flip a service that is degraded for the probe's reason.
    feed(&mut m, 1010, &[300, 300]).await;
    assert_eq!(state(&m), ServiceState::Degraded);
    assert!(m.probe_error("svc").unwrap().contains("ready probe failed"));
    m.stop_service("svc").await;
}

#[tokio::test]
async fn memory_never_degrades_a_service_that_is_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.memory_warn = Some("1G".into())),
    );
    m.start_service("svc").await; // `starting`, not promoted
    feed(&mut m, 1000, &[1300, 1300]).await;
    assert_eq!(state(&m), ServiceState::Starting);
    m.stop_service("svc").await;
}

// ── max_memory: restart | notify ────────────────────────────────────────────────────────────

#[tokio::test]
async fn over_max_memory_restarts_by_default_with_an_alert_and_a_fresh_budget() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.max_memory = Some("1G".into())),
    );
    up(&mut m).await;
    let first_pid = m.services["svc"].status.pid.unwrap();

    feed(&mut m, 1000, &[2000, 2000]).await;
    let a = alerts(&mut rx);
    assert!(
        a.iter()
            .any(|x| x.contains("restarting (1/3)") && x.contains("max_memory 1.0 GB")),
        "{a:?}"
    );
    assert_ne!(
        m.services["svc"].status.pid.unwrap(),
        first_pid,
        "a new process replaced the leaking one"
    );
    assert_eq!(
        m.memory_facts("svc").unwrap().budget,
        Budget::Ok,
        "the new process earns its own standing"
    );
    m.stop_service("svc").await;
}

#[tokio::test]
async fn after_three_restarts_a_service_over_max_memory_is_stopped_and_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.max_memory = Some("1G".into())),
    );
    up(&mut m).await;
    for round in 0..4u64 {
        m.update_state("svc", ServiceState::Running);
        feed(&mut m, 1000 + round * 20, &[2000, 2000]).await;
    }
    let a = alerts(&mut rx);
    assert!(a.iter().any(|x| x.contains("restarting (3/3)")), "{a:?}");
    assert!(
        a.iter().any(|x| x.contains("after 3 restarts, stopped")),
        "{a:?}"
    );
    assert_eq!(state(&m), ServiceState::Failed);
    assert!(m.services["svc"].child.is_none(), "really stopped");
}

#[tokio::test]
async fn on_memory_max_notify_alerts_once_leaves_it_running_and_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| {
            d.max_memory = Some("1G".into());
            d.on_memory_max = OnMemoryMax::Notify;
        }),
    );
    up(&mut m).await;
    let pid = m.services["svc"].status.pid.unwrap();

    feed(&mut m, 1000, &[2000, 2000, 2000, 2000]).await;
    assert_eq!(m.services["svc"].status.pid.unwrap(), pid, "not restarted");
    assert_eq!(state(&m), ServiceState::Degraded);
    assert_eq!(
        m.probe_error("svc").as_deref(),
        Some("using 2.0 GB, over max_memory 1.0 GB")
    );
    let a = alerts(&mut rx);
    assert_eq!(a.len(), 1, "one alert on entering, none per sample: {a:?}");
    assert!(a[0].contains("on_memory_max: notify"), "{a:?}");
    assert_eq!(m.memory_facts("svc").unwrap().budget, Budget::Over);
    m.stop_service("svc").await;
}

// ── leak detection across reloads and restarts ──────────────────────────────────────────────

#[tokio::test]
async fn growth_after_every_reload_alerts_a_suspected_leak_once_and_rides_in_the_note() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(dir.path(), &cfg("sleep 30", |_| {}));
    up(&mut m).await;

    // A reload every 10 s, RSS +40 MB each time.
    let mut t = 1000;
    for (i, mb) in [300u64, 340, 380, 420, 460, 500].iter().enumerate() {
        m.test_sample("svc", t, mb * MB, &[t]).await;
        t += 10;
        if i < 3 {
            assert!(alerts(&mut rx).is_empty(), "no alert before the evidence");
        }
    }
    let a = alerts(&mut rx);
    assert_eq!(a.len(), 1, "{a:?}");
    assert!(a[0].starts_with("leak suspected: +"), "{a:?}");
    assert!(a[0].contains("reloads"), "{a:?}");
    assert!(m.service_note("svc").unwrap().contains("leak suspected"));
    assert!(m.memory_facts("svc").unwrap().leak.is_some());
    assert_eq!(
        state(&m),
        ServiceState::Running,
        "a leak is a warning, not a state"
    );
    m.stop_service("svc").await;
}

#[tokio::test]
async fn a_service_that_returns_to_its_level_after_each_reload_is_never_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(dir.path(), &cfg("sleep 30", |_| {}));
    up(&mut m).await;
    let mut t = 1000;
    for mb in [300u64, 330, 300, 335, 301, 332, 300, 331] {
        m.test_sample("svc", t, mb * MB, &[t]).await;
        t += 10;
    }
    assert!(alerts(&mut rx).is_empty());
    assert!(m.memory_facts("svc").unwrap().leak.is_none());
    m.stop_service("svc").await;
}

#[tokio::test]
async fn growth_across_restarts_counts_as_a_leak_too() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(dir.path(), &cfg("sleep 30", |_| {}));
    up(&mut m).await;
    let mut t = 1000;
    for mb in [300u64, 380, 460, 540, 620, 700] {
        m.test_sample("svc", t, mb * MB, &[]).await;
        t += 5;
        m.restart_service("svc").await; // closes a cycle with what the old process reached
        m.update_state("svc", ServiceState::Running);
    }
    let a = alerts(&mut rx);
    assert!(
        a.iter()
            .any(|x| x.starts_with("leak suspected") && x.contains("restarts")),
        "{a:?}"
    );
    m.stop_service("svc").await;
}

// ── series, trend, files ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_series_trend_and_budget_reach_the_status_file_and_the_metrics_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.memory_warn = Some("1G".into())),
    );
    up(&mut m).await;
    // +20 MB every 5 s = 240 MB/min, climbing through the budget.
    let mbs: Vec<u64> = (0..12).map(|i| 700 + 40 * i).collect();
    feed(&mut m, 1000, &mbs).await;
    m.write_status_file();
    m.write_metrics_file();

    let status: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    let row = &status["services"]["svc"];
    assert_eq!(row["state"], "degraded");
    assert_eq!(row["budget_state"], "warn");
    assert_eq!(row["memory_rss"], 1140u64 * MB);
    assert_eq!(row["memory_trend"], "rising");
    assert!(
        row["memory_trend_mb_min"].as_f64().unwrap() > 400.0,
        "{row}"
    );

    let metrics: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-metrics.json")).unwrap(),
    )
    .unwrap();
    let svc = &metrics["services"]["svc"];
    assert_eq!(svc["samples"].as_array().unwrap().len(), 12);
    assert_eq!(svc["samples"][0], serde_json::json!([1000, 700 * MB]));
    assert_eq!(
        svc["interval_s"], 3600,
        "the interval this manager samples at"
    );
    assert_eq!(svc["budget_state"], "warn");
    assert_eq!(
        m.memory_facts("svc").unwrap().trend.unwrap().direction,
        Direction::Rising
    );
    m.stop_service("svc").await;
}

#[tokio::test]
async fn a_stopped_service_reports_no_rss_and_no_budget_but_keeps_its_series() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.memory_warn = Some("1G".into())),
    );
    up(&mut m).await;
    feed(&mut m, 1000, &[1300, 1300]).await;
    m.stop_service("svc").await;
    let f = m.memory_facts("svc").unwrap();
    assert_eq!((f.rss, f.trend, f.budget), (None, None, Budget::Ok));
    m.write_metrics_file();
    let metrics: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-metrics.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        metrics["services"]["svc"]["samples"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn the_mcp_tools_return_what_the_real_producer_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(
        dir.path(),
        &cfg("sleep 30", |d| d.memory_warn = Some("1G".into())),
    );
    up(&mut m).await;
    let mbs: Vec<u64> = (0..8).map(|i| 900 + 50 * i).collect();
    feed(&mut m, 1000, &mbs).await;
    m.write_status_file();
    m.write_metrics_file();

    let out: serde_json::Value = serde_json::from_str(
        &crate::mcp::tools::metrics_tool(
            dir.path(),
            &serde_json::json!({"service": "svc", "last": 3}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(out["samples"].as_array().unwrap().len(), 3);
    assert_eq!(out["rss_bytes"], 1250 * MB);
    assert_eq!(out["min_bytes"], 1150 * MB);
    assert_eq!(out["max_bytes"], 1250 * MB);
    assert_eq!(out["budget_state"], "warn");
    assert_eq!(out["trend"], "rising");
    assert_eq!(out["running"], true);
    assert_eq!(out["state"], "degraded");

    let err = crate::mcp::tools::metrics_tool(dir.path(), &serde_json::json!({"service": "nope"}))
        .unwrap_err();
    assert!(err.contains("Known services: svc"), "{err}");
    assert!(crate::mcp::tools::metrics_tool(dir.path(), &serde_json::json!({})).is_err());

    let status: serde_json::Value =
        serde_json::from_str(&crate::mcp::tools::status_tool(dir.path()).unwrap()).unwrap();
    let svc = status["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "svc")
        .unwrap();
    assert_eq!(svc["budget_state"], "warn");
    assert_eq!(svc["memory_trend"], "rising");
    assert_eq!(svc["memory_rss"], 1250u64 * MB);
    m.stop_service("svc").await;

    // The run ends (its status file names a dead pid): the series is still there to compare,
    // and says it is no longer live.
    let path = dir.path().join(".logs/.tui-status.json");
    let mut file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    file["pid"] = (i32::MAX as u32).into();
    std::fs::write(&path, file.to_string()).unwrap();
    let after: serde_json::Value = serde_json::from_str(
        &crate::mcp::tools::metrics_tool(dir.path(), &serde_json::json!({"service": "svc"}))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after["running"], false);
    assert!(after["state"].is_null());
    assert_eq!(after["samples"].as_array().unwrap().len(), 8);
}

#[tokio::test]
async fn metrics_before_atlas_ever_sampled_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let err = crate::mcp::tools::metrics_tool(dir.path(), &serde_json::json!({"service": "svc"}))
        .unwrap_err();
    assert!(err.contains("no metrics yet"), "{err}");
}

// ── a real child that grows its memory stepwise ─────────────────────────────────────────────

/// Not a test of its own: when `ATLAS_TEST_MEM_GROWER=<step_mb>` is set this binary, run as a
/// service command by [`grower_command`], leaks `step_mb` MB (pages touched, so they count)
/// every 300 ms, logging `[bun] reloaded` each time, until killed. Otherwise a no-op.
#[test]
fn mem_grower_helper() {
    let Some(step) = std::env::var("ATLAS_TEST_MEM_GROWER")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return;
    };
    let mut hoard: Vec<Vec<u8>> = Vec::new();
    loop {
        hoard.push(vec![1u8; step << 20]);
        // Touch every page again so nothing is lazily zero-mapped.
        if let Some(b) = hoard.last_mut() {
            for p in b.iter_mut().step_by(4096) {
                *p = p.wrapping_add(1);
            }
        }
        println!("[bun] reloaded");
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(unix)]
fn grower_command() -> String {
    format!(
        "ATLAS_TEST_MEM_GROWER=40 '{}' --exact runtime::manager::mem_tests::mem_grower_helper --nocapture",
        std::env::current_exe().unwrap().display()
    )
}

/// The whole chain with no mocks: a real process leaks 40 MB per step, the real sampler (`ps`
/// / `proc_pid_rusage`) measures its tree, the series grows, the trend says `rising`, the
/// budget flips the service to `degraded`, the reload lines it prints are seen by the leak
/// detector, and `max_memory` (notify) alerts. Unix only: it relies on the shell `VAR=x cmd`.
#[cfg(unix)]
#[tokio::test]
async fn a_real_child_that_leaks_stepwise_is_measured_degraded_and_flagged_as_a_leak() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        dir.path(),
        &cfg(&grower_command(), |d| {
            d.memory_warn = Some("200M".into());
            d.max_memory = Some("100G".into());
        }),
    );
    m.usage_every = Duration::ZERO; // the real sampler runs on every call
    up(&mut m).await;

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut degraded = false;
    let mut leak = false;
    while std::time::Instant::now() < deadline && !(degraded && leak) {
        tokio::time::sleep(Duration::from_millis(400)).await;
        m.sample_usage().await;
        degraded = state(&m) == ServiceState::Degraded;
        leak = m.memory_facts("svc").is_some_and(|f| f.leak.is_some());
    }
    let facts = m.memory_facts("svc").unwrap();
    let series: Vec<u64> = m.services["svc"]
        .mem
        .series
        .tail(None)
        .iter()
        .map(|s| s.rss)
        .collect();
    assert!(degraded, "never degraded; series {series:?}");
    assert!(
        m.probe_error("svc").unwrap().contains("over memory_warn"),
        "{:?}",
        m.probe_error("svc")
    );
    assert!(
        leak,
        "reloads were not seen; cycles {}; series {series:?}",
        m.services["svc"].mem.leak.cycles()
    );
    assert!(facts.rss.unwrap() > 200 * MB);
    assert!(series.windows(2).filter(|w| w[1] >= w[0]).count() * 2 > series.len());
    let a = alerts(&mut rx);
    assert!(a.iter().any(|x| x.starts_with("leak suspected")), "{a:?}");
    m.stop_service("svc").await;
}
