//! Issue #199: what a hostd restart (an upgrade, a reboot, a crash) does to tasks in flight. Every
//! test stops a real daemon and starts another on the same directory; the state the dead daemon left
//! is the last line of each task in `tasks.jsonl`, which `strand` writes exactly as the runner does.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use crate::HostClient;
use crate::crew::store::{AskRequest, CrewStore, Task, TaskAction, TaskState};

/// Stop `client`'s daemon and wait until its socket is gone, so the next `daemon` binds a new one.
async fn stop(client: HostClient) {
    client.call("shutdown", json!({})).await.ok();
    drop(client);
    tokio::time::sleep(Duration::from_millis(600)).await;
}

/// The tasks of `crewmate` as the store has them.
fn stored(dir: &Path, crewmate: &str, task: &str) -> Task {
    CrewStore::new(dir)
        .read_tasks(crewmate, 100)
        .into_iter()
        .find(|t| t.id == task)
        .expect("task in the store")
}

/// Leave `task` on disk as a daemon killed in the middle of it would have: appended in `state`.
fn strand(dir: &Path, crewmate: &str, task: &str, edit: impl FnOnce(&mut Task)) {
    let store = CrewStore::new(dir);
    let mut t = stored(dir, crewmate, task);
    edit(&mut t);
    store.append_task(crewmate, &t).unwrap();
}

async fn send(client: &HostClient, crewmate: &str, prompt: &str) -> String {
    let v = client
        .call(
            "crewmate_task",
            json!({"id": crewmate, "prompt": prompt, "source": "test"}),
        )
        .await
        .unwrap();
    v["task"]["id"].as_str().unwrap().to_string()
}

async fn task_json(client: &HostClient, crewmate: &str, task: &str) -> Value {
    let v = client
        .call("crewmate_tasks", json!({"id": crewmate, "limit": 50}))
        .await
        .unwrap();
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .cloned()
        .unwrap_or(Value::Null)
}

fn running(t: &mut Task) {
    t.state = TaskState::Running;
    t.started_at = Some(1_700_000_000);
    t.pane = Some(format!("crew:x:{}", t.id));
}

