//! Session actions: the host environment they get and the `session_ended` they emit.

use super::*;

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
    let line = tokio::time::timeout(atlas_core::scaling::HANG, async {
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

    // Tick until the health tick reaps it: `true` ends whenever the machine gets to it, not within a
    // fixed 300 ms.
    let end = std::time::Instant::now() + atlas_core::scaling::HANG;
    while mgr.session_active("Quick") && std::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(20)).await;
        mgr.check_health().await;
    }

    assert!(!mgr.session_active("Quick"));
    let mut ended = None;
    while let Ok(ev) = rx.try_recv() {
        if let ManagerEvent::SessionEnded { label, ok } = ev {
            ended = Some((label, ok));
        }
    }
    assert_eq!(ended, Some(("Quick".to_string(), true)));
}
