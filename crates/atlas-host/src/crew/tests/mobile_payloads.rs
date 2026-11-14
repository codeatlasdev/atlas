//! Contract with the phone's crew screens (`apps/mobile/lib/features/crew/{audit,usage,settings}`): the
//! replies of `crewmate_audit`, `crew_audit`, `crewmate_usage`, `engine_limits` and `crew_settings_get`
//! as a real hostd answers them, written to `apps/mobile/test/fixtures/` and read by the Dart model tests.
//! Volatile values (clock, random ids) are fixed before the comparison.
//! Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::mobile_payloads

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};

/// Compare `live` with `apps/mobile/test/fixtures/<name>`; `UPDATE_FIXTURES=1` rewrites the file first.
fn fixture(name: &str, live: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures")
        .join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{name} missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::mobile_payloads")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_fixtures_are_real_payloads() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let script = Hooks::new()
        .tool("Bash", json!({"command": "cargo test -p atlas-host"}))
        .done("ok")
        .script();
    use_agent_bin(&fake_agent(tmp.path(), "claude", &script));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Fixture").await;
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "GITHUB_TOKEN", "value": "ghp_fixture_value_1"}),
        )
        .await
        .unwrap();
    client
        .call("crewmate_update", json!({"id": id, "approval": "ask"}))
        .await
        .unwrap();
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "run the tests", "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, &id, &task, "done", Duration::from_secs(20)).await;
    client.call("crew_pause", json!({})).await.unwrap();
    client.call("crew_resume", json!({})).await.unwrap();
    // The task is `done` a beat before its `task_finished` entry is written: wait for the entry.
    for _ in 0..100 {
        let a = client
            .call("crewmate_audit", json!({"id": id, "kind": "task_finished"}))
            .await
            .unwrap();
        if !a["entries"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let mut mate = client
        .call("crewmate_audit", json!({"id": id}))
        .await
        .unwrap();
    let mut crew = client.call("crew_audit", json!({})).await.unwrap();
    for (reply, expect) in [
        (
            &mate,
            &[
                "task_queued",
                "task_started",
                "tool_call",
                "task_finished",
                "config_changed",
                "secret_set",
            ][..],
        ),
        (&crew, &["crewmate_created", "paused", "resumed"][..]),
    ] {
        let kinds: Vec<&str> = reply["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        for k in expect {
            assert!(kinds.contains(k), "no {k} entry in {kinds:?}");
        }
    }
    for reply in [&mut mate, &mut crew] {
        for (i, e) in reply["entries"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            e["ts"] = json!(1_700_000_000 + i as u64);
            if e["crewmate_id"].is_string() {
                e["crewmate_id"] = json!("crewmate-fixture");
            }
            if e["id"].is_string() {
                e["id"] = json!("crewmate-fixture");
            }
            if e["task_id"].is_string() {
                e["task_id"] = json!("task-fixture");
            }
        }
    }
    fixture("crewmate_audit.json", &mate);
    fixture("crew_audit.json", &crew);

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The test engine path (`run_fake`) reports no usage, so the record is appended by the writer the runner
// calls after a real turn (`CrewStore::append_usage`) and read back by the real `crewmate_usage`.
#[tokio::test(flavor = "multi_thread")]
async fn usage_limits_and_settings_fixtures_are_real_payloads() {
    use crate::crew::store::{CrewStore, UsageRecord};

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "UsageMate").await;
    let store = CrewStore::new(&dir);
    for (ts, input, output) in [(1_700_000_000, 1200, 340), (1_700_000_100, 800, 90)] {
        store
            .append_usage(
                &id,
                "claude",
                ts,
                &UsageRecord {
                    model: Some("claude-sonnet-4".into()),
                    input_tokens: Some(input),
                    output_tokens: Some(output),
                    cache_read: Some(9000),
                    cache_write: Some(150),
                    cost_usd: Some(0.0123),
                    duration_ms: Some(4200),
                },
            )
            .unwrap();
    }

    let mut usage = client
        .call("crewmate_usage", json!({"id": id, "since": 0}))
        .await
        .unwrap();
    let records = usage["records"].as_array_mut().unwrap();
    assert_eq!(records.len(), 2, "{records:?}");
    for r in records.iter_mut() {
        assert!(r["input_tokens"].is_number(), "input_tokens: {r}");
        r["crewmate_id"] = json!("crewmate-fixture");
    }
    fixture("crewmate_usage.json", &usage);

    let limits = client.call("engine_limits", json!({})).await.unwrap();
    assert!(limits["limits"].is_object(), "{limits}");
    fixture("engine_limits.json", &limits);

    // The default (nothing saved), then an explicit model: both shapes the settings screen reads.
    let default = client.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(default["short_replies"], true, "{default}");
    fixture("crew_settings.json", &default);
    client
        .call(
            "crew_settings_set",
            json!({"short_replies": false, "short_replies_model": "haiku"}),
        )
        .await
        .unwrap();
    let custom = client.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(custom["short_replies"], false, "{custom}");
    fixture("crew_settings_custom.json", &custom);

    client.call("shutdown", json!({})).await.ok();
}
