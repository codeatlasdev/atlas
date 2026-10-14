//! Scheduler behavior: dependency gating, restarts, start timeouts, selection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;

use super::ServiceManager;
use super::types::ManagerEvent;
use crate::config::{DependsOn, ProjectConfig, RestartPolicy, ServiceDef};
use crate::runtime::service::ServiceState;

fn svc(cmd: &str) -> ServiceDef {
    ServiceDef {
        command: Some(cmd.to_string()),
        ..Default::default()
    }
}

fn deps(pairs: &[(&str, &str)]) -> Option<DependsOn> {
    let yaml: String = pairs.iter().map(|(n, c)| format!("{n}: {c}\n")).collect();
    Some(serde_norway::from_str(&yaml).unwrap())
}

fn manager(
    dir: &tempfile::TempDir,
    services: Vec<(&str, ServiceDef)>,
) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
    let config = ProjectConfig {
        name: "t".into(),
        services: services
            .into_iter()
            .map(|(n, d)| (n.to_string(), d))
            .collect::<HashMap<_, _>>(),
        ..Default::default()
    };
    let (tx, rx) = mpsc::unbounded_channel();
    (
        ServiceManager::new(PathBuf::from(dir.path()), &config, tx),
        rx,
    )
}

/// Drive the same loop headless does: apply watcher/log events and tick
/// `check_health` until `done` or the deadline.
async fn drive(
    m: &mut ServiceManager,
    rx: &mut mpsc::UnboundedReceiver<ManagerEvent>,
    secs: u64,
    mut done: impl FnMut(&ServiceManager) -> bool,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        while let Ok(ev) = rx.try_recv() {
            match ev {
                ManagerEvent::StateChanged { name, state } => m.update_state(&name, state),
                ManagerEvent::LogReady { name } => m.mark_log_ready(&name),
                _ => {}
            }
        }
        m.check_health().await;
        if done(m) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn dependent_waits_for_oneshot_to_complete_then_spawns() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![
            ("migrate", svc("sleep 1")),
            (
                "api",
                ServiceDef {
                    depends_on: deps(&[("migrate", "completed_successfully")]),
                    ..svc("sleep 30")
                },
            ),
        ],
    );
    m.start_app_services(&names(&["migrate", "api"])).await;
    assert_eq!(m.get_state("api"), Some(ServiceState::Stopped), "gated");
    assert_eq!(
        m.service_note("api").as_deref(),
        Some("waiting for migrate (completed_successfully)")
    );

    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("api") == Some(ServiceState::Running)
    })
    .await;
    assert!(m.completed_ok("migrate"), "implied one-shot completed");
    assert_eq!(m.get_state("migrate"), Some(ServiceState::Stopped));
    assert_eq!(m.get_state("api"), Some(ServiceState::Running));
    m.stop_all().await;
}

#[tokio::test]
async fn failed_dependency_fails_dependents_transitively_with_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![
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
        ],
    );
    m.start_app_services(&names(&["bad", "mid", "leaf"])).await;
    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("leaf") == Some(ServiceState::Failed)
    })
    .await;
    assert_eq!(m.get_state("bad"), Some(ServiceState::Failed));
    assert_eq!(m.get_state("mid"), Some(ServiceState::Failed));
    assert_eq!(m.get_state("leaf"), Some(ServiceState::Failed));
    assert_eq!(
        m.fail_reason("mid").as_deref(),
        Some("dependency bad failed")
    );
    assert_eq!(
        m.fail_reason("leaf").as_deref(),
        Some("dependency mid failed")
    );
    assert!(!m.scheduling_pending(), "nothing may wait forever");
    m.stop_all().await;
}

#[tokio::test]
async fn log_ready_gates_on_matching_line_and_marks_dependency_running() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![
            (
                "api",
                ServiceDef {
                    ready_log: Some("listening on \\d+".into()),
                    // No port/health: only the log can make it ready.
                    ..svc("sleep 1; echo listening on 80; sleep 30")
                },
            ),
            (
                "web",
                ServiceDef {
                    depends_on: deps(&[("api", "log_ready")]),
                    ..svc("sleep 30")
                },
            ),
        ],
    );
    m.start_app_services(&names(&["api", "web"])).await;
    // Child is alive but silent: web must not start on liveness alone.
    tokio::time::sleep(Duration::from_millis(300)).await;
    m.check_health().await;
    assert_eq!(m.get_state("web"), Some(ServiceState::Stopped));

    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("web") == Some(ServiceState::Running)
    })
    .await;
    assert_eq!(m.get_state("api"), Some(ServiceState::Running));
    assert_eq!(m.get_state("web"), Some(ServiceState::Running));
    m.stop_all().await;
}

