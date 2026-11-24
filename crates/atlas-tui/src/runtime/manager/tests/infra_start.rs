//! Infra (`compose up`) and spawn failures: the reason reaches the card and every log.

use super::*;

#[cfg(unix)]
/// Seed the shared login-shell probe with a PATH that holds `bin` (a dir with a fake `docker`) and
/// nothing else that could provide a real one.
#[must_use = "the fake PATH is undone when the guard drops"]
fn seed_docker_path(bin: &std::path::Path) -> atlas_core::login_env::Installed {
    use atlas_core::login_env::{Probe, install};
    install(Probe {
        shell: "/bin/sh".into(),
        source: "default",
        flags: None,
        error: None,
        took_ms: 0,
        vars: vec![
            ("PATH".into(), format!("{}:/usr/bin:/bin", bin.display())),
            ("HOME".into(), bin.display().to_string()),
        ],
    })
}

/// A project dir with `infra:` (compose file present) and two command-less services, `postgres` on
/// `pg` and `redis` on `redis` (atlas.yaml rejects a shared port).
#[cfg(unix)]
fn infra_project(pg: u16, redis: u16) -> (tempfile::TempDir, ProjectConfig) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("compose.yml"), "services: {}\n").unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        format!(
            "name: infra\ninfra:\n  compose_file: compose.yml\nservices:\n  postgres:\n    port: {pg}\n  redis:\n    port: {redis}\n"
        ),
    )
    .unwrap();
    let cfg = crate::config::load(dir.path()).unwrap();
    (dir, cfg)
}

#[cfg(unix)]
/// A `docker` that behaves like a Linux box where the user is not in the `docker` group:
/// `docker info` and `docker compose …` both die on the socket.
fn fake_docker(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    std::fs::write(&docker, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// Drain events until `InfraReady`, applying what the real event loops apply.
#[cfg(unix)]
async fn run_until_infra_ready(
    mgr: &mut ServiceManager,
    rx: &mut mpsc::UnboundedReceiver<ManagerEvent>,
) -> Vec<ManagerEvent> {
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(90), async {
        while let Some(ev) = rx.recv().await {
            match &ev {
                ManagerEvent::StateChanged { name, state } => {
                    mgr.update_state(name, state.clone());
                }
                ManagerEvent::InfraFailed { names, reason } => mgr.record_failure(names, reason),
                _ => {}
            }
            let done = matches!(ev, ManagerEvent::InfraReady { .. });
            seen.push(ev);
            if done {
                break;
            }
        }
    })
    .await
    .expect("InfraReady never arrived");
    // `record_failure` queued its LogLines on the same channel; the real loops read them next.
    while let Ok(ev) = rx.try_recv() {
        seen.push(ev);
    }
    seen
}

/// Incident (Linux, 1.5 alpha): minio/postgres/redis all `failed` with "Failed to start" and View logs
/// "Waiting for output…": the docker failure was only a toast/alert, the card had no reason and no
/// service log was written. Every infra service must carry the reason in the card note AND in its log.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_compose_up_leaves_the_reason_on_the_card_and_in_every_infra_log() {
    let dir_bin = tempfile::TempDir::new().unwrap();
    let bin = fake_docker(
        dir_bin.path(),
        r#"if [ "$1" = info ]; then exit 1; fi
echo 'permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock: connect: permission denied' >&2
exit 1"#,
    );
    let _env = seed_docker_path(&bin);
    let (dir, cfg) = infra_project(1, 2);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_all().await;
    let events = run_until_infra_ready(&mut mgr, &mut rx).await;

    for name in ["postgres", "redis"] {
        assert_eq!(mgr.get_state(name), Some(ServiceState::Failed), "{name}");
        let note = mgr.service_note(name).unwrap_or_default();
        assert!(
            note.contains("usermod -aG docker"),
            "{name} card note: {note:?}"
        );
        let log = std::fs::read_to_string(dir.path().join(format!(".logs/{name}.log")))
            .unwrap_or_default();
        assert!(log.contains("usermod -aG docker"), "{name}.log: {log:?}");
        assert!(
            events.iter().any(|e| matches!(e,
                ManagerEvent::LogLine { name: n, line, .. } if n == name && line.contains("usermod"))),
            "{name}: the reason must also reach live View logs as a LogLine"
        );
    }
}

