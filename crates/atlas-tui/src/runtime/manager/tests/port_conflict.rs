//! A port held by a foreign process (issue #133): who holds it, in the status file, MCP and the logs.

use super::*;

/// Not a test of its own: when `ATLAS_TEST_PORT_HOLDER` is set this binary, re-run by
/// [`foreign_holder`], becomes the stand-in for "a dev server somebody else started": it binds a
/// loopback port, prints it and blocks until its stdin closes. Without the variable it is a no-op.
#[test]
fn port_holder_helper() {
    use std::io::{Read, Write};
    if std::env::var_os("ATLAS_TEST_PORT_HOLDER").is_none() {
        return;
    }
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    println!("PORT={}", l.local_addr().unwrap().port());
    std::io::stdout().flush().unwrap();
    let _ = std::io::stdin().read(&mut [0u8; 1]);
}

/// A real, separate process listening on a real port. Owns the child: dropping it (at the end of
/// a test or while a failed assertion unwinds) ends the helper and reaps it.
struct Holder(std::process::Child);

impl Holder {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        drop(self.0.stdin.take()); // EOF on stdin: the helper returns and exits
        let _ = self.0.kill(); // in case it is stuck
        let _ = self.0.wait();
    }
}

fn foreign_holder() -> (Holder, u16) {
    use std::io::BufRead;
    let mut holder = Holder(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::manager::tests::port_conflict::port_holder_helper",
                "--nocapture",
            ])
            .env("ATLAS_TEST_PORT_HOLDER", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut out = std::io::BufReader::new(holder.0.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            out.read_line(&mut line).unwrap_or(0) > 0,
            "holder died before binding"
        );
        if let Some(p) = line.trim().strip_prefix("PORT=") {
            return (holder, p.parse().unwrap());
        }
    }
}

fn end_holder(holder: Holder) {
    drop(holder);
}

/// Incident (#133): an agent's dev server kept the port; the service failed with the framework's
/// own message and nobody could tell who held it without reading logs.
#[tokio::test]
async fn starting_on_a_port_held_by_another_process_names_the_holder() {
    let (holder, port) = foreign_holder();
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

    let c = mgr.port_conflict("svc").expect("conflict not detected");
    assert_eq!(
        c.pid,
        holder.id(),
        "must name the holder, not atlas or the test"
    );
    assert!(!c.cmd.is_empty());
    assert_eq!(mgr.get_state("svc"), Some(ServiceState::Failed));
    let note = mgr.service_note("svc").unwrap();
    assert!(
        note.starts_with(&format!("port {port} held by PID {}", holder.id())),
        "{note}"
    );
    assert!(
        note.ends_with(&format!(
            "stop it yourself ({}) or change the service port",
            if cfg!(windows) {
                format!("taskkill /PID {} /F", holder.id())
            } else {
                format!("kill {}", holder.id())
            }
        )),
        "{note}"
    );
    let log = std::fs::read_to_string(dir.path().join(".logs/svc.log")).unwrap();
    assert!(
        log.contains(&format!("held by PID {}", holder.id())),
        "{log}"
    );
    end_holder(holder);
}

/// The status file carries `port_conflict` as its own object (not folded into `note`), and the MCP
/// `atlas_status` tool returns what the real producer wrote.
#[tokio::test]
async fn the_status_file_and_mcp_status_carry_port_conflict_as_a_separate_field() {
    let (holder, port) = foreign_holder();
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("exit 1", Some(port)),
        tx,
    );
    mgr.start_service("svc").await;
    until_child_exited(&mut mgr, "svc").await;
    mgr.check_health().await; // writes .logs/.tui-status.json

    let file: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".logs/.tui-status.json")).unwrap(),
    )
    .unwrap();
    let pc = &file["services"]["svc"]["port_conflict"];
    assert_eq!(pc["pid"], holder.id());
    assert!(pc["cmd"].is_string() && pc.get("cwd").is_some(), "{pc}");

    let out = crate::mcp::tools::call(dir.path(), "atlas_status", &serde_json::json!({}))
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let row = &v["services"][0];
    assert_eq!(row["port_conflict"]["pid"], holder.id());
    assert!(row["note"].as_str().unwrap().contains("held by PID"));
    end_holder(holder);
}