#[tokio::test]
async fn on_failure_restarts_then_gives_up_and_never_double_spawns() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("spawns");
    let (mut m, mut rx) = manager(
        &dir,
        vec![(
            "crashy",
            ServiceDef {
                restart: RestartPolicy::OnFailure,
                max_restarts: Some(2),
                ..svc(&format!("echo x >> {}; exit 1", log.display()))
            },
        )],
    );
    m.start_app_services(&names(&["crashy"])).await;
    drive(&mut m, &mut rx, 15, |m| {
        m.fail_reason("crashy")
            .is_some_and(|r| r.contains("gave up"))
    })
    .await;
    assert_eq!(m.get_state("crashy"), Some(ServiceState::Failed));
    assert_eq!(
        m.fail_reason("crashy").as_deref(),
        Some("gave up after 2 restarts")
    );
    // Keep ticking: a gave-up service must stay down.
    drive(&mut m, &mut rx, 2, |_| false).await;
    let spawns = std::fs::read_to_string(&log).unwrap().lines().count();
    assert_eq!(spawns, 3, "1 initial + exactly 2 restarts");
    m.stop_all().await;
}

#[tokio::test]
async fn on_failure_leaves_clean_exit_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![(
            "task",
            ServiceDef {
                restart: RestartPolicy::OnFailure,
                ..svc("exit 0")
            },
        )],
    );
    m.start_app_services(&names(&["task"])).await;
    drive(&mut m, &mut rx, 5, |m| {
        m.get_state("task") == Some(ServiceState::Stopped)
    })
    .await;
    assert_eq!(m.get_state("task"), Some(ServiceState::Stopped));
    assert!(m.completed_ok("task"));
    assert!(!m.scheduling_pending());
    m.stop_all().await;
}

#[tokio::test]
async fn restart_no_keeps_legacy_failed_on_exit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(&dir, vec![("svc", svc("exit 1"))]);
    m.start_app_services(&names(&["svc"])).await;
    drive(&mut m, &mut rx, 5, |m| {
        m.get_state("svc") == Some(ServiceState::Failed)
    })
    .await;
    assert_eq!(m.get_state("svc"), Some(ServiceState::Failed));
    assert!(!m.scheduling_pending());
}

#[tokio::test]
async fn stop_and_quit_cancel_scheduled_restart() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("spawns");
    let (mut m, mut rx) = manager(
        &dir,
        vec![(
            "crashy",
            ServiceDef {
                restart: RestartPolicy::Always,
                max_restarts: Some(0),
                ..svc(&format!("echo x >> {}; exit 1", log.display()))
            },
        )],
    );
    m.start_app_services(&names(&["crashy"])).await;
    drive(&mut m, &mut rx, 5, |m| m.scheduling_pending()).await;
    assert!(m.scheduling_pending(), "restart should be scheduled");

    m.stop_service("crashy").await;
    assert!(!m.scheduling_pending(), "user stop cancels the restart");
    let before = std::fs::read_to_string(&log).unwrap().lines().count();
    drive(&mut m, &mut rx, 3, |_| false).await;
    let after = std::fs::read_to_string(&log).unwrap().lines().count();
    assert_eq!(before, after, "no respawn after a user stop");
    assert_eq!(m.get_state("crashy"), Some(ServiceState::Stopped));

    // And quit: restart again, then stop_all mid-backoff.
    m.restart_service("crashy").await;
    drive(&mut m, &mut rx, 5, |m| m.scheduling_pending()).await;
    m.stop_all().await;
    assert!(!m.scheduling_pending(), "stop_all cancels pending restarts");
}

