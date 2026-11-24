//! Actions: the in-flight guard, pausing services around one, tunnels, streamed and persisted output.

use super::*;

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
    // The background `sleep 600` holds the output pipe for ten minutes, ten times the hang ceiling: the
    // action can only return before the ceiling if it does not wait for the pipe holder. (Timing the
    // return failed on a loaded release run: 5.9 s against a 5 s bound.)
    let run = tokio::time::timeout(
        atlas_core::scaling::HANG,
        run_action_streaming(
            &format!("sleep 600 & echo $! > {}; echo done", pidfile.display()),
            dir.path(),
            &HashMap::new(),
            &dir.path().join(".logs"),
            "bg",
            &tx,
            None,
        ),
    )
    .await;
    // reap the stray sleep we started (exact pid we recorded)
    if let Ok(p) = std::fs::read_to_string(&pidfile)
        && let Ok(p) = p.trim().parse::<i32>()
    {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(p),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    assert_eq!(
        run,
        Ok(true),
        "drain is bounded: the pipe holder is not waited for"
    );
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
            None,
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
