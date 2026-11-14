//! Automations as the apps see them: triggers in `crewmate_list`, `last_fired` kept on replace, and
//! the `add_trigger` action a crewmate proposes and the user approves.

use std::time::Duration;

use serde_json::json;

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin,
    wait_crewmate_triggers,
};

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_list_includes_triggers() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("h")).await;
    let crewmate_id = create_crewmate(&client, "AutoMate").await;

    client
        .call("crewmate_triggers_set", json!({"id": crewmate_id, "triggers": [
            {"id": "t1", "kind": {"type": "schedule", "cron": "0 9 * * *"}, "enabled": true, "prompt": "run"}
        ]}))
        .await
        .unwrap();

    let crewmate = wait_crewmate_triggers(&client, &crewmate_id, Duration::from_secs(5)).await;
    let triggers = crewmate["triggers"].as_array().unwrap();
    assert_eq!(triggers.len(), 1, "crewmate_list must return triggers");
    assert_eq!(triggers[0]["id"], "t1");
    assert!(
        crewmate["triggers"][0].get("next_fire").is_some(),
        "trigger must have next_fire"
    );

    client.call("shutdown", json!({})).await.ok();
}

// ── replacing the list keeps last_fired of an existing id ─────────────────
// Incident: every crewmate_triggers_set overwrote last_fired, resetting the fire history.