/// Audit #1: a `ready_log` service with no port must stay Starting while the
/// child is alive but silent, and turn Running only on the match.
#[tokio::test]
async fn ready_log_service_stays_starting_until_regex_matches() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![(
            "slow",
            ServiceDef {
                ready_log: Some("Server ready".into()),
                ..svc("echo compiling; sleep 3; echo Server ready; sleep 30")
            },
        )],
    );
    m.start_app_services(&names(&["slow"])).await;
    // Several health ticks across > MIN_UPTIME while the log is still silent.
    drive(&mut m, &mut rx, 2, |_| false).await;
    assert_eq!(
        m.get_state("slow"),
        Some(ServiceState::Starting),
        "alive child must not promote a ready_log service"
    );
    assert_eq!(
        m.service_note("slow")
            .as_deref()
            .map(|n| n.starts_with("waiting for ready_log (")),
        Some(true),
        "{:?}",
        m.service_note("slow")
    );
    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("slow") == Some(ServiceState::Running)
    })
    .await;
    assert_eq!(m.get_state("slow"), Some(ServiceState::Running));
    assert_eq!(m.start_wait("slow"), None);
    assert!(m.uptime_secs("slow").is_some());
    m.stop_all().await;
}

/// Audit #2: a crash-looping port-less service never turns Running and never
/// satisfies a `healthy` dependent.
#[tokio::test]
async fn portless_crash_loop_never_runs_and_does_not_unblock_healthy_dep() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![
            (
                "worker",
                ServiceDef {
                    restart: RestartPolicy::OnFailure,
                    max_restarts: Some(2),
                    ..svc("sleep 1; exit 2")
                },
            ),
            (
                "dependent",
                ServiceDef {
                    depends_on: deps(&[("worker", "healthy")]),
                    ..svc("sleep 30")
                },
            ),
        ],
    );
    m.start_app_services(&names(&["worker", "dependent"])).await;
    let mut ever_running = false;
    drive(&mut m, &mut rx, 15, |m| {
        ever_running |= m.get_state("worker") == Some(ServiceState::Running);
        m.fail_reason("worker")
            .is_some_and(|r| r.contains("gave up"))
    })
    .await;
    assert!(!ever_running, "crash loop must never flash Running");
    assert_eq!(m.get_state("worker"), Some(ServiceState::Failed));
    assert_eq!(m.exit_code("worker"), Some(2));
    assert_eq!(m.restart_count("worker"), 2);
    // Dependent was never spawned on a false "healthy": it fails with the dep.
    drive(&mut m, &mut rx, 3, |m| {
        m.get_state("dependent") == Some(ServiceState::Failed)
    })
    .await;
    assert_eq!(m.get_state("dependent"), Some(ServiceState::Failed));
    assert_eq!(
        m.fail_reason("dependent").as_deref(),
        Some("dependency worker failed")
    );
    m.stop_all().await;
}

/// Race seen under the parallel gate: the no-probe watcher sends Running after
/// MIN_UPTIME without looking at the child, so a child that already exited but
/// was not reaped yet by check_health flashed Running.
#[tokio::test]
async fn late_running_event_for_an_exited_child_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(&dir, vec![("worker", svc("exit 2"))]);
    m.start_app_services(&names(&["worker"])).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    m.update_state("worker", ServiceState::Running);
    assert_ne!(m.get_state("worker"), Some(ServiceState::Running));
    m.stop_all().await;
}

/// The class of the race above: a watcher of the PREVIOUS spawn (its MIN_UPTIME timer started
/// before a restart) delivers Running while the new child is alive but has only just started.
/// Running for a no-probe service is accepted only after MIN_UPTIME of the current spawn.
#[tokio::test]
async fn late_running_event_for_a_fresh_respawn_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, _rx) = manager(&dir, vec![("worker", svc("sleep 30"))]);
    m.start_app_services(&names(&["worker"])).await;
    // The spawn is "just now", whatever the machine's load made start_app_services cost.
    m.services.get_mut("worker").unwrap().sched.spawned_at = Some(std::time::Instant::now());
    m.update_state("worker", ServiceState::Running);
    assert_ne!(m.get_state("worker"), Some(ServiceState::Running));
    // Once the current spawn is older than MIN_UPTIME, the same event is accepted.
    m.services.get_mut("worker").unwrap().sched.spawned_at =
        Some(std::time::Instant::now() - super::types::MIN_UPTIME);
    m.update_state("worker", ServiceState::Running);
    assert_eq!(m.get_state("worker"), Some(ServiceState::Running));
    m.stop_all().await;
}

