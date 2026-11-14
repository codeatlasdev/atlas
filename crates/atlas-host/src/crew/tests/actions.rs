//! `ATLAS_ACTION` lines: parsed from a reply, parked as `waiting_action`, answered by the user.

use std::time::Duration;

use serde_json::json;

use super::fixture::{daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state};

// ── Test: parse_atlas_action — valid kinds ────────────────────────────────
#[test]
fn parse_atlas_action_valid_kinds() {
    use crate::crew::runner::parse_atlas_action;
    use crate::crew::store::TaskAction;
    let r = parse_atlas_action(
        "text\nATLAS_ACTION: {\"kind\":\"connect_github\",\"reason\":\"to clone x/y\"}\n",
    );
    assert!(matches!(&r, Some(TaskAction::ConnectGithub { reason }) if reason == "to clone x/y"));
    let r2 = parse_atlas_action(
        "ATLAS_ACTION: {\"kind\":\"provide_secret\",\"name\":\"KEY\",\"label\":\"L\",\"reason\":\"R\"}",
    );
    assert!(matches!(&r2, Some(TaskAction::ProvideSecret { name, .. }) if name == "KEY"));
    let r3 = parse_atlas_action(
        "ATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Q?\",\"options\":[\"A\",\"B\"]}",
    );
    assert!(matches!(&r3, Some(TaskAction::Choose { question, .. }) if question == "Q?"));
    let r4 = parse_atlas_action("ATLAS_ACTION: {\"kind\":\"sign_in_engine\",\"engine\":\"omp\"}");
    assert!(matches!(&r4, Some(TaskAction::SignInEngine { engine }) if engine == "omp"));
    // Last line wins
    let r5 = parse_atlas_action(
        "ATLAS_ACTION: {\"kind\":\"connect_github\",\"reason\":\"first\"}\nATLAS_ACTION: {\"kind\":\"sign_in_engine\",\"engine\":\"omp\"}",
    );
    assert!(matches!(&r5, Some(TaskAction::SignInEngine { .. })));
}

#[test]
fn parse_atlas_action_unknown_kind_falls_back_to_input() {
    use crate::crew::runner::parse_atlas_action;
    use crate::crew::store::TaskAction;
    let r = parse_atlas_action(
        "ATLAS_ACTION: {\"kind\":\"future_kind\",\"label\":\"My label\",\"reason\":\"why\"}",
    );
    assert!(matches!(&r, Some(TaskAction::Input { label, .. }) if label == "My label"));
}

#[test]
fn parse_atlas_action_invalid_json_returns_none() {
    use crate::crew::runner::parse_atlas_action;
    assert!(parse_atlas_action("ATLAS_ACTION: not_json").is_none());
    assert!(parse_atlas_action("no action here").is_none());
}

// ── Test: provide_secret value never in task JSON ─────────────────────────
#[tokio::test(flavor = "multi_thread")]
async fn provide_secret_value_never_in_task_json() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-secret");
    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name":"SecMate","role":"r","agent":"claude","approval":"auto"}),
        )
        .await
        .unwrap();
    let crewmate_id = crewmate["id"].as_str().unwrap().to_string();
    // Call crewmate_action_answer on a non-existent task (expects error).
    // The error message must not contain the secret value.
    let result = client
        .call(
            "crewmate_action_answer",
            json!({"id": crewmate_id, "task": "nonexistent", "value": "supersecret"}),
        )
        .await;
    let err_str = format!("{result:?}");
    assert!(
        !err_str.contains("supersecret"),
        "secret value leaked into error: {err_str}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── Test: task_action fixture serialization ────────────────────────────────
#[test]
fn task_action_fixture_is_real_payload() {
    use crate::crew::store::TaskAction;
    let actions: Vec<TaskAction> = vec![
        TaskAction::ConnectGithub {
            reason: "to clone owner/repo".to_string(),
        },
        TaskAction::ProvideSecret {
            name: "API_KEY".to_string(),
            label: "API Key".to_string(),
            reason: "needed for the API".to_string(),
        },
        TaskAction::Input {
            label: "Enter value".to_string(),
            reason: "configuration needed".to_string(),
            placeholder: Some("e.g. my-value".to_string()),
        },
        TaskAction::Choose {
            question: "Which deployment target?".to_string(),
            options: vec!["staging".to_string(), "production".to_string()],
        },
        TaskAction::SignInEngine {
            engine: "omp".to_string(),
        },
    ];
    let json = serde_json::to_string_pretty(&actions).expect("serialize actions");
    // The file the phone's contract test reads; a cwd-relative path made cargo (cwd = this crate) write and
    // check a stray copy under crates/atlas-host/apps, so the two could drift unseen.
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/task_actions.json");
    let fixture_path = fixture_path.as_path();
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(fixture_path.parent().unwrap()).ok();
        std::fs::write(fixture_path, &json).expect("write fixture");
        return;
    }
    if !fixture_path.exists() {
        panic!(
            "fixture missing — run UPDATE_FIXTURES=1 cargo test -p atlas-host task_action_fixture_is_real_payload to generate it"
        );
    }
    let stored = std::fs::read_to_string(fixture_path).expect("read fixture");
    let stored_v: serde_json::Value = serde_json::from_str(&stored).expect("parse stored");
    let live_v: serde_json::Value = serde_json::from_str(&json).expect("parse live");
    assert_eq!(
        stored_v, live_v,
        "fixture is out of date — run UPDATE_FIXTURES=1 cargo test -p atlas-host task_action_fixture_is_real_payload"
    );
}

// ── Test: github project not logged in → waiting_action ───────────────────
#[tokio::test(flavor = "multi_thread")]
async fn github_project_not_logged_in_yields_waiting_action() {
    let _bin_lock = lock_agent_bin().await;
    *crate::crew::runner::TEST_FORCE_GITHUB_AUTH.lock() = Some("to clone test/repo".to_string());
    *crate::crew::runner::TEST_GITHUB_LOGGED_IN.lock() = Some(false);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-gh-auth");
    let fake_bin = fake_agent(tmp.path(), "claude", "exit 0");
    use_agent_bin(&fake_bin);
    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name":"GhMate","role":"r","agent":"claude","approval":"auto"}),
        )
        .await
        .unwrap();
    let crewmate_id = crewmate["id"].as_str().unwrap().to_string();
    let v = client
        .call(
            "crewmate_task",
            json!({"id": crewmate_id, "prompt": "do stuff", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();
    let task = wait_task_state(
        &client,
        &crewmate_id,
        &task_id,
        "waiting_action",
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(task["action"]["kind"], "connect_github");
    assert_eq!(task["action"]["reason"], "to clone test/repo");
    // Now connect: set logged in and call resume.
    *crate::crew::runner::TEST_GITHUB_LOGGED_IN.lock() = Some(true);
    client
        .call("crew_github_connect_resume", json!({}))
        .await
        .unwrap();
    // Task should leave waiting_action state.
    let start = std::time::Instant::now();
    loop {
        let v2 = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 5}))
            .await
            .unwrap();
        let t = v2["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task_id)
            .unwrap()
            .clone();
        if t["state"] != "waiting_action" {
            break;
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("task never left waiting_action");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    *crate::crew::runner::TEST_FORCE_GITHUB_AUTH.lock() = None;
    *crate::crew::runner::TEST_GITHUB_LOGGED_IN.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
