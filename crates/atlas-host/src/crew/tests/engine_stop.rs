//! An engine turn is a process tree the crew must be able to end: `crew_pause` and a per-task cancel
//! stop it (claude, codex, opencode, omp alike), and a turn that hits the task timeout leaves nothing
//! running. Each test runs the real engine module against a fake CLI that starts a `sleep` child and
//! records its pid (incident: #290, pause and the 2 h timeout left the agent running and spending).

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::HostClient;

/// A fake engine CLI: starts `sleep 60` as a child, records the child's pid in `pidfile`, and waits
/// for it (so killing only the CLI itself would orphan the `sleep`).
fn hanging_cli(dir: &Path, name: &str, pidfile: &Path) -> std::path::PathBuf {
    fake_agent(
        dir,
        name,
        &format!(
            "sleep 60 &\necho $! > \"{}\"\nwait\nexit 0",
            pidfile.display()
        ),
    )
}

/// The pid the fake CLI recorded, once it has.
async fn recorded_pid(pidfile: &Path) -> u32 {
    let start = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(pidfile)
            && let Ok(pid) = s.trim().parse::<u32>()
        {
            return pid;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "the fake engine never recorded its child's pid"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `pid` is gone: dead, or a zombie nobody reaped (a killed grandchild is reparented to init, which
/// may take a moment to reap it).
fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('Z')),
        Err(_) => !atlas_core::sys::pid_alive(pid),
    }
}

async fn wait_gone(pid: u32, within: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if gone(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    gone(pid)
}

/// The task stays `cancelled` (with the reason) after the killed turn has unwound: the turn's own
/// "cancelled" error reaches the runner a moment after the cancel was stored, and it used to overwrite
/// it with `failed` (`claude: cancelled`), which CI caught as a race on a loaded runner.
async fn stays_cancelled(client: &HostClient, crewmate: &str, task: &str, reason: Option<&str>) {
    // The runner is idle again (nothing running) once the turn has unwound.
    let start = Instant::now();
    loop {
        let list = client.call("crewmate_list", json!({})).await.unwrap();
        let me = list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == crewmate)
            .cloned()
            .unwrap();
        if me["state"] == "idle" {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the crewmate never went idle after the cancel: {me}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let v = client
        .call("crewmate_tasks", json!({"id": crewmate, "limit": 20}))
        .await
        .unwrap();
    let t = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .cloned()
        .unwrap();
    assert_eq!(t["state"], "cancelled", "{t}");
    match reason {
        Some(r) => assert_eq!(t["error"], r, "{t}"),
        None => assert!(t["error"].is_null(), "a user cancel has no error: {t}"),
    }
}

async fn engine_crewmate(client: &HostClient, kind: &str) -> String {
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Eng", "role": "r", "agent": kind, "approval": "auto",
                   "engine": {"kind": kind}}),
        )
        .await
        .unwrap();
    v["id"].as_str().unwrap().to_string()
}

async fn send(client: &HostClient, crewmate: &str) -> String {
    let v = client
        .call(
            "crewmate_task",
            json!({"id": crewmate, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    v["task"]["id"].as_str().unwrap().to_string()
}

struct Engine {
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Engine {
    /// Run `kind`'s real module with `bin` as its program.
    async fn use_program(kind: &str, bin: &Path) -> Self {
        let lock = lock_agent_bin().await;
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
            Some((kind.to_string(), bin.to_string_lossy().into_owned()));
        Self { _lock: lock }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
        *crate::crew::engine::TEST_TURN_TIMEOUT.lock() = None;
    }
}

async fn pause_stops_the_engine_turn(kind: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let pidfile = tmp.path().join("child.pid");
    let bin = hanging_cli(tmp.path(), kind, &pidfile);
    let _engine = Engine::use_program(kind, &bin).await;
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = engine_crewmate(&client, kind).await;

    let task = send(&client, &id).await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    let child = recorded_pid(&pidfile).await;
    assert!(!gone(child), "the engine's child should be running");

    client.call("crew_pause", json!({})).await.unwrap();
    let cancelled = wait_task_state(&client, &id, &task, "cancelled", Duration::from_secs(5)).await;
    assert_eq!(cancelled["error"], "paused", "{cancelled}");
    stays_cancelled(&client, &id, &task, Some("paused")).await;
    assert!(
        wait_gone(child, Duration::from_secs(5)).await,
        "{kind}: pause left the engine's child process {child} running"
    );
    client.call("shutdown", json!({})).await.ok();
}

// #290: `crew_pause` cancelled the task record but only killed a pty session; an engine turn kept
// running (and spending). Every engine's process tree must end within 5 s of the pause.
#[tokio::test(flavor = "multi_thread")]
async fn pause_ends_a_claude_turn_and_its_child() {
    pause_stops_the_engine_turn("claude").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_ends_a_codex_turn_and_its_child() {
    pause_stops_the_engine_turn("codex").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_ends_an_opencode_turn_and_its_child() {
    pause_stops_the_engine_turn("opencode").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_ends_an_omp_turn_and_its_child() {
    pause_stops_the_engine_turn("omp").await;
}

// A per-task cancel (not only the crew-wide pause) ends the turn's tree too.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_one_task_ends_its_engine_turn_and_child() {
    let tmp = tempfile::tempdir().unwrap();
    let pidfile = tmp.path().join("child.pid");
    let bin = hanging_cli(tmp.path(), "claude", &pidfile);
    let _engine = Engine::use_program("claude", &bin).await;
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = engine_crewmate(&client, "claude").await;

    let task = send(&client, &id).await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    let child = recorded_pid(&pidfile).await;

    let v: Value = client
        .call("crewmate_cancel", json!({"id": id, "task": task}))
        .await
        .unwrap();
    let _ = v;
    wait_task_state(&client, &id, &task, "cancelled", Duration::from_secs(5)).await;
    stays_cancelled(&client, &id, &task, None).await;
    assert!(
        wait_gone(child, Duration::from_secs(5)).await,
        "cancel left the engine's child process {child} running"
    );
    client.call("shutdown", json!({})).await.ok();
}

// #290: the 2 h `TASK_TIMEOUT` dropped the future without `kill_on_drop`, so the agent kept running.
// A turn that hits a (shortened) timeout fails the task and leaves no child behind.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_times_out_leaves_no_child_process() {
    for kind in ["claude", "codex", "opencode", "omp"] {
        let tmp = tempfile::tempdir().unwrap();
        let pidfile = tmp.path().join("child.pid");
        let bin = hanging_cli(tmp.path(), kind, &pidfile);
        let _engine = Engine::use_program(kind, &bin).await;
        *crate::crew::engine::TEST_TURN_TIMEOUT.lock() = Some(Duration::from_secs(2));
        let client = daemon(&tmp.path().join("hostd")).await;
        let id = engine_crewmate(&client, kind).await;

        let task = send(&client, &id).await;
        let child = recorded_pid(&pidfile).await;
        let start = Instant::now();
        loop {
            let v = client
                .call("crewmate_tasks", json!({"id": id, "limit": 5}))
                .await
                .unwrap();
            let t = v["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == task.as_str())
                .cloned()
                .unwrap();
            if t["state"] == "failed" {
                assert!(
                    t["error"].as_str().unwrap().contains("timed out"),
                    "{kind}: {t}"
                );
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "{kind}: the task never timed out: {t}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            wait_gone(child, Duration::from_secs(5)).await,
            "{kind}: the timeout left the engine's child process {child} running"
        );
        client.call("shutdown", json!({})).await.ok();
    }
}
