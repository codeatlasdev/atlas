//! Real services, real daemonized grandchildren: what `atlas dev` starts is tagged with its owner
//! and reaped by the tag when it stops or its run dies. Unix only: the shell one-liners build a
//! process that leaves its parent chain.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use atlas_core::hygiene::{self, Kind, Owner, Registry, SESSION_ENV};
use atlas_core::procs;

fn cfg(command: &str) -> ProjectConfig {
    let mut services = HashMap::new();
    services.insert(
        "svc".to_string(),
        ServiceDef {
            command: Some(command.to_string()),
            ..Default::default()
        },
    );
    ProjectConfig {
        name: "t".to_string(),
        services,
        ..Default::default()
    }
}

/// A service whose shell starts a double-forked `sleep` and writes its pid to `marker`, then runs
/// on itself. The subshell lingers for a moment before it exits (the `sleep 1`), so the `sleep`
/// is seen below the service's root first and is re-parented to init afterwards: a process that
/// daemonizes AFTER being seen, which is what the owner's recording covers on macOS (a process
/// that forks and detaches between two recordings escapes; see `atlas_core::hygiene`).
fn daemonizing(marker: &std::path::Path) -> String {
    format!(
        "( sleep 300 & echo $! > {}; sleep 1.5 ) & wait; exec sleep 300",
        marker.display()
    )
}

/// Record the run's roots' trees now (the periodic tick of `check_health`, done by hand).
fn track(mgr: &ServiceManager) {
    mgr.hygiene.track().unwrap();
}

async fn grandchild(marker: &std::path::Path) -> u32 {
    for _ in 0..200 {
        if let Ok(s) = std::fs::read_to_string(marker)
            && let Ok(p) = s.trim().parse::<u32>()
        {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the daemonized grandchild never started");
}

fn gone(pid: u32) -> bool {
    procs::start_time(pid).is_none()
        || std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().starts_with('Z'))
            .unwrap_or(false)
}

async fn wait_gone(pid: u32, within: Duration) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        if gone(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

// Incident (#127): stopping a service killed its parent-chain tree, but a grandchild that
// daemonized (a browser a test run opened, a dev server that forked away) kept running forever.
#[cfg(unix)]
#[tokio::test]
async fn stopping_a_service_ends_a_daemonized_grandchild_too() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("gc.pid");
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg(&daemonizing(&marker)), tx);
    mgr.start_service("svc").await;
    let gc = grandchild(&marker).await;
    track(&mgr);
    // The subshell exits and the grandchild leaves the parent chain.
    let root = mgr.services["svc"].status.pid.unwrap();
    let mut outside = false;
    for _ in 0..100 {
        if !procs::tree(root, &procs::table()).contains(&gc) {
            outside = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        outside,
        "premise: the grandchild is outside the parent chain"
    );
    assert!(
        mgr.hygiene.members().iter().any(|m| m.pid == gc),
        "the grandchild was recorded while it was below the service"
    );

    mgr.stop_service("svc").await;
    assert!(
        wait_gone(gc, Duration::from_secs(10)).await,
        "daemonized grandchild {gc} survived the stop"
    );
    assert!(
        mgr.hygiene
            .members()
            .iter()
            .all(|m| m.session != mgr.session_tag("svc"))
    );
}

// Stopping one service must not touch a sibling's processes (each has its own tag).
#[cfg(unix)]
#[tokio::test]
async fn stopping_one_service_leaves_the_tagged_processes_of_another_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (m_a, m_b) = (dir.path().join("a.pid"), dir.path().join("b.pid"));
    let mut config = cfg(&daemonizing(&m_a));
    config.services.insert(
        "other".into(),
        ServiceDef {
            command: Some(daemonizing(&m_b)),
            ..Default::default()
        },
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &config, tx);
    mgr.start_service("svc").await;
    mgr.start_service("other").await;
    let (a, b) = (grandchild(&m_a).await, grandchild(&m_b).await);
    track(&mgr);

    mgr.stop_service("svc").await;
    assert!(wait_gone(a, Duration::from_secs(10)).await);
    assert!(!gone(b), "the other service's grandchild must live");

    mgr.stop_service("other").await;
    assert!(wait_gone(b, Duration::from_secs(10)).await);
}