#[tokio::test(flavor = "multi_thread")]
async fn triggers_set_preserves_last_fired() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("h")).await;
    let crewmate_id = create_crewmate(&client, "AutoMate").await;

    // First set: one trigger.
    client
        .call("crewmate_triggers_set", json!({"id": crewmate_id, "triggers": [
            {"id": "t1", "kind": {"type": "interval", "every_secs": 3600}, "enabled": true, "prompt": "run"}
        ]}))
        .await
        .unwrap();

    // Read back last_fired (should be Some — baseline).
    let crewmate_v1 = wait_crewmate_triggers(&client, &crewmate_id, Duration::from_secs(5)).await;
    let lf1 = crewmate_v1["triggers"][0]["last_fired"].as_u64();
    assert!(
        lf1.is_some(),
        "new interval trigger must have a baseline last_fired"
    );

    // Second set: same id plus a new one — last_fired must survive.
    client
        .call("crewmate_triggers_set", json!({"id": crewmate_id, "triggers": [
            {"id": "t1", "kind": {"type": "interval", "every_secs": 3600}, "enabled": true, "prompt": "run2"},
            {"id": "t2", "kind": {"type": "schedule", "cron": "0 10 * * *"}, "enabled": true, "prompt": "run3"}
        ]}))
        .await
        .unwrap();

    // Wait until t2 appears.
    let start = std::time::Instant::now();
    let crewmate_v2 = loop {
        let v = client.call("crewmate_list", json!({})).await.unwrap();
        let crewmate = v["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == crewmate_id)
            .unwrap()
            .clone();
        if crewmate["triggers"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0)
            == 2
        {
            break crewmate;
        }
        if start.elapsed() > Duration::from_secs(5) {
            panic!("second triggers_set did not appear");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    let t1 = crewmate_v2["triggers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t1")
        .unwrap()
        .clone();
    let lf2 = t1["last_fired"].as_u64();
    assert_eq!(
        lf1, lf2,
        "last_fired of t1 must be preserved across triggers_set"
    );

    // t2 is new interval — must have a baseline last_fired too.
    let t2 = crewmate_v2["triggers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t2")
        .unwrap()
        .clone();
    assert!(
        t2["last_fired"].as_u64().is_some(),
        "new schedule trigger t2 must have baseline last_fired"
    );

    client.call("shutdown", json!({})).await.ok();
}

// ── Serialization: Trigger with tz/next_fire and TaskAction::AddTrigger ────
// Cross-boundary: the TS side reads these exact snake_case keys.

#[test]
fn trigger_tz_and_next_fire_serialize_to_snake_case() {
    use crate::crew::store::{Trigger, TriggerKind};
    let t = Trigger {
        id: "trig-1".into(),
        kind: TriggerKind::Schedule {
            cron: "0 9 * * 1-5".into(),
        },
        enabled: true,
        prompt: "Review PRs".into(),
        last_fired: Some(1730203200),
        tz: Some("America/Sao_Paulo".into()),
        next_fire: Some(1730289600),
    };
    let v: serde_json::Value = serde_json::to_value(&t).unwrap();
    assert_eq!(v["tz"], "America/Sao_Paulo", "tz must serialize as 'tz'");
    assert_eq!(
        v["next_fire"], 1730289600u64,
        "next_fire must serialize as 'next_fire'"
    );
    assert_eq!(v["last_fired"], 1730203200u64, "last_fired must serialize");
    // next_fire must NOT be read back from JSON (skip_deserializing).
    let raw = r#"{"id":"x","kind":{"type":"schedule","cron":"0 9 * * *"},"enabled":true,"prompt":"p","last_fired":null,"next_fire":9999}"#;
    let t2: Trigger = serde_json::from_str(raw).unwrap();
    assert_eq!(t2.next_fire, None, "next_fire must be skip_deserializing");
}

#[test]
fn add_trigger_action_serializes_to_snake_case_keys() {
    use crate::crew::store::{TaskAction, TriggerKind};
    let action = TaskAction::AddTrigger {
        trigger: TriggerKind::Schedule {
            cron: "0 9 * * 1-5".into(),
        },
        prompt: "Review the open PRs and summarize".into(),
        reason: "You asked for a daily PR review".into(),
    };
    let v: serde_json::Value = serde_json::to_value(&action).unwrap();
    assert_eq!(v["kind"], "add_trigger", "kind tag must be 'add_trigger'");
    assert!(v.get("trigger").is_some(), "trigger field must be present");
    assert!(v.get("prompt").is_some(), "prompt field must be present");
    assert!(v.get("reason").is_some(), "reason field must be present");
    assert_eq!(
        v["trigger"]["type"], "schedule",
        "trigger type must be snake_case"
    );
    assert_eq!(v["trigger"]["cron"], "0 9 * * 1-5");

    // Round-trip deserialization.
    let json_str = serde_json::to_string(&action).unwrap();
    let action2: TaskAction = serde_json::from_str(&json_str).unwrap();
    match action2 {
        TaskAction::AddTrigger {
            trigger: TriggerKind::Schedule { cron },
            prompt,
            reason,
        } => {
            assert_eq!(cron, "0 9 * * 1-5");
            assert_eq!(prompt, "Review the open PRs and summarize");
            assert_eq!(reason, "You asked for a daily PR review");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

// ── add_trigger: parsed, answered 'add', lands in crewmate.triggers ────────────
// Incident: add_trigger was not wired end-to-end.

#[tokio::test(flavor = "multi_thread")]
async fn add_trigger_add_answer_creates_trigger_with_task_tz() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("h");
    // Fake agent emits ATLAS_ACTION add_trigger then exits 0.
    let action = r#"ATLAS_ACTION: {"kind":"add_trigger","trigger":{"type":"schedule","cron":"0 9 * * 1-5"},"prompt":"Review PRs","reason":"daily review"}"#;
    let fake_bin = fake_agent(tmp.path(), "claude", &Hooks::new().done(action).script());
    use_agent_bin(&fake_bin);

    let client = daemon(&dir).await;
    let crewmate_id = create_crewmate(&client, "AutoMate").await;

    // Send task with tz so it gets stamped onto the trigger.
    let v = client
        .call("crewmate_task", json!({"id": crewmate_id, "prompt": "review PRs", "source": "test", "tz": "America/Sao_Paulo"}))
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();

    // Wait for WaitingAction.
    let start = std::time::Instant::now();
    let task = loop {
        let tv = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 5}))
            .await
            .unwrap();
        let t = tv["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task_id)
            .unwrap()
            .clone();
        if t["state"] == "waiting_action" {
            break t;
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("task never reached waiting_action (state={})", t["state"]);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(task["action"]["kind"], "add_trigger");
    assert_eq!(task["action"]["trigger"]["type"], "schedule");

    // Answer 'add'.
    client
        .call(
            "crewmate_action_answer",
            json!({"id": crewmate_id, "task": task_id, "value": "add"}),
        )
        .await
        .unwrap();

    // Wait for task to complete.
    let start = std::time::Instant::now();
    loop {
        let tv = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 5}))
            .await
            .unwrap();
        let t = tv["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task_id)
            .unwrap()
            .clone();
        if t["state"] != "waiting_action" && t["state"] != "running" && t["state"] != "queued" {
            break;
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("task never completed (state={})", t["state"]);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Check triggers were added.
    let crewmate = wait_crewmate_triggers(&client, &crewmate_id, Duration::from_secs(5)).await;
    let triggers = crewmate["triggers"].as_array().unwrap();
    assert_eq!(triggers.len(), 1, "add trigger must create one trigger");
    let t = &triggers[0];
    assert_eq!(t["kind"]["type"], "schedule");
    assert_eq!(t["kind"]["cron"], "0 9 * * 1-5");
    assert_eq!(
        t["tz"], "America/Sao_Paulo",
        "tz must be stamped from task.tz"
    );
    assert!(
        t["last_fired"].as_u64().is_some(),
        "trigger must have baseline last_fired"
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── add_trigger: 'skip' answer adds nothing ────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn add_trigger_skip_answer_adds_nothing() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("h");
    let action = r#"ATLAS_ACTION: {"kind":"add_trigger","trigger":{"type":"interval","every_secs":3600},"prompt":"hourly check","reason":"you asked"}"#;
    let fake_bin = fake_agent(tmp.path(), "claude", &Hooks::new().done(action).script());
    use_agent_bin(&fake_bin);

    let client = daemon(&dir).await;
    let crewmate_id = create_crewmate(&client, "AutoMate").await;

    let v = client
        .call(
            "crewmate_task",
            json!({"id": crewmate_id, "prompt": "check stuff", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();

    // Wait for WaitingAction.
    let start = std::time::Instant::now();
    loop {
        let tv = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 5}))
            .await
            .unwrap();
        let t = tv["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task_id)
            .unwrap()
            .clone();
        if t["state"] == "waiting_action" {
            break;
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("task never reached waiting_action");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Answer 'skip'.
    client
        .call(
            "crewmate_action_answer",
            json!({"id": crewmate_id, "task": task_id, "value": "skip"}),
        )
        .await
        .unwrap();

    // Wait for task to complete.
    let start = std::time::Instant::now();
    loop {
        let tv = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 5}))
            .await
            .unwrap();
        let t = tv["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task_id)
            .unwrap()
            .clone();
        if t["state"] != "waiting_action" && t["state"] != "running" && t["state"] != "queued" {
            break;
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("task never completed");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Check no triggers were added.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let v = client.call("crewmate_list", json!({})).await.unwrap();
    let crewmate = v["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == crewmate_id)
        .unwrap()
        .clone();
    let triggers = crewmate["triggers"].as_array().unwrap();
    assert!(triggers.is_empty(), "skip must not add any triggers");

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── parse_atlas_action: add_trigger JSON is correctly deserialized ─────────

#[test]
fn parse_atlas_action_add_trigger() {
    use crate::crew::runner::parse_atlas_action;
    use crate::crew::store::{TaskAction, TriggerKind};
    let text = r#"Some reply text.
ATLAS_ACTION: {"kind":"add_trigger","trigger":{"type":"schedule","cron":"0 9 * * 1-5"},"prompt":"Review the open PRs and summarize","reason":"You asked for a daily PR review"}"#;
    let action = parse_atlas_action(text).expect("should parse");
    match action {
        TaskAction::AddTrigger {
            trigger: TriggerKind::Schedule { cron },
            prompt,
            reason,
        } => {
            assert_eq!(cron, "0 9 * * 1-5");
            assert_eq!(prompt, "Review the open PRs and summarize");
            assert_eq!(reason, "You asked for a daily PR review");
        }
        other => panic!("unexpected action: {other:?}"),
    }
}
