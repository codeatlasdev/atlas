//! The kill switch: `crew_pause` stops every running task and holds every queue, no automation fires
//! while it is on, `crew_resume` starts the queues again, and it survives a restart of hostd.

use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use crate::HostClient;

async fn task_state(client: &HostClient, crewmate: &str, task: &str) -> Value {
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

async fn paused(client: &HostClient) -> bool {
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    list["paused"]
        .as_bool()
        .expect("crewmate_list has `paused`")
}

// Crew review P0, PR #106 comment 6874910355: nothing could stop every crewmate at once (only a
// per-task cancel or uninstalling the server). Pausing must cancel what runs, hold what is queued
// (a new task still queues), and resuming must start the queue.
#[tokio::test(flavor = "multi_thread")]
async fn pause_cancels_the_running_task_holds_the_queue_and_resume_starts_it() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    // The first task hangs; the one queued behind it would finish at once.
    let hang = fake_agent(tmp.path(), "claude", "sleep 60\nexit 0");
    use_agent_bin(&hang);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Stopper").await;

    let running = send(&client, &id, "first").await;
    let queued = send(&client, &id, "second").await;
    wait_task_state(&client, &id, &running, "running", Duration::from_secs(15)).await;
    assert!(!paused(&client).await);

    // Pause: the running task is cancelled with the reason, the queued one is untouched.
    assert_eq!(
        client.call("crew_pause", json!({})).await.unwrap(),
        json!({})
    );
    assert!(paused(&client).await);
    let cancelled =
        wait_task_state(&client, &id, &running, "cancelled", Duration::from_secs(15)).await;
    assert_eq!(cancelled["error"], "paused", "{cancelled}");

    // Nothing starts while paused, however long we wait; a new task queues behind the held one.
    let later = send(&client, &id, "third").await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    for t in [&queued, &later] {
        assert_eq!(task_state(&client, &id, t).await["state"], "queued", "{t}");
    }
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    let me = list["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id.as_str())
        .unwrap();
    assert_eq!(me["state"], "idle", "{me}");
    assert_eq!(me["queued"], 2, "{me}");

    // Resume: the queue runs (now with an agent that ends at once).
    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&quick);
    assert_eq!(
        client.call("crew_resume", json!({})).await.unwrap(),
        json!({})
    );
    assert!(!paused(&client).await);
    wait_task_state(&client, &id, &queued, "done", Duration::from_secs(20)).await;
    wait_task_state(&client, &id, &later, "done", Duration::from_secs(20)).await;

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Pausing twice, or resuming when not paused, is harmless (a phone button can be tapped twice).
#[tokio::test(flavor = "multi_thread")]
async fn pause_and_resume_are_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    client.call("crew_resume", json!({})).await.unwrap();
    assert!(!paused(&client).await);
    client.call("crew_pause", json!({})).await.unwrap();
    client.call("crew_pause", json!({})).await.unwrap();
    assert!(paused(&client).await);
    client.call("crew_resume", json!({})).await.unwrap();
    client.call("crew_resume", json!({})).await.unwrap();
    assert!(!paused(&client).await);
    client.call("shutdown", json!({})).await.ok();
}

// A trigger that would fire while paused is skipped, not queued (it would otherwise all go off at
// once on resume). A webhook is the trigger a test can fire for real: POST to the hook server.
#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_does_not_fire_while_paused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Quiet").await;
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": [
                {"id": "wh", "kind": {"type": "webhook", "token": "tok-pause"},
                 "enabled": true, "prompt": "hook: {{event}}"}
            ]}),
        )
        .await
        .unwrap();
    let status = |body: &'static str| {
        let dir = dir.clone();
        let id = id.clone();
        async move { super::untrusted::post_webhook(&dir, &id, "tok-pause", body).await }
    };

    client.call("crew_pause", json!({})).await.unwrap();
    let resp = status("while paused").await;
    assert!(resp.contains("204") || resp.contains("503"), "{resp}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let tasks = client
        .call("crewmate_tasks", json!({"id": id, "limit": 10}))
        .await
        .unwrap();
    assert_eq!(
        tasks["tasks"].as_array().unwrap().len(),
        0,
        "a paused crew must not queue a trigger's task: {tasks}"
    );

    // Resumed, the very same webhook fires.
    client.call("crew_resume", json!({})).await.unwrap();
    status("after resume").await;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = client
            .call("crewmate_tasks", json!({"id": id, "limit": 10}))
            .await
            .unwrap();
        if tasks["tasks"].as_array().unwrap().len() == 1 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "resumed webhook never fired"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    client.call("shutdown", json!({})).await.ok();
}