// Incident (#199): a deploy of the crew server marked every running task failed with "interrupted by
// daemon restart", losing work the user started. A terminal task must run again, same id, and finish.
#[tokio::test(flavor = "multi_thread")]
async fn a_running_terminal_task_runs_again_after_a_restart() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Phoenix").await;
    let task = send(&client, &id, "ship it").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    stop(client).await;
    strand(&dir, &id, &task, running);

    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("shipped").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;

    assert_eq!(done["result"], "shipped", "{done}");
    assert_eq!(done["restarts"], 1, "one interruption is counted: {done}");
    assert_eq!(
        done["resumed"], true,
        "the task says it was resumed: {done}"
    );
    assert!(
        done["steps"].as_array().is_some_and(|s| s
            .iter()
            .any(|s| s["text"].as_str().is_some_and(|t| t.contains("restarted")))),
        "a step says the server restarted: {done}"
    );
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The interrupted task goes back to the HEAD of its crewmate's queue: the task the user sent after it
// must not overtake it.
#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_task_runs_before_the_ones_queued_behind_it() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Orderly").await;
    let first = send(&client, &id, "first").await;
    wait_task_state(&client, &id, &first, "running", Duration::from_secs(15)).await;
    let second = send(&client, &id, "second").await;
    stop(client).await;
    // Stopping killed the agent and the dying worker may already have started `second`: leave the
    // log as a daemon killed mid-`first` would, with `second` still waiting behind it.
    strand(&dir, &id, &second, |t| {
        *t = Task {
            id: t.id.clone(),
            ts: t.ts,
            ..Task::new("test", "second")
        }
    });
    strand(&dir, &id, &first, running);

    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let a = wait_task_state(&client, &id, &first, "done", Duration::from_secs(20)).await;
    let b = wait_task_state(&client, &id, &second, "done", Duration::from_secs(20)).await;

    assert!(
        a["finished_at"].as_u64() <= b["started_at"].as_u64(),
        "the resumed task finished before the queued one started: {a} / {b}"
    );
    assert_eq!(b["restarts"].as_u64().unwrap_or(0), 0, "{b}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A task that crashes the daemon every time must not loop forever: after two interruptions the third
// restart fails it with a message that says why.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_interrupted_three_times_fails_with_a_clear_error() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Doomed").await;
    let task = send(&client, &id, "crash the box").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    stop(client).await;
    // It was already resumed twice and was running again when the daemon died a third time.
    strand(&dir, &id, &task, |t| {
        running(t);
        t.restarts = 2;
        t.resumed = true;
    });

    let client = daemon(&dir).await;
    let t = task_json(&client, &id, &task).await;

    assert_eq!(t["state"], "failed", "{t}");
    let err = t["error"].as_str().unwrap_or("");
    assert!(
        err.contains("restart") && err.contains('3'),
        "the error names the restarts that interrupted it: {err}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// One interruption below the cap is still resumed (the cap is not off by one).
#[tokio::test(flavor = "multi_thread")]
async fn a_task_interrupted_twice_is_still_resumed() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Stubborn").await;
    let task = send(&client, &id, "again").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    stop(client).await;
    strand(&dir, &id, &task, |t| {
        running(t);
        t.restarts = 1;
        t.resumed = true;
    });

    let quick = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("third time").script(),
    );
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;

    assert_eq!(done["restarts"], 2, "{done}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A paused crew resumes nothing on its own: the interrupted task is held (queued) until `crew_resume`.
#[tokio::test(flavor = "multi_thread")]
async fn a_paused_crew_holds_the_interrupted_task_until_resume() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Held").await;
    let task = send(&client, &id, "hold me").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    client.call("crew_pause", json!({})).await.unwrap();
    stop(client).await;
    strand(&dir, &id, &task, running);

    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("resumed").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let held = task_json(&client, &id, &task).await;
    assert_eq!(held["state"], "queued", "paused: nothing runs: {held}");

    client.call("crew_resume", json!({})).await.unwrap();
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;
    assert_eq!(done["result"], "resumed", "{done}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

/// A fake claude engine that records its argv (one line per turn) and replies `reply`.
fn recording_engine(tmp: &Path, reply: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let log = tmp.join("turns.log");
    let script = format!(
        "printf '%s\\n' \"$*\" >> '{log}'\n\
         printf '{{\"type\":\"result\",\"session_id\":\"ses-keep\",\"result\":\"{reply}\",\"is_error\":false}}\\n'\n\
         exit 0",
        log = log.display()
    );
    (fake_agent(tmp, "claude-engine", &script), log)
}

async fn engine_crewmate(client: &HostClient, name: &str) -> String {
    let v = client
        .call(
            "crewmate_create",
            json!({"name": name, "role": "r", "agent": "claude", "approval": "auto",
                   "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap();
    v["id"].as_str().unwrap().to_string()
}

// A conversational task resumes the agent's session (the one the next turn would use), with a short
// prompt saying the previous run was interrupted, instead of starting the conversation over.
#[tokio::test(flavor = "multi_thread")]
async fn a_running_conversational_task_resumes_its_session_after_a_restart() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let (bin, log) = recording_engine(tmp.path(), "first");
    crate::crew::engine::set_test_engine_bin(Some(&bin.to_string_lossy()));
    let client = daemon(&dir).await;
    let id = engine_crewmate(&client, "Sessioned").await;
    let t1 = send(&client, &id, "turn one").await;
    wait_task_state(&client, &id, &t1, "done", Duration::from_secs(15)).await;
    let t2 = send(&client, &id, "turn two").await;
    wait_task_state(&client, &id, &t2, "done", Duration::from_secs(15)).await;
    stop(client).await;
    // The daemon died while turn two was running (its session was saved by turn one).
    strand(&dir, &id, &t2, |t| {
        running(t);
        t.finished_at = None;
        t.result = None;
        t.reply = None;
    });
    std::fs::remove_file(&log).unwrap();

    let client = daemon(&dir).await;
    let done = wait_task_state(&client, &id, &t2, "done", Duration::from_secs(20)).await;

    assert_eq!(done["resumed"], true, "{done}");
    let turns = std::fs::read_to_string(&log).unwrap();
    let turn = turns.lines().next().expect("the engine ran once");
    assert!(
        turn.starts_with("--resume ses-keep "),
        "the stored session id is passed to the engine: {turn}"
    );
    assert!(
        turn.contains("interrupted"),
        "the prompt says the previous run was interrupted: {turn}"
    );
    assert!(
        turn.contains("turn two"),
        "and carries the task so the agent knows what it was doing: {turn}"
    );
    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

fn input_action() -> TaskAction {
    TaskAction::Input {
        label: "Which branch?".into(),
        reason: "need it".into(),
        placeholder: None,
    }
}

// A task waiting for the user stays waiting (its card is on the task) and the answer still reaches
// it after the restart: a terminal agent runs again with the answer, as it would have without one.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_action_task_can_be_answered_after_a_restart() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Patient").await;
    let task = send(&client, &id, "needs input").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    stop(client).await;
    strand(&dir, &id, &task, |t| {
        running(t);
        t.state = TaskState::WaitingAction;
        t.action = Some(input_action());
        t.result = Some("Which branch should I use?".into());
    });

    let quick = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("used main").script(),
    );
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    // Still waiting, card intact, and nothing ran on its own.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let waiting = task_json(&client, &id, &task).await;
    assert_eq!(waiting["state"], "waiting_action", "{waiting}");
    assert_eq!(waiting["action"]["kind"], "input", "{waiting}");

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "main"}),
        )
        .await
        .unwrap();
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;
    assert_eq!(done["result"], "used main", "{done}");
    assert_eq!(done["answer"], "main", "{done}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The same for a conversational crewmate: the answer becomes the next turn on the stored session.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_action_conversational_task_continues_its_session_when_answered() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let (bin, log) = recording_engine(tmp.path(), "first");
    crate::crew::engine::set_test_engine_bin(Some(&bin.to_string_lossy()));
    let client = daemon(&dir).await;
    let id = engine_crewmate(&client, "Chatty").await;
    let t1 = send(&client, &id, "ask me").await;
    wait_task_state(&client, &id, &t1, "done", Duration::from_secs(15)).await;
    stop(client).await;
    strand(&dir, &id, &t1, |t| {
        running(t);
        t.state = TaskState::WaitingAction;
        t.action = Some(input_action());
        t.finished_at = None;
        t.result = None;
    });
    std::fs::remove_file(&log).unwrap();

    let client = daemon(&dir).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        task_json(&client, &id, &t1).await["state"],
        "waiting_action"
    );
    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": t1, "value": "develop"}),
        )
        .await
        .unwrap();
    let done = wait_task_state(&client, &id, &t1, "done", Duration::from_secs(20)).await;

    assert_eq!(done["answer"], "develop", "{done}");
    let turns = std::fs::read_to_string(&log).unwrap();
    let turn = turns.lines().next().expect("one turn ran");
    assert!(turn.starts_with("--resume ses-keep "), "{turn}");
    assert!(turn.ends_with("develop"), "the answer is the turn: {turn}");
    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A pending `ask_user` question belonged to an agent process that is gone: it must not stay open on
// the task for someone to answer into the void. It is closed with a step that says why.
#[tokio::test(flavor = "multi_thread")]
async fn an_open_ask_is_closed_with_a_step_after_a_restart() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Asker").await;
    let task = send(&client, &id, "ask something").await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;
    stop(client).await;
    strand(&dir, &id, &task, |t| {
        running(t);
        t.ask = Some(AskRequest {
            id: "ask-1".into(),
            question: "Deploy now?".into(),
            options: vec![],
            allow_text: true,
            answered: None,
        });
    });

    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;

    assert!(
        done.get("ask").is_none_or(Value::is_null),
        "the dead agent's question is gone from the task: {done}"
    );
    let steps = done["steps"].to_string();
    assert!(
        steps.contains("question") && steps.contains("restart"),
        "a step explains the question was dropped by the restart: {steps}"
    );
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Tasks that were not in flight are left alone: a finished task is not touched by recovery.
#[tokio::test(flavor = "multi_thread")]
async fn finished_tasks_are_not_resumed() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("fine").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Tidy").await;
    let task = send(&client, &id, "one").await;
    wait_task_state(&client, &id, &task, "done", Duration::from_secs(15)).await;
    stop(client).await;

    let client = daemon(&dir).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let t = task_json(&client, &id, &task).await;
    assert_eq!(t["state"], "done", "{t}");
    assert!(t.get("resumed").is_none_or(|v| v == false), "{t}");
    assert!(t.get("restarts").is_none_or(|v| v == 0), "{t}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
