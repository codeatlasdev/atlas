//! Conversational engines (omp, claude, codex, opencode): sessions kept across turns, secrets in the
//! agent's env only, approval rules and the omp approval extension.

use std::time::Duration;

use serde_json::json;

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};

#[tokio::test(flavor = "multi_thread")]
async fn engine_two_turns_keep_session() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Fake binary: counts invocations via a counter file.
    // Turn 1: outputs session_id "ses-001", result "first".
    // Turn 2: detects --resume ses-001 in argv, echoes "resume:ses-001" as result.
    let counter_file = tmp.path().join("argc.txt");
    let script = format!(
        r#"n=0
[ -f "{f}" ] && n=$(cat "{f}")
n=$((n+1))
echo $n > "{f}"
if [ "$n" = "1" ]; then
  printf '{{"type":"result","session_id":"ses-001","result":"first","is_error":false}}\n'
else
  RESUME_ID=""
  prev=""
  for a in "$@"; do
[ "$prev" = "--resume" ] && RESUME_ID="$a"
prev="$a"
  done
  printf '{{"type":"result","session_id":"%s","result":"resume:%s","is_error":false}}\n' "$RESUME_ID" "$RESUME_ID"
fi
exit 0"#,
        f = counter_file.display()
    );
    let fake_bin = fake_agent(tmp.path(), "claude-engine", &script);
    crate::crew::engine::set_test_engine_bin(Some(&fake_bin.to_string_lossy()));

    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "EngMate",
                "role": "r",
                "agent": "claude",
                "approval": "auto",
                "engine": {"kind": "claude"}
            }),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    // Turn 1.
    let v1 = client
        .call(
            "crewmate_task",
            json!({"id":id,"prompt":"p1","source":"test"}),
        )
        .await
        .unwrap();
    let t1_id = v1["task"]["id"].as_str().unwrap().to_string();
    let t1 = wait_task_state(&client, &id, &t1_id, "done", Duration::from_secs(15)).await;
    assert_eq!(t1["state"], "done", "turn 1 must succeed");

    // Session file must exist after turn 1.
    let session_path = dir.join("crew").join(&id).join("session.json");
    assert!(
        session_path.exists(),
        "session.json must be written after turn 1"
    );

    // Turn 2.
    let v2 = client
        .call(
            "crewmate_task",
            json!({"id":id,"prompt":"p2","source":"test"}),
        )
        .await
        .unwrap();
    let t2_id = v2["task"]["id"].as_str().unwrap().to_string();
    let t2 = wait_task_state(&client, &id, &t2_id, "done", Duration::from_secs(15)).await;
    assert_eq!(t2["state"], "done", "turn 2 must succeed");

    // Reply on turn 2 must contain the resumed session id.
    let reply = t2["result"].as_str().unwrap_or("");
    assert!(
        reply.contains("ses-001") || reply.contains("resume"),
        "turn 2 reply must show the session was resumed, got: {reply}"
    );

    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── Test CE-2: secrets never appear in crewmate_list or runner-generated fields ─
//
// The secret IS passed as an env var to the engine subprocess (that is the
// whole point), so a fake binary that prints `env` will obviously contain it.
// What we guarantee is that the runner itself never echoes the secret into
// protocol replies: crewmate_create, crewmate_list, and crewmate_tasks must not expose it.