// A service `env:` could name ATLAS_SESSION itself: the manager's tag must win, or its tree
// would pose as another session (on Linux, where the tag is what finds it). The process's own
// environment tells it: the service writes what it sees.
#[cfg(unix)]
#[tokio::test]
async fn a_service_env_cannot_retag_its_tree() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("seen");
    let mut config = cfg(&format!(
        "echo \"$ATLAS_SESSION\" > {}; sleep 30",
        out.display()
    ));
    config
        .services
        .get_mut("svc")
        .unwrap()
        .env
        .insert(SESSION_ENV.to_string(), "someone-else".to_string());
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &config, tx);
    mgr.start_service("svc").await;
    let mut seen = String::new();
    for _ in 0..200 {
        seen = std::fs::read_to_string(&out).unwrap_or_default();
        if !seen.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    mgr.stop_service("svc").await;
    assert_eq!(seen.trim(), mgr.session_tag("svc"));
}

// The run itself dies (SIGKILL, crash): nothing stops its services. The registry still names the
// roots with a dead owner, and the next run's sweep ends the tagged tree, grandchild included.
#[cfg(unix)]
#[tokio::test]
async fn a_sweep_after_the_owner_run_died_ends_its_services_and_their_grandchildren() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("gc.pid");
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg(&daemonizing(&marker)), tx);
    let reg = Registry::at(&dir.path().join("reg"));
    mgr.set_hygiene_registry(reg.clone());
    mgr.start_service("svc").await;
    let gc = grandchild(&marker).await;
    track(&mgr);
    let tag = mgr.session_tag("svc");
    let roots = reg.roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        (roots[0].session.as_str(), roots[0].kind),
        (tag.as_str(), Kind::Service)
    );
    assert_eq!(roots[0].owner, Owner::current());

    // While the owner lives, a sweep leaves everything.
    let s = hygiene::sweep(&reg, false);
    assert!(s.orphans.is_empty(), "{:?}", s.orphans);
    assert!(!gone(gc));

    // The owner "dies": rewrite its record to a process that does not exist.
    let dead = Owner {
        pid: 0x7fff_fff0,
        start: 1,
    };
    reg.set_owner(std::slice::from_ref(&tag), dead).unwrap();
    let s = hygiene::sweep(&reg, false);
    assert!(s.orphans.iter().any(|o| o.pid == gc), "{:?}", s.orphans);
    assert!(wait_gone(gc, Duration::from_secs(10)).await);
    // The service's own shell died with it; drop the manager's stale handle.
    mgr.stop_service("svc").await;
}

// Incident (#127): a test-run action opened a browser that daemonized and outlived the action
// (and every run after it). The action's session ends it with the action.
#[cfg(unix)]
#[tokio::test]
async fn an_action_that_leaves_a_daemonized_process_behind_does_not_outlive_it() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("gc.pid");
    let (tx, _rx) = mpsc::unbounded_channel();
    let registry = Registry::at(&dir.path().join("reg"));
    let tag = "run-1:action/Test".to_string();
    let owner = super::actions::ActionOwner {
        session: tag.clone(),
        project: dir.path().to_string_lossy().into_owned(),
        registry: registry.clone(),
    };
    let ok = super::actions::run_action_streaming(
        // The shell waits for its subshell (1.5 s), so the action's own recording (every second)
        // sees the `sleep` below it before the action ends and leaves it to init.
        &format!(
            "( sleep 300 & echo $! > {}; sleep 1.5 ) & wait; echo done",
            marker.display()
        ),
        dir.path(),
        &HashMap::new(),
        &dir.path().join(".logs"),
        "Test",
        &tx,
        Some(owner),
    )
    .await;
    assert!(ok);
    let gc = grandchild(&marker).await;
    assert!(
        wait_gone(gc, Duration::from_secs(10)).await,
        "the action's daemonized process {gc} outlived it"
    );
    assert!(
        registry.roots().iter().all(|r| r.session != tag),
        "the finished action leaves no record"
    );
}