/// A port-less service that stays alive past MIN_UPTIME does turn Running.
#[tokio::test]
async fn portless_service_runs_after_min_uptime() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(&dir, vec![("idle", svc("sleep 30"))]);
    m.start_app_services(&names(&["idle"])).await;
    m.check_health().await;
    assert_eq!(
        m.get_state("idle"),
        Some(ServiceState::Starting),
        "not green at t=0"
    );
    drive(&mut m, &mut rx, 6, |m| {
        m.get_state("idle") == Some(ServiceState::Running)
    })
    .await;
    assert_eq!(m.get_state("idle"), Some(ServiceState::Running));
    assert_eq!(m.exit_code("idle"), None);
    m.stop_all().await;
}

/// Item 3: a port that never opens shows `waiting for :PORT (Ns)`, then one
/// warn note, and a CriticalAlert when the service is `critical`.
#[tokio::test]
async fn start_timeout_warns_and_alerts_critical_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![(
            "badport",
            ServiceDef {
                port: Some(1),
                critical: Some(true),
                start_timeout: Some(1),
                ..svc("sleep 30")
            },
        )],
    );
    m.start_app_services(&names(&["badport"])).await;
    let (mut notes, mut alerts) = (0, 0);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_waiting = false;
    while tokio::time::Instant::now() < deadline && (notes == 0 || alerts == 0) {
        m.check_health().await;
        saw_waiting |= m
            .service_note("badport")
            .is_some_and(|n| n.starts_with("waiting for :1 ("));
        while let Ok(ev) = rx.try_recv() {
            match ev {
                ManagerEvent::ServiceNote { message, .. }
                    if message.starts_with("still starting after") =>
                {
                    notes += 1;
                }
                ManagerEvent::CriticalAlert { service, .. } if service == "badport" => {
                    alerts += 1;
                }
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(saw_waiting, "sidebar note must show the wait");
    assert_eq!((notes, alerts), (1, 1));
    assert_eq!(m.get_state("badport"), Some(ServiceState::Starting));
    assert!(
        m.service_note("badport")
            .is_some_and(|n| n.starts_with("start timeout: still waiting for :1 (")),
        "{:?}",
        m.service_note("badport")
    );
    // No second firing.
    m.check_health().await;
    let again = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|e| matches!(e, ManagerEvent::CriticalAlert { .. }))
        .count();
    assert_eq!(again, 0);
    m.stop_all().await;
}

/// Item 6: `autostart: false` is excluded from boot's app list and starts on demand.
#[tokio::test]
async fn autostart_false_is_not_booted_but_starts_on_demand() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(
        &dir,
        vec![
            ("api", svc("sleep 30")),
            (
                "lazy",
                ServiceDef {
                    autostart: Some(false),
                    ..svc("sleep 30")
                },
            ),
        ],
    );
    m.start_all().await;
    let app_names = loop {
        match rx.recv().await {
            Some(ManagerEvent::InfraReady { app_names }) => break app_names,
            Some(_) => {}
            None => panic!("no InfraReady"),
        }
    };
    assert_eq!(app_names, vec!["api".to_string()]);
    assert!(m.is_manual_start("lazy"));
    m.start_app_services(&app_names).await;
    assert_eq!(m.get_state("lazy"), Some(ServiceState::Stopped));
    m.restart_service("lazy").await;
    assert_eq!(m.get_state("lazy"), Some(ServiceState::Starting));
    m.stop_all().await;
}

fn manager_selected(
    dir: &tempfile::TempDir,
    services: Vec<(&str, ServiceDef)>,
    select: &[&str],
    no_deps: bool,
) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
    let config = ProjectConfig {
        name: "t".into(),
        services: services
            .into_iter()
            .map(|(n, d)| (n.to_string(), d))
            .collect::<HashMap<_, _>>(),
        ..Default::default()
    };
    let req = crate::config::SelectionRequest {
        services: select.iter().map(|s| s.to_string()).collect(),
        profiles: vec![],
        no_deps,
    };
    let selection = crate::config::resolve_selection(&config, &req).unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    (
        ServiceManager::with_selection(PathBuf::from(dir.path()), &config, tx, &selection),
        rx,
    )
}