/// A free port is no conflict, and the MCP field is an explicit `null` (stable shape).
#[tokio::test]
async fn a_free_port_reports_no_conflict_and_mcp_says_null() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("sleep 30", Some(port)),
        tx,
    );
    mgr.start_service("svc").await;
    mgr.check_health().await;
    assert!(mgr.port_conflict("svc").is_none());
    let out = crate::mcp::tools::call(dir.path(), "atlas_status", &serde_json::json!({}))
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["services"][0]["port_conflict"].is_null(), "{v}");
    mgr.stop_service("svc").await;
}

/// The conflict ends with the holder: the note must not keep blaming a process that is gone.
#[tokio::test]
async fn the_conflict_clears_when_the_holder_exits() {
    let (holder, port) = foreign_holder();
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
    assert!(mgr.port_conflict("svc").is_some());

    end_holder(holder);
    let deadline = std::time::Instant::now() + atlas_core::scaling::HANG;
    while mgr.port_conflict("svc").is_some() && std::time::Instant::now() < deadline {
        mgr.refresh_port_conflicts().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        mgr.port_conflict("svc").is_none(),
        "stale conflict after the holder exited"
    );
    assert_ne!(
        mgr.service_note("svc")
            .as_deref()
            .map(|n| n.contains("held by")),
        Some(true)
    );
}

/// Our own previous run is not "foreign": `.pids` roots (and their children) are `kill_orphans`'s
/// business, so they never show up as a conflict nor become releasable through atlas.
#[tokio::test]
async fn a_holder_recorded_in_pids_is_own_not_a_conflict() {
    let (holder, port) = foreign_holder();
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
    std::fs::write(
        crate::runtime::pid_file(dir.path()),
        holder.id().to_string(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("exit 1", Some(port)),
        tx,
    );
    mgr.detect_port_conflict("svc", port).await;
    assert!(mgr.port_conflict("svc").is_none());
    end_holder(holder);
}

/// Found reviewing #133: the announcement was first written before the spawn's log writers
/// rotated `<name>.log` to `.prev.log`, so it vanished from the live log. It must be in the log
/// of the spawn that hit the conflict, and a restart on the same holder must say it again.
#[tokio::test]
async fn the_conflict_line_survives_log_rotation_and_repeats_on_every_restart() {
    let (holder, port) = foreign_holder();
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("exit 1", Some(port)),
        tx,
    );
    let needle = format!("held by PID {}", holder.id());
    let live = |d: &std::path::Path| std::fs::read_to_string(d.join(".logs/svc.log")).unwrap();

    mgr.start_service("svc").await;
    until_child_exited(&mut mgr, "svc").await;
    assert!(
        live(dir.path()).contains(&needle),
        "first spawn: {}",
        live(dir.path())
    );

    // Second spawn on the same holder: the log is rotated again and must be told again.
    mgr.start_service("svc").await;
    until_child_exited(&mut mgr, "svc").await;
    assert!(
        live(dir.path()).contains(&needle),
        "restart: {}",
        live(dir.path())
    );
    let prev = std::fs::read_to_string(dir.path().join(".logs/svc.prev.log")).unwrap();
    assert!(
        prev.contains(&needle),
        "the previous spawn kept its own line"
    );
    end_holder(holder);
}

/// The live buffer (View logs, MCP-less TUI) gets the line too, after the restart marker.
#[tokio::test]
async fn the_conflict_line_reaches_the_live_log_stream() {
    let (holder, port) = foreign_holder();
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mgr = ServiceManager::new(
        dir.path().to_path_buf(),
        &one_service_cfg("exit 1", Some(port)),
        tx,
    );
    mgr.start_service("svc").await;
    let mut seen = false;
    while let Ok(ev) = rx.try_recv() {
        if let ManagerEvent::LogLine { line, stream, .. } = ev {
            seen |= line.contains(&format!("held by PID {}", holder.id()))
                && matches!(stream, LogStream::Stderr)
                && line.contains(": stop it yourself (")
                && line.ends_with(") or change the service port");
        }
    }
    assert!(
        seen,
        "no LogLine naming the holder and what the user can do"
    );
    end_holder(holder);
}