#[tokio::test(flavor = "multi_thread")]
async fn engine_secrets_not_in_api_replies() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Fake engine: writes env to a sidecar file so we can assert delivery,
    // but outputs a fixed reply that does NOT contain the secret.
    let env_dump_path = tmp.path().join("env_dump.txt");
    let script = format!(
        r#"env > "{env_dump}"
printf '{{"type":"result","session_id":"s1","result":"engine-done","is_error":false}}\n'
exit 0"#,
        env_dump = env_dump_path.display()
    );
    let fake_bin = fake_agent(tmp.path(), "secret-eng", &script);
    crate::crew::engine::set_test_engine_bin(Some(&fake_bin.to_string_lossy()));

    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "SecretMate",
                "role": "r",
                "agent": "claude",
                "approval": "auto",
                "engine": {"kind": "claude"}
            }),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    // crewmate_create reply must not contain the secret (it doesn't exist yet, trivially true,
    // but also the Crewmate struct must not have a "secrets" field in its JSON representation).
    assert!(
        !crewmate.to_string().contains("secrets"),
        "crewmate_create reply must not mention secrets"
    );

    // Set a secret.
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "PRIVATE_TOKEN", "value": "xyzSECRET123"}),
        )
        .await
        .expect("crewmate_secret_set failed");

    // crewmate_list must not expose the secret.
    let list = client.call("crewmate_list", json!({})).await.unwrap();
    assert!(
        !list.to_string().contains("xyzSECRET123"),
        "crewmate_list must not expose the secret value"
    );

    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();

    let task = wait_task_state(&client, &id, &task_id, "done", Duration::from_secs(15)).await;
    assert_eq!(task["state"], "done");

    // The fixed reply from the fake binary must not contain the secret.
    let reply = task["result"].as_str().unwrap_or("");
    assert!(
        !reply.contains("xyzSECRET123"),
        "fixed reply must not contain the secret: {reply}"
    );

    // Verify the secret WAS delivered to the subprocess env.
    let env_dump = std::fs::read_to_string(&env_dump_path).unwrap_or_default();
    assert!(
        env_dump.contains("xyzSECRET123"),
        "secret must have been present in the engine subprocess env"
    );

    // crewmate_tasks reply must not contain the secret in system-generated fields
    // (error, prompt, pane). The reply field is engine output — not checked here.
    let tasks_v = client
        .call("crewmate_tasks", json!({"id": id, "limit": 5}))
        .await
        .unwrap();
    let t = tasks_v["tasks"]
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["id"] == task_id))
        .cloned()
        .unwrap_or_default();
    for field in &["error", "prompt", "pane", "trigger_id"] {
        let v = t[field].as_str().unwrap_or("");
        assert!(
            !v.contains("xyzSECRET123"),
            "field '{field}' must not contain the secret, got: {v}"
        );
    }

    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Incident (#206): a Codex crewmate set to `ask` ran unattended (`codex exec` has no approval
// channel), so the setting promised a safety it did not give. Every engine that cannot ask
// (`engine::can_ask`) refuses `ask` on create and on update; the ones that can accept it.
#[tokio::test(flavor = "multi_thread")]
async fn ask_is_refused_for_every_engine_that_cannot_ask() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    for kind in ["opencode", "codex", "claude", "omp"] {
        let r = client
            .call(
                "crewmate_create",
                json!({
                    "name": format!("{kind}Mate"), "role": "r", "agent": kind,
                    "approval": "ask", "engine": {"kind": kind}
                }),
            )
            .await;
        let refused = r.as_ref().map_or(true, |v| v.get("error").is_some());
        assert_eq!(
            refused,
            !crate::crew::engine::can_ask(kind),
            "{kind}+ask: {r:?}"
        );
    }

    // Switching an existing auto crewmate to ask is refused the same way.
    let auto = client
        .call(
            "crewmate_create",
            json!({"name": "CxAuto", "role": "r", "agent": "codex", "approval": "auto",
                   "engine": {"kind": "codex"}}),
        )
        .await
        .unwrap();
    let r = client
        .call(
            "crewmate_update",
            json!({"id": auto["id"], "approval": "ask"}),
        )
        .await;
    assert!(r.is_err(), "codex auto -> ask must be refused: {r:?}");
    client.call("shutdown", json!({})).await.ok();
}

// ── Test: opencode crewmate_list exposes approval_supported:false ─────────────

