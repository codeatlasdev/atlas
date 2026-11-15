//! Automations that must not loop, and a delete that must not leave anything behind: chain triggers
//! stop at a crewmate already in the chain, GitHub dedupe survives an unwritable seen file and a new
//! trigger starts at the head of the feed, and a deleted crewmate's directory is not recreated by
//! the turn that was still running (issues #294 and #297).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use super::github_fake::FakeGithub;
use crate::HostClient;
use crate::crew::github::TEST_GITHUB_API;
use crate::crew::triggers::TEST_GITHUB_POLL;

async fn tasks_of(client: &HostClient, id: &str) -> Vec<Value> {
    client
        .call("crewmate_tasks", json!({"id": id, "limit": 50}))
        .await
        .unwrap()["tasks"]
        .as_array()
        .unwrap()
        .clone()
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn set_triggers(client: &HostClient, id: &str, triggers: Value) {
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": triggers}),
        )
        .await
        .unwrap();
}

// ── chain triggers ───────────────────────────────────────────────────────────

// Regression (#294): two crewmates triggering each other on `done` ping-ponged forever.
#[tokio::test(flavor = "multi_thread")]
async fn mutual_done_triggers_run_one_round_and_the_refusal_is_audited() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);
    let client = daemon(&tmp.path().join("hostd")).await;
    let a = create_crewmate(&client, "Aa").await;
    let b = create_crewmate(&client, "Bb").await;
    for (me, other) in [(&a, &b), (&b, &a)] {
        set_triggers(
            &client,
            me,
            json!([{"id": "chain", "kind": {"type": "crewmate", "from_crewmate": other, "on": "done"},
                    "enabled": true, "prompt": "{{event}}"}]),
        )
        .await;
    }

    let v = client
        .call(
            "crewmate_task",
            json!({"id": a, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let first = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, &a, &first, "done", Duration::from_secs(15)).await;

    // B runs once, because of A. B's `done` would start A again: refused, in the audit.
    let refused = until("the refusal in A's audit", async || {
        let v = client
            .call(
                "crewmate_audit",
                json!({"id": a, "kind": "chain_refused", "limit": 20}),
            )
            .await
            .unwrap();
        v["entries"].as_array().unwrap().first().cloned()
    })
    .await;
    assert_eq!(refused["from_crewmate"], b.as_str(), "{refused}");
    assert_eq!(refused["chain"], json!([a, b]), "{refused}");
    // Give a loop time to show itself.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(tasks_of(&client, &a).await.len(), 1, "A ran once");
    let b_tasks = tasks_of(&client, &b).await;
    assert_eq!(b_tasks.len(), 1, "B ran once: {b_tasks:?}");
    assert_eq!(b_tasks[0]["trigger_id"], "chain");

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A chain that never repeats a crewmate still runs to its end: A -> B -> C.
#[tokio::test(flavor = "multi_thread")]
async fn a_chain_without_a_repeat_runs_to_the_end() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);
    let client = daemon(&tmp.path().join("hostd")).await;
    let a = create_crewmate(&client, "Aa").await;
    let b = create_crewmate(&client, "Bb").await;
    let c = create_crewmate(&client, "Cc").await;
    for (me, from) in [(&b, &a), (&c, &b)] {
        set_triggers(
            &client,
            me,
            json!([{"id": "chain", "kind": {"type": "crewmate", "from_crewmate": from, "on": "done"},
                    "enabled": true, "prompt": "{{event}}"}]),
        )
        .await;
    }
    client
        .call(
            "crewmate_task",
            json!({"id": a, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task = until("C's task", async || {
        tasks_of(&client, &c).await.into_iter().next()
    })
    .await;
    assert_eq!(task["trigger_id"], "chain");

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── GitHub dedupe ────────────────────────────────────────────────────────────

fn mention(event_id: &str, comment_id: u64) -> Value {
    json!({"id": event_id, "type": "IssueCommentEvent", "payload": {
        "action": "created",
        "issue": {"number": 7, "title": "Crash", "user": {"login": "someone"}},
        "comment": {"id": comment_id, "body": "@nami please fix it", "user": {"login": "dev"}}}})
}

fn feed_api() -> Arc<FakeGithub> {
    let f = Arc::new(FakeGithub::new("nami-bot"));
    f.issues.lock().insert(
        7,
        json!({"number": 7, "title": "Crash", "body": "b", "author_association": "MEMBER",
               "user": {"login": "someone"}}),
    );
    for id in [100u64, 101] {
        f.comments.lock().insert(
            id,
            json!({"id": id, "body": "@nami please fix it", "author_association": "COLLABORATOR",
                   "user": {"login": "dev"}}),
        );
    }
    f
}

async fn watch(client: &HostClient, id: &str) {
    set_triggers(
        client,
        id,
        json!([{"id": "gh", "kind": {"type": "github", "repo": "o/r", "events": ["mention"], "handle": "nami"},
                "enabled": true, "prompt": "Handle: {{event}}"}]),
    )
    .await;
}

fn seen_file(dir: &std::path::Path, id: &str) -> std::path::PathBuf {
    dir.join("crew").join(id).join("gh_seen.jsonl")
}

// Regression (#294): enabling a GitHub trigger on a repo with old mentions answered all of them.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_github_trigger_starts_at_the_head_of_the_feed() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = feed_api();
    api.events
        .lock()
        .extend([mention("old-1", 100), mention("old-2", 100)]);
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch(&client, &id).await;

    let seen = seen_file(&dir, &id);
    until("the baseline", async || seen.exists().then_some(())).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        tasks_of(&client, &id).await.is_empty(),
        "old mentions started a task"
    );

    // An event that appears after the baseline is the trigger's to answer, once.
    api.events.lock().insert(0, mention("new-1", 101));
    until("the new mention's task", async || {
        (!tasks_of(&client, &id).await.is_empty()).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(tasks_of(&client, &id).await.len(), 1);

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Regression (#294): `append_seen` swallowed its error, so an unwritable seen file re-fired the same
// event on every poll. Here the seen file cannot be opened for append (it is a directory).
#[tokio::test(flavor = "multi_thread")]
async fn an_unwritable_seen_file_never_fires_the_same_event_twice() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = feed_api();
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    // Not new: a seen file exists (here it cannot be written to).
    std::fs::create_dir_all(seen_file(&dir, &id)).unwrap();
    api.events.lock().push(mention("ev-1", 100));
    watch(&client, &id).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        tasks_of(&client, &id).await.is_empty(),
        "an event that could not be recorded as seen started a task"
    );

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── delete ───────────────────────────────────────────────────────────────────

// Regression (#297): delete removed `crew/<id>/` while the turn ran; the turn's last write recreated
// it without a `crewmate.json`.
#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_crewmate_with_a_running_turn_leaves_no_directory() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(30).done("late").script(),
    );
    use_agent_bin(&agent);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Doomed").await;
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(15)).await;

    client
        .call("crewmate_delete", json!({"id": id}))
        .await
        .unwrap();
    // The turn's end, and whatever it records, lands after the delete.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !dir.join("crew").join(&id).exists(),
        "a late write recreated crew/{id}/"
    );
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    assert!(
        list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != id.as_str())
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
