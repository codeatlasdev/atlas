//! The append-only audit (#201) against a real hostd: what a task's tool calls, approvals, lifecycle
//! and the crew's configuration changes leave in `crewmate_audit` / `crew_audit`, that a secret never
//! shows, and that it is all still there after a restart.

use std::time::Duration;

use serde_json::{Value, json};

use super::approvals::decision;
use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use crate::HostClient;

async fn audit(client: &HostClient, id: &str, kind: Option<&str>) -> Vec<Value> {
    let mut args = json!({"id": id, "limit": 500});
    if let Some(k) = kind {
        args["kind"] = json!(k);
    }
    let v = client.call("crewmate_audit", args).await.unwrap();
    v["entries"].as_array().unwrap().clone()
}

async fn crew_audit(client: &HostClient) -> Vec<Value> {
    let v = client
        .call("crew_audit", json!({"limit": 500}))
        .await
        .unwrap();
    v["entries"].as_array().unwrap().clone()
}

async fn run_task(client: &HostClient, id: &str, prompt: &str) -> String {
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": prompt, "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(client, id, &task, "done", Duration::from_secs(20)).await;
    task
}

// The task step keeps 80 characters of a tool's input; the audit keeps all of it, with the full tool
// name, in the order the task went: queued, started, tool calls, finished.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_leaves_its_lifecycle_and_every_tool_call_with_full_input() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let long_cmd = format!("echo {}", "z".repeat(300));
    let script = Hooks::new()
        .tool(
            "mcp__github__create_issue",
            json!({"title": "bug", "command": long_cmd}),
        )
        .tool("Bash", json!({"command": "ls -la"}))
        .done("ok")
        .script();
    use_agent_bin(&fake_agent(tmp.path(), "claude", &script));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Audited").await;

    let task = run_task(&client, &id, "do the thing").await;

    let all = audit(&client, &id, None).await; // newest first
    let kinds: Vec<&str> = all
        .iter()
        .rev()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "task_queued",
            "task_started",
            "tool_call",
            "tool_call",
            "task_finished"
        ],
        "{all:#?}"
    );
    let tools = audit(&client, &id, Some("tool_call")).await;
    assert_eq!(tools[1]["tool_name"], "mcp__github__create_issue");
    assert_eq!(
        tools[1]["tool_input"]["command"],
        long_cmd.as_str(),
        "not cut at 80"
    );
    assert_eq!(tools[0]["tool_name"], "Bash");
    assert_eq!(tools[0]["task_id"], task.as_str());
    let fin = &audit(&client, &id, Some("task_finished")).await[0];
    assert_eq!(fin["state"], "done");
    assert_eq!(fin["task_id"], task.as_str());
    let queued = &audit(&client, &id, Some("task_queued")).await[0];
    assert_eq!(queued["source"], "test");
    assert_eq!(queued["prompt"], "do the thing");

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A value the crewmate holds as a secret never reaches the audit, even when the agent puts it in a
// command line; the entry says which secret it was. Setting one records its name, not the value.
#[tokio::test(flavor = "multi_thread")]
async fn a_secret_in_a_tool_input_is_redacted_and_setting_one_records_only_its_name() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let script = Hooks::new()
        .tool(
            "Bash",
            json!({"command": "curl -H 'Authorization: Bearer ghp_audit_secret_value_42' x"}),
        )
        .done("ok")
        .script();
    use_agent_bin(&fake_agent(tmp.path(), "claude", &script));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Leaky").await;
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "GITHUB_TOKEN", "value": "ghp_audit_secret_value_42"}),
        )
        .await
        .unwrap();

    run_task(&client, &id, "go").await;

    let all = client
        .call("crewmate_audit", json!({"id": id}))
        .await
        .unwrap()
        .to_string();
    assert!(
        !all.contains("ghp_audit_secret_value_42"),
        "secret in the reply: {all}"
    );
    let raw = std::fs::read_to_string(dir.join("crew").join(&id).join("audit.jsonl")).unwrap();
    assert!(
        !raw.contains("ghp_audit_secret_value_42"),
        "secret on disk: {raw}"
    );
    let tool = &audit(&client, &id, Some("tool_call")).await[0];
    assert!(
        tool["tool_input"]["command"]
            .as_str()
            .unwrap()
            .contains("Bearer [secret:GITHUB_TOKEN]"),
        "{tool}"
    );
    let set = &audit(&client, &id, Some("secret_set")).await[0];
    assert_eq!(set["name"], "GITHUB_TOKEN");
    assert!(set.get("value").is_none());

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// An approval is audited twice: asked (tool and input), then decided (allow or deny, and by whom).
// Answered over the socket, the answerer is the desktop. (A phone's node id and a Telegram chat are
// covered where the answer is made: agents::approval_done_says_how_it_ended_and_who_answered.)
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_is_audited_asked_and_decided_with_who_answered() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Asker").await;
    let pane = format!("crew:{id}:task-appr");

    // The prompt is held for the connected client; answer it as the desktop would.
    let d = dir.clone();
    let p = pane.clone();
    let held = tokio::spawn(async move { decision(&d, &p).await });
    let req = loop {
        let st = client.call("agent_status_all", json!({})).await.unwrap();
        if let Some(r) = st["pending_approvals"][0]["req"].as_str() {
            break r.to_string();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    client
        .call(
            "agent_approve",
            json!({"pane": pane, "req": req, "allow": false}),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&held.await.unwrap()).unwrap(),
        json!({"decision": "deny"})
    );

    let mut asked = Vec::new();
    for _ in 0..100 {
        asked = audit(&client, &id, Some("approval_decision")).await;
        if !asked.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let decided = &asked[0];
    assert_eq!(
        (decided["decision"].as_str(), decided["by"].as_str()),
        (Some("deny"), Some("desktop"))
    );
    assert_eq!(
        decided["tool_name"], "Bash",
        "the decision names the tool it was about"
    );
    assert_eq!(decided["task_id"], "task-appr");
    let q = &audit(&client, &id, Some("approval_asked")).await[0];
    assert_eq!(q["tool_input"]["command"], "ls");
    assert_eq!(q["req"], req.as_str());

    client.call("shutdown", json!({})).await.ok();
}

// Changes to what a crewmate may do are on record: approval mode, engine, triggers (webhook token
// kept out), secrets. Create and delete are crew-level so they outlive the crewmate; so are pause and resume.
#[tokio::test(flavor = "multi_thread")]
async fn configuration_changes_pause_and_the_crewmates_life_are_on_record() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Changer").await;

    client
        .call("crewmate_update", json!({"id": id, "approval": "ask"}))
        .await
        .unwrap();
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": [
                {"id": "wh", "kind": {"type": "webhook", "token": "tok-secret-audit", "allow_auto": true},
                 "enabled": true, "prompt": "p"}
            ]}),
        )
        .await
        .unwrap();
    let cfg = audit(&client, &id, Some("config_changed")).await;
    let approval = cfg
        .iter()
        .find(|e| e["field"] == "approval")
        .expect("approval change");
    assert_eq!(
        (approval["from"].as_str(), approval["to"].as_str()),
        (Some("auto"), Some("ask"))
    );
    let trig = cfg
        .iter()
        .find(|e| e["field"] == "triggers")
        .expect("trigger change");
    assert_eq!(
        trig["to"][0]["kind"]["allow_auto"], true,
        "what the trigger allows is recorded"
    );
    assert!(
        !trig.to_string().contains("tok-secret-audit"),
        "webhook token kept out: {trig}"
    );

    client.call("crew_pause", json!({})).await.unwrap();
    client.call("crew_pause", json!({})).await.unwrap(); // twice: one entry
    client.call("crew_resume", json!({})).await.unwrap();
    client
        .call("crewmate_delete", json!({"id": id}))
        .await
        .unwrap();

    let crew = crew_audit(&client).await;
    let kinds: Vec<&str> = crew
        .iter()
        .rev()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["crewmate_created", "paused", "resumed", "crewmate_deleted"],
        "{crew:#?}"
    );
    let deleted = &crew[0];
    assert_eq!(
        (deleted["id"].as_str(), deleted["name"].as_str()),
        (Some(id.as_str()), Some("Changer"))
    );
    // The crewmate's own log went with it, and asking for it says so instead of an empty list.
    assert!(
        client
            .call("crewmate_audit", json!({"id": id}))
            .await
            .is_err()
    );

    client.call("shutdown", json!({})).await.ok();
}

