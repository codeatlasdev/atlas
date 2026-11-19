//! What the engine says about a service reaches four machine consumers as English bytes: the notes and
//! alerts of the event stream, the status file (`.logs/.tui-status.json`), the MCP `atlas_status` tool and the
//! `fail_reason` that `atlas dev --ci` prints. Giving those sentences a stable code (`runtime::note`) must not
//! change one byte of them.
//!
//! `engine_status.json` is the REAL output of the producers, written before the codes existed
//! (`UPDATE_FIXTURES=1 cargo test -p atlas-tui engine_text`); this test drives the same services through the
//! same loop and compares the bytes.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::ServiceManager;
use super::types::ManagerEvent;
use crate::config::{DependsOn, ProjectConfig, RestartPolicy, ServiceDef};
use crate::runtime::service::ServiceState;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/engine_status.json");

fn svc(cmd: &str) -> ServiceDef {
    ServiceDef {
        command: Some(crate::runtime::process::test_sh(cmd)),
        ..Default::default()
    }
}

fn deps(pairs: &[(&str, &str)]) -> Option<DependsOn> {
    let yaml: String = pairs.iter().map(|(n, c)| format!("{n}: {c}\n")).collect();
    Some(serde_norway::from_str(&yaml).unwrap())
}

/// The sentence of one note/alert event, the way every machine consumer reads it.
fn said(ev: &ManagerEvent) -> Option<(&'static str, String, String)> {
    match ev {
        ManagerEvent::ServiceNote { name, message } => Some(("note", name.clone(), message.clone())),
        ManagerEvent::CriticalAlert { service, message } => {
            Some(("alert", service.clone(), message.clone()))
        }
        _ => None,
    }
}

/// Keys that change on every run (clocks, pids, samples).
const VOLATILE: [&str; 9] = [
    "pid",
    "updated_at",
    "started_at",
    "uptime_s",
    "mem_mb",
    "cpu",
    "heartbeat_age_s",
    "session",
    "clients",
];

fn without_volatile(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.retain(|k, _| !VOLATILE.contains(&k.as_str()));
            o.values_mut().for_each(without_volatile);
        }
        Value::Array(a) => a.iter_mut().for_each(without_volatile),
        _ => {}
    }
}