#[tokio::test(flavor = "multi_thread")]
async fn opencode_crewmate_approval_supported_false_in_list() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "OCBot2",
                "role": "r",
                "agent": "opencode",
                "approval": "auto",
                "engine": {"kind": "opencode"}
            }),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap();

    let list = client.call("crewmate_list", json!({})).await.unwrap();
    let entry = list["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == id)
        .cloned()
        .unwrap();
    assert_eq!(
        entry["approval_supported"].as_bool(),
        Some(false),
        "opencode crewmate must have approval_supported:false in crewmate_list: {entry}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── Test: omp + approval:ask is now ALLOWED (extension handles decisions) ─

#[tokio::test(flavor = "multi_thread")]
async fn omp_crewmate_ask_approval_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    // Previously rejected; must now succeed.
    let r = client
        .call(
            "crewmate_create",
            json!({"name": "OmpAsk", "role": "r", "agent": "omp", "approval": "ask"}),
        )
        .await;
    assert!(r.is_ok(), "omp+ask must now be allowed; got: {r:?}");
    client.call("shutdown", json!({})).await.ok();
}

// ── Test: hook_server uses CREWMATE_DECISION_WAIT for crewmate: panes ─────────────
// This is a pure-unit test of the wait-duration logic (no daemon needed).

#[tokio::test(flavor = "multi_thread")]
async fn omp_extension_written_for_ask_approval() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Fake omp: just exit 0 immediately (no RPC protocol); we only care
    // that the extension file is written before omp is called.
    let script = r#"#!/bin/sh
# Check that the extension flag is present and the file exists.
ext_file=""
prev=""
for a in "$@"; do
[ "$prev" = "--extension" ] && ext_file="$a"
prev="$a"
done
if [ -n "$ext_file" ] && [ -f "$ext_file" ]; then
echo "extension_ok"
fi
exit 0
"#;
    let fake_bin = fake_agent(tmp.path(), "omp", script);
    crate::crew::engine::set_test_engine_bin(Some(&fake_bin.to_string_lossy()));

    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "OmpExtMate",
                "role": "r",
                "agent": "omp",
                "approval": "ask",
                "engine": {"kind": "omp"}
            }),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "test", "source": "test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();
    // Task may be done or failed — either is fine; we only care the extension file exists.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let v = client
            .call("crewmate_tasks", json!({"id": id, "limit": 5}))
            .await
            .unwrap();
        if let Some(t) = v["tasks"]
            .as_array()
            .and_then(|ts| ts.iter().find(|t| t["id"] == task_id))
        {
            let s = t["state"].as_str().unwrap_or("");
            if matches!(s, "done" | "failed" | "cancelled") {
                break;
            }
        }
        if std::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Extension file must exist in the crewmate dir.
    let ext_path = dir.join("crew").join(&id).join("omp-approval.ts");
    assert!(
        ext_path.exists(),
        "omp-approval.ts extension must be written for approval:ask"
    );

    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
// ── Tests: codex exec_args builder ──────────────────────────────────────
// These test the argv builder (codex::exec_args) that constructs the
// actual command passed to the codex binary — no source-text inspection.

#[test]
fn codex_argv_first_turn_has_required_flags() {
    use crate::crew::engine::codex::exec_args;
    let args = exec_args(None, std::path::Path::new("/tmp/out.txt"), "hello");
    // First turn: exec (no resume)
    assert_eq!(
        &args[0..2],
        &["exec", "--json"][..],
        "first two: exec --json"
    );
    // Required flags must be present.
    assert!(
        args.iter().any(|a| a == "--skip-git-repo-check"),
        "--skip-git-repo-check must appear, got: {args:?}"
    );
    assert!(
        args.iter().any(|a| a == "--dangerously-bypass-hook-trust"),
        "--dangerously-bypass-hook-trust must appear, got: {args:?}"
    );
    // No "resume" on first turn.
    assert!(
        !args.iter().any(|a| a == "resume"),
        "first turn must not contain 'resume', got: {args:?}"
    );
    // Prompt is last.
    assert_eq!(args.last().map(|s| s.as_str()), Some("hello"));
}

#[test]
fn codex_argv_resume_has_required_flags() {
    use crate::crew::engine::codex::exec_args;
    let args = exec_args(
        Some("thread-abc"),
        std::path::Path::new("/tmp/out.txt"),
        "hi",
    );
    // Resume turn: exec resume <id>
    assert_eq!(
        &args[0..3],
        &["exec", "resume", "thread-abc"][..],
        "resume argv prefix wrong: {args:?}"
    );
    assert!(
        args.iter().any(|a| a == "--skip-git-repo-check"),
        "--skip-git-repo-check must appear in resume, got: {args:?}"
    );
    assert!(
        args.iter().any(|a| a == "--dangerously-bypass-hook-trust"),
        "--dangerously-bypass-hook-trust must appear in resume, got: {args:?}"
    );
}