// A restarted hostd still serves what was written before it, and numbering carries on.
#[tokio::test(flavor = "multi_thread")]
async fn the_audit_is_still_there_after_a_restart() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let script = Hooks::new()
        .tool("Bash", json!({"command": "pwd"}))
        .done("ok")
        .script();
    use_agent_bin(&fake_agent(tmp.path(), "claude", &script));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Rebooter").await;
    run_task(&client, &id, "first").await;
    let before = audit(&client, &id, None).await;
    client.call("shutdown", json!({})).await.ok();
    drop(client);
    tokio::time::sleep(Duration::from_millis(800)).await;

    let client = daemon(&dir).await;
    let after = audit(&client, &id, None).await;
    assert_eq!(after, before, "same entries after the restart");
    run_task(&client, &id, "second").await;
    let more = audit(&client, &id, None).await;
    assert!(more.len() > before.len());
    assert!(
        more[0]["seq"].as_u64() > before[0]["seq"].as_u64(),
        "seq continues"
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// `since`, `limit` and `kind` on the wire; an unknown crewmate is an error, not an empty list.
#[tokio::test(flavor = "multi_thread")]
async fn the_read_command_filters_by_kind_since_and_limit() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let script = Hooks::new()
        .tool("A", json!({}))
        .tool("B", json!({}))
        .tool("C", json!({}))
        .done("ok")
        .script();
    use_agent_bin(&fake_agent(tmp.path(), "claude", &script));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Filter").await;
    run_task(&client, &id, "go").await;

    let two = client
        .call(
            "crewmate_audit",
            json!({"id": id, "kind": "tool_call", "limit": 2}),
        )
        .await
        .unwrap();
    let names: Vec<&str> = two["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["tool_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["C", "B"], "newest first, limited");
    let none = client
        .call(
            "crewmate_audit",
            json!({"id": id, "since": 4_000_000_000u64}),
        )
        .await
        .unwrap();
    assert!(none["entries"].as_array().unwrap().is_empty());
    assert!(
        client
            .call("crewmate_audit", json!({"id": "nope"}))
            .await
            .is_err()
    );
    assert!(
        client
            .call("crewmate_audit", json!({"id": "../x"}))
            .await
            .is_err()
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Contract with Atlas Desktop's Audit section (`apps/desktop/src/lib/crew/settings/audit.ts` reads
// this file): the real `crewmate_audit` reply of a crewmate that ran a task, was asked for an approval,
// had settings changed and a secret set, so every kind the section shows is in it. Entries get fixed
// ids/times (the clock and random ids are the only things that vary) before the comparison.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_audit_fixture_is_real_payload
#[tokio::test(flavor = "multi_thread")]
async fn crewmate_audit_fixture_is_real_payload() {
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
    run_task(&client, &id, "run the tests").await;
    // An approval, answered by the desktop.
    let pane = format!("crew:{id}:task-fx");
    let (d, p) = (dir.clone(), pane.clone());
    let held = tokio::spawn(async move { decision(&d, &p).await });
    let req = loop {
        let st = client.call("agent_status_all", json!({})).await.unwrap();
        if let Some(r) = st["pending_approvals"][0]["req"].as_str() {
            break r.to_string();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    client
        .call(
            "agent_approve",
            json!({"pane": pane, "req": req, "allow": true}),
        )
        .await
        .unwrap();
    held.await.unwrap();
    for _ in 0..100 {
        if !audit(&client, &id, Some("approval_decision"))
            .await
            .is_empty()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let mut live = client
        .call("crewmate_audit", json!({"id": id}))
        .await
        .unwrap();
    let kinds: std::collections::BTreeSet<&str> = live["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    for k in [
        "task_queued",
        "task_started",
        "tool_call",
        "task_finished",
        "approval_asked",
        "approval_decision",
        "config_changed",
        "secret_set",
    ] {
        assert!(
            kinds.contains(k),
            "the fixture lacks a {k} entry: {kinds:?}"
        );
    }
    let entries = live["entries"].as_array_mut().unwrap();
    // The contract the desktop reads: newest first.
    let seqs: Vec<u64> = entries.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] > w[1]),
        "entries are not newest first: {seqs:?}"
    );
    // Stable values for the clock, the random ids and the order. Hooks, the task's end and the
    // approval are written by different tasks, so how they interleave varies from run to run
    // (CI on PR #228 got another order than the fixture); the fixture pins the shapes, not that.
    for e in entries.iter_mut() {
        e["crewmate_id"] = json!("crewmate-fixture");
        e["req"]
            .is_string()
            .then(|| e["req"] = json!("req-fixture"));
        if e["task_id"].is_string() {
            e["task_id"] = json!("task-fixture");
        }
        e["ts"] = json!(0);
        e["seq"] = json!(0);
    }
    // Keys sorted at every level, so the order does not depend on serde_json's `preserve_order`.
    fn canon(v: &Value) -> String {
        match v {
            Value::Object(m) => {
                let mut kv: Vec<_> = m
                    .iter()
                    .map(|(k, v)| format!("{k:?}:{}", canon(v)))
                    .collect();
                kv.sort();
                format!("{{{}}}", kv.join(","))
            }
            Value::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
            v => v.to_string(),
        }
    }
    entries.sort_by_cached_key(canon);
    let n = entries.len() as u64;
    for (i, e) in entries.iter_mut().enumerate() {
        e["seq"] = json!(n - i as u64);
        e["ts"] = json!(1_700_000_000 + n - i as u64);
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/__fixtures__/crewmate_audit.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect(
        "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_audit_fixture_is_real_payload",
    ))
    .unwrap();
    assert_eq!(
        stored, live,
        "crewmate_audit.json changed: rerun with UPDATE_FIXTURES=1"
    );

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