/// Drive a manager through services that exercise every scheduler sentence reachable without a wall clock.
async fn run_scenario() -> Value {
    let dir = tempfile::tempdir().unwrap();
    let services: Vec<(&str, ServiceDef)> = vec![
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
        (
            "once",
            ServiceDef {
                oneshot: true,
                ..svc("exit 0")
            },
        ),
        (
            "clean",
            ServiceDef {
                restart: RestartPolicy::OnFailure,
                ..svc("exit 0")
            },
        ),
        (
            "loop",
            ServiceDef {
                restart: RestartPolicy::Always,
                max_restarts: Some(1),
                ..svc("exit 1")
            },
        ),
        (
            "empty",
            ServiceDef {
                command: Some("  ".into()),
                ..Default::default()
            },
        ),
        ("slow", svc("sleep 1")),
        (
            "waiter",
            ServiceDef {
                depends_on: deps(&[("slow", "completed_successfully")]),
                ..svc("sleep 30")
            },
        ),
    ];
    let config = ProjectConfig {
        name: "t".into(),
        services: services
            .into_iter()
            .map(|(n, d)| (n.to_string(), d))
            .collect::<HashMap<_, _>>(),
        ..Default::default()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut m = ServiceManager::new(PathBuf::from(dir.path()), &config, tx);
    let all: Vec<String> = [
        "bad", "mid", "leaf", "once", "clean", "loop", "empty", "slow", "waiter",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    m.start_app_services(&all).await;

    let mut events: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let deadline = tokio::time::Instant::now() + atlas_core::scaling::HANG;
    let settled = |m: &ServiceManager| {
        m.get_state("leaf") == Some(ServiceState::Failed)
            && m.get_state("loop") == Some(ServiceState::Failed)
            && m.get_state("waiter") == Some(ServiceState::Running)
            && m.get_state("clean") == Some(ServiceState::Stopped)
            && m.get_state("once") == Some(ServiceState::Stopped)
    };
    while tokio::time::Instant::now() < deadline {
        while let Ok(ev) = rx.try_recv() {
            if let Some((kind, name, message)) = said(&ev) {
                events.entry(name).or_default().push(json!([kind, message]));
            }
            match ev {
                ManagerEvent::StateChanged { name, state } => m.update_state(&name, state),
                ManagerEvent::LogReady { name } => m.mark_log_ready(&name),
                _ => {}
            }
        }
        m.check_health().await;
        if settled(&m) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(settled(&m), "the scenario did not settle");
    // What `--ci` prints as `reason` for a failed service, and the sidebar's note for each.
    let mut ci: BTreeMap<String, Value> = BTreeMap::new();
    for n in &all {
        ci.insert(
            n.clone(),
            json!({"fail_reason": m.fail_reason(n), "service_note": m.service_note(n)}),
        );
    }
    m.write_status_file();
    let mut file: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    without_volatile(&mut file);
    let mcp = crate::mcp::tools::call(dir.path(), "atlas_status", &json!({}))
        .await
        .unwrap();
    let mut mcp: Value = serde_json::from_str(&mcp).unwrap();
    without_volatile(&mut mcp);
    m.stop_all().await;
    json!({"events": events, "ci": ci, "status_file": file, "mcp": mcp})
}

/// The memory sentences: soft budget, `max_memory` (restart, stop, notify), a leak, and the budget clearing.
async fn run_memory_scenario() -> Value {
    use crate::config::OnMemoryMax;
    const MB: u64 = 1 << 20;
    let dir = tempfile::tempdir().unwrap();
    let def = |tweak: &dyn Fn(&mut ServiceDef)| {
        let mut d = svc("sleep 30");
        tweak(&mut d);
        d
    };
    let config = ProjectConfig {
        name: "t".into(),
        services: HashMap::from([
            (
                "warn".to_string(),
                def(&|d| d.memory_warn = Some("1G".into())),
            ),
            (
                "notify".to_string(),
                def(&|d| {
                    d.max_memory = Some("1G".into());
                    d.on_memory_max = OnMemoryMax::Notify;
                }),
            ),
            (
                "restart".to_string(),
                def(&|d| d.max_memory = Some("1G".into())),
            ),
            ("leak".to_string(), def(&|_| {})),
        ]),
        ..Default::default()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut m = ServiceManager::new(PathBuf::from(dir.path()), &config, tx);
    m.min_uptime = Duration::ZERO;
    m.usage_every = Duration::from_secs(3600);
    m.sampled_at = Some(std::time::Instant::now());
    for n in ["warn", "notify", "restart", "leak"] {
        m.start_service(n).await;
        m.update_state(n, ServiceState::Running);
    }
    let mut events: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut drain = |rx: &mut mpsc::UnboundedReceiver<ManagerEvent>| {
        while let Ok(ev) = rx.try_recv() {
            if let Some((kind, name, message)) = said(&ev) {
                events.entry(name).or_default().push(json!([kind, message]));
            }
        }
    };
    let mut probe_errors: BTreeMap<String, Value> = BTreeMap::new();

    for (i, mb) in [500u64, 1200, 1300].iter().enumerate() {
        m.test_sample("warn", 1000 + 5 * i as u64, mb * MB, &[])
            .await;
    }
    drain(&mut rx);
    probe_errors.insert("warn_degraded".into(), json!(m.probe_error("warn")));
    for (i, mb) in [400u64, 400].iter().enumerate() {
        m.test_sample("warn", 1015 + 5 * i as u64, mb * MB, &[])
            .await;
    }
    drain(&mut rx);

    for i in 0..4u64 {
        m.test_sample("notify", 1000 + 5 * i, 2000 * MB, &[]).await;
    }
    drain(&mut rx);
    probe_errors.insert("notify_degraded".into(), json!(m.probe_error("notify")));

    for round in 0..4u64 {
        m.update_state("restart", ServiceState::Running);
        for i in 0..2u64 {
            m.test_sample("restart", 1000 + round * 20 + 5 * i, 2000 * MB, &[])
                .await;
        }
    }
    drain(&mut rx);

    let mut t = 1000;
    for mb in [300u64, 340, 380, 420, 460, 500] {
        m.test_sample("leak", t, mb * MB, &[t]).await;
        t += 10;
    }
    drain(&mut rx);

    let notes: BTreeMap<&str, Value> = ["warn", "notify", "restart", "leak"]
        .iter()
        .map(|n| {
            (
                *n,
                json!({
                    "service_note": m.service_note(n),
                    "probe_error": m.probe_error(n),
                    "leak": m.memory_facts(n).and_then(|f| f.leak),
                    "fail_reason": m.fail_reason(n),
                }),
            )
        })
        .collect();
    m.write_status_file();
    let mut file: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    without_volatile(&mut file);
    if let Some(s) = file["services"].as_object_mut() {
        // Samples and the process tree's own numbers move run to run.
        s.values_mut().for_each(|v| {
            if let Some(o) = v.as_object_mut() {
                o.remove("memory_rss");
                o.remove("memory_trend");
                o.remove("memory_trend_mb_min");
            }
        });
    }
    let mcp = crate::mcp::tools::call(dir.path(), "atlas_status", &json!({}))
        .await
        .unwrap();
    let mut mcp: Value = serde_json::from_str(&mcp).unwrap();
    without_volatile(&mut mcp);
    if let Some(rows) = mcp["services"].as_array_mut() {
        rows.iter_mut().for_each(|v| {
            if let Some(o) = v.as_object_mut() {
                o.remove("memory_rss");
                o.remove("memory_trend");
                o.remove("memory_trend_mb_min");
            }
        });
    }
    for n in ["warn", "notify", "restart", "leak"] {
        m.stop_service(n).await;
    }
    json!({"events": events, "notes": notes, "probe_errors": probe_errors, "status_file": file, "mcp": mcp})
}

#[tokio::test]
async fn status_file_mcp_ci_and_events_keep_their_english_bytes() {
    let doc = json!({"scheduler": run_scenario().await, "memory": run_memory_scenario().await});
    let got = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FIXTURE).parent().unwrap()).unwrap();
        std::fs::write(FIXTURE, &got).unwrap();
    }
    let want = std::fs::read_to_string(FIXTURE)
        .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it");
    // Compared as documents first so a failure names the key; then as bytes.
    let (g, w): (Value, Value) = (
        serde_json::from_str(&got).unwrap(),
        serde_json::from_str(&want).unwrap(),
    );
    assert_eq!(g, w);
    assert_eq!(got, want);
}