/// `atlas dev web`: web + its dependency chain boot; the rest stay `off`
/// (Disabled), are marked in the status file and start by hand.
#[tokio::test]
async fn deselected_services_are_off_marked_and_startable_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager_selected(
        &dir,
        vec![
            ("db", svc("sleep 30")),
            (
                "api",
                ServiceDef {
                    depends_on: deps(&[("db", "started")]),
                    ..svc("sleep 30")
                },
            ),
            (
                "web",
                ServiceDef {
                    depends_on: deps(&[("api", "started")]),
                    ..svc("sleep 30")
                },
            ),
            ("worker", svc("sleep 30")),
        ],
        &["web"],
        false,
    );
    assert_eq!(m.get_state("worker"), Some(ServiceState::Disabled));
    assert!(m.is_deselected("worker") && !m.is_deselected("web"));
    m.start_all().await;
    let app_names = loop {
        match rx.recv().await {
            Some(ManagerEvent::InfraReady { app_names }) => break app_names,
            Some(_) => {}
            None => panic!("no InfraReady"),
        }
    };
    assert_eq!(app_names, names(&["db", "api", "web"]), "dependency order");
    m.start_app_services(&app_names).await;
    drive(&mut m, &mut rx, 8, |m| {
        ["db", "api", "web"]
            .iter()
            .all(|n| m.get_state(n) == Some(ServiceState::Running))
    })
    .await;
    assert_eq!(m.get_state("worker"), Some(ServiceState::Disabled));
    assert!(m.services["worker"].child.is_none(), "never spawned");

    m.write_status_file();
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(v["services"]["worker"]["state"], "disabled");
    assert_eq!(v["services"]["worker"]["deselected"], true);
    assert_eq!(v["services"]["web"]["deselected"], false);

    // Startable by hand (menu / palette / atlas_start_service): joins the run.
    m.restart_service("worker").await;
    assert_eq!(m.get_state("worker"), Some(ServiceState::Starting));
    assert!(!m.is_deselected("worker"));
    m.stop_all().await;
}

/// `--no-deps`: the dependency is off and never gates the selected service.
#[tokio::test]
async fn no_deps_leaves_dependency_off_and_does_not_gate() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager_selected(
        &dir,
        vec![
            ("db", svc("sleep 30")),
            (
                "api",
                ServiceDef {
                    depends_on: deps(&[("db", "healthy")]),
                    ..svc("sleep 30")
                },
            ),
        ],
        &["api"],
        true,
    );
    assert_eq!(m.get_state("db"), Some(ServiceState::Disabled));
    m.start_app_services(&names(&["api"])).await;
    assert_eq!(m.get_state("api"), Some(ServiceState::Starting));
    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("api") == Some(ServiceState::Running)
    })
    .await;
    assert_eq!(m.get_state("api"), Some(ServiceState::Running));
    assert_eq!(m.get_state("db"), Some(ServiceState::Disabled));
    m.stop_all().await;
}

/// Item 5: exit code, restart count and uptime land in the status file.
#[tokio::test]
async fn status_file_carries_exit_code_restarts_and_uptime() {
    let dir = tempfile::tempdir().unwrap();
    let (mut m, mut rx) = manager(&dir, vec![("boom", svc("exit 7")), ("ok", svc("sleep 30"))]);
    m.start_app_services(&names(&["boom", "ok"])).await;
    drive(&mut m, &mut rx, 8, |m| {
        m.get_state("boom") == Some(ServiceState::Failed)
            && m.get_state("ok") == Some(ServiceState::Running)
    })
    .await;
    m.write_status_file();
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(v["services"]["boom"]["exit_code"], 7);
    assert!(v["services"]["boom"]["uptime_s"].is_null());
    assert!(v["services"]["ok"]["exit_code"].is_null());
    assert!(v["services"]["ok"]["uptime_s"].as_u64().is_some());
    assert!(v["services"]["ok"]["started_at"].as_u64().is_some());
    assert_eq!(v["services"]["ok"]["restarts"], 0);
    m.stop_all().await;
}