/// Same class: `docker` is not installed / not on the PATH a menu-launched app has. Unix only: on Windows the
/// process search also falls back to the parent's PATH and the system folders, so a runner's docker is found.
#[cfg(unix)]
#[tokio::test]
async fn a_missing_docker_binary_is_reported_not_silent() {
    let empty = tempfile::TempDir::new().unwrap();
    // PATH with no docker anywhere (and no /usr/bin: a real docker there would defeat the test),
    // but with `sh`: the login env is process-wide, so every other test that spawns `sh -c` while
    // this one runs resolves it on THIS path. An empty one made their spawns fail with ENOENT
    // ("failed to spawn: …", a stale failure reason), seen only when the windows overlapped.
    std::os::unix::fs::symlink("/bin/sh", empty.path().join("sh")).unwrap();
    let _env = atlas_core::login_env::install(atlas_core::login_env::Probe {
        shell: "/bin/sh".into(),
        source: "default",
        flags: None,
        error: None,
        took_ms: 0,
        vars: vec![("PATH".into(), empty.path().display().to_string())],
    });
    let (dir, cfg) = infra_project(1, 2);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_all().await;
    run_until_infra_ready(&mut mgr, &mut rx).await;

    let note = mgr.service_note("postgres").unwrap_or_default();
    assert!(note.contains("docker not found"), "card note: {note:?}");
    let log = std::fs::read_to_string(dir.path().join(".logs/postgres.log")).unwrap_or_default();
    assert!(log.contains("docker not found"), "postgres.log: {log:?}");
}

/// Compose succeeded but the port never opened (container crashed, other port mapped): the 30 s wait
/// also turned the service Failed with no reason at all.
#[cfg(unix)]
#[tokio::test]
async fn an_infra_port_that_never_opens_says_which_port_in_card_and_log() {
    let dir_bin = tempfile::TempDir::new().unwrap();
    let bin = fake_docker(dir_bin.path(), "exit 0");
    let _env = seed_docker_path(&bin);
    let (dir, cfg) = infra_project(1, 2);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_all().await;
    run_until_infra_ready(&mut mgr, &mut rx).await;

    let note = mgr.service_note("postgres").unwrap_or_default();
    assert!(note.contains(":1"), "card note: {note:?}");
    assert!(note.contains("30"), "the wait is named: {note:?}");
    let log = std::fs::read_to_string(dir.path().join(".logs/postgres.log")).unwrap_or_default();
    assert!(log.contains(":1"), "postgres.log: {log:?}");
}

/// A healthy infra service keeps a clean log: nothing is written when nothing failed.
#[cfg(unix)]
#[tokio::test]
async fn a_healthy_infra_start_writes_no_failure_lines() {
    let (l1, l2) = (
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    );
    let (p1, p2) = (
        l1.local_addr().unwrap().port(),
        l2.local_addr().unwrap().port(),
    );
    let dir_bin = tempfile::TempDir::new().unwrap();
    let bin = fake_docker(dir_bin.path(), "exit 0");
    let _env = seed_docker_path(&bin);
    let (dir, cfg) = infra_project(p1, p2);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_all().await;
    run_until_infra_ready(&mut mgr, &mut rx).await;

    assert_eq!(mgr.get_state("postgres"), Some(ServiceState::Running));
    assert!(mgr.fail_reason("postgres").is_none());
    assert!(!dir.path().join(".logs/postgres.log").exists());
    drop((l1, l2));
}

/// Same incident class (Failed, card says "Failed to start", View logs empty): `spawn()` itself fails
/// (here: `dir:` does not exist), so there is no process and no pipe to carry a reason.
#[tokio::test]
async fn a_service_that_cannot_be_spawned_says_why_in_card_and_log() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: t\nservices:\n  api:\n    command: echo hi\n    dir: does/not/exist\n",
    )
    .unwrap();
    let cfg = crate::config::load(dir.path()).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);

    mgr.start_service("api").await;

    assert_eq!(mgr.get_state("api"), Some(ServiceState::Failed));
    let note = mgr.service_note("api").unwrap_or_default();
    assert!(note.contains("could not start"), "card note: {note:?}");
    assert!(
        note.contains("does/not/exist"),
        "the directory is named: {note:?}"
    );
    let log = std::fs::read_to_string(dir.path().join(".logs/api.log")).unwrap_or_default();
    assert!(log.contains("could not start"), "api.log: {log:?}");
    let live = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(e, ManagerEvent::LogLine { name, line, .. } if name == "api" && line.contains("could not start"))
    });
    assert!(live, "View logs (live buffer) must get the line too");
}

/// An empty `command:` used to turn the service Failed with nothing said anywhere.
#[tokio::test]
async fn an_empty_command_says_so_in_card_and_log() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &one_service_cfg("   ", None), tx);

    mgr.start_service("svc").await;

    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Failed));
    let note = mgr.service_note("svc").unwrap_or_default();
    assert!(note.contains("empty"), "card note: {note:?}");
    let log = std::fs::read_to_string(dir.path().join(".logs/svc.log")).unwrap_or_default();
    assert!(log.contains("empty"), "svc.log: {log:?}");
}

/// A service that spawns fine keeps no leftover reason from an earlier failed start (restart after fix).
#[tokio::test]
async fn a_successful_respawn_clears_the_previous_failure_reason() {
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("sleep 30", None),
        tx,
    );
    mgr.record_failure(
        &["svc".to_string()],
        &crate::runtime::note::Note::plain("old reason"),
    );
    assert_eq!(mgr.fail_reason("svc").as_deref(), Some("old reason"));

    mgr.start_service("svc").await;

    assert!(
        mgr.fail_reason("svc").is_none(),
        "stale reason survived a new spawn"
    );
    mgr.stop_service("svc").await;
}