// Paused is stored in crew.json, so a hostd restart (an upgrade, a reboot) does not quietly start
// every crewmate again.
#[tokio::test(flavor = "multi_thread")]
async fn paused_survives_a_restart_of_hostd() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    client.call("crew_pause", json!({})).await.unwrap();
    let stored: Value =
        serde_json::from_slice(&std::fs::read(dir.join("crew.json")).unwrap()).unwrap();
    assert_eq!(stored["paused"], true, "{stored}");
    client.call("shutdown", json!({})).await.ok();
    drop(client);
    // The socket of the dead daemon must be gone before the next one binds it.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = daemon(&dir).await;
    assert!(paused(&client).await, "paused must survive the restart");
    client.call("shutdown", json!({})).await.ok();
}

// `crew_settings_set` rewrites crew.json from the settings it is given: it must not un-pause.
#[tokio::test(flavor = "multi_thread")]
async fn saving_crew_settings_does_not_resume_a_paused_crew() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    client.call("crew_pause", json!({})).await.unwrap();
    client
        .call("crew_settings_set", json!({"short_replies": false}))
        .await
        .unwrap();
    assert!(paused(&client).await, "settings save must keep paused");
    client.call("shutdown", json!({})).await.ok();
}

// A queued task is held across a restart while paused, and runs on resume (recovery must not start
// a worker for a paused crew).
#[tokio::test(flavor = "multi_thread")]
async fn a_queue_recovered_after_a_restart_stays_held_while_paused() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let quick = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&quick);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Held").await;
    client.call("crew_pause", json!({})).await.unwrap();
    let task = send(&client, &id, "wait for me").await;
    client.call("shutdown", json!({})).await.ok();
    drop(client);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = daemon(&dir).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(task_state(&client, &id, &task).await["state"], "queued");
    client.call("crew_resume", json!({})).await.unwrap();
    wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The push event clients get when the switch flips, built in `crew/events.rs`: the desktop reads
// this file. Real serializer output, never hand-written.
#[test]
fn crew_paused_event_fixture_is_real_payload() {
    use crate::crew::events;
    let on: Value = serde_json::from_str(&events::crew_paused(true)).unwrap();
    assert_eq!(on, json!({"ev": "crew_paused", "paused": true}));
    let off: Value = serde_json::from_str(&events::crew_paused(false)).unwrap();
    assert_eq!(off, json!({"ev": "crew_paused", "paused": false}));

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/__fixtures__/crew_paused_event.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&on).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&fixture_path).expect(
        "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crew_paused_event_fixture",
    ))
    .unwrap();
    assert_eq!(
        stored, on,
        "crew_paused_event.json changed: rerun with UPDATE_FIXTURES=1"
    );
}

// Clients learn of the switch by push, so a second client (a phone) sees what the first one did.
#[tokio::test(flavor = "multi_thread")]
async fn pausing_pushes_the_event_to_every_client() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let a = daemon(&dir).await;
    let b = HostClient::connect_dir(&dir, "second").await.unwrap();
    let mut events = b.events();
    a.call("crew_pause", json!({})).await.unwrap();
    let ev = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let e = events.recv().await.unwrap();
            if e["ev"] == "crew_paused" {
                return (*e).clone();
            }
        }
    })
    .await
    .expect("no crew_paused event reached the second client");
    assert_eq!(ev["paused"], true);
    a.call("crew_resume", json!({})).await.unwrap();
    let ev = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let e = events.recv().await.unwrap();
            if e["ev"] == "crew_paused" {
                return (*e).clone();
            }
        }
    })
    .await
    .expect("no crew_paused event for the resume");
    assert_eq!(ev["paused"], false);
    a.call("shutdown", json!({})).await.ok();
}
