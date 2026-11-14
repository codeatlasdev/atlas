//! The guards against the agent itself (#214), end to end on a real hostd: a fake agent edits a real
//! git work tree and reports `done`; a refused task parks on a `refused` card and the owner's
//! `override` or `retry` decides it. The checks themselves are tested in `runner/guard/tests.rs`.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{Hooks, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state};
use crate::HostClient;

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A project with one passing test and `origin/HEAD` on its only commit (the task's base).
fn project(root: &Path) -> std::path::PathBuf {
    let p = root.join("proj");
    std::fs::create_dir_all(p.join("src")).unwrap();
    git(&p, &["init", "-q", "-b", "main"]);
    std::fs::write(
        p.join("src/tests.rs"),
        "#[test]\nfn adds() {\n    assert_eq!(1 + 1, 2);\n    assert!(true);\n}\n",
    )
    .unwrap();
    git(&p, &["add", "-A"]);
    git(&p, &["commit", "-q", "-m", "base"]);
    git(&p, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(
        &p,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    p
}

async fn crewmate_on(client: &HostClient, proj: &Path) -> String {
    client
        .call(
            "crewmate_create",
            json!({"name": "Guarded", "role": "r", "agent": "claude", "approval": "auto",
                   "project": {"kind": "path", "path": proj.to_string_lossy()}}),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn send(client: &HostClient, id: &str, prompt: &str) -> String {
    client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": prompt, "source": "test"}),
        )
        .await
        .unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The agent weakens the test (drops an assert) and says it is finished and verified.
fn weakening_agent(root: &Path) -> std::path::PathBuf {
    let edit = "printf '#[test]\\nfn adds() {\\n    assert_eq!(1 + 1, 2);\\n}\\n' > src/tests.rs\n";
    let script = format!(
        "{edit}{}",
        Hooks::new()
            .done("Fixed the cart. All tests pass.")
            .script()
    );
    fake_agent(root, "claude", &script)
}

async fn task_of(client: &HostClient, id: &str, task: &str) -> Value {
    client
        .call("crewmate_tasks", json!({"id": id, "limit": 20}))
        .await
        .unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .unwrap()
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_weakened_test_parks_the_task_and_override_finishes_it_without_the_unproven_claim() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    use_agent_bin(&weakening_agent(tmp.path()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "fix the cart").await;

    let parked = wait_task_state(
        &client,
        &id,
        &task,
        "waiting_action",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(parked["action"]["kind"], "refused", "{parked}");
    let reasons = parked["action"]["reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r
            .as_str()
            .unwrap()
            .contains("removes 1 assertion(s) from src/tests.rs")),
        "{reasons:?}"
    );
    // A card for anything but its two answers is an error, not a failed task.
    let bad = client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "yes"}),
        )
        .await;
    assert!(bad.is_err(), "{bad:?}");
    assert_eq!(
        task_of(&client, &id, &task).await["state"],
        "waiting_action"
    );

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "override"}),
        )
        .await
        .unwrap();
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    let result = done["result"].as_str().unwrap();
    assert!(result.contains("Fixed the cart."), "{result}");
    assert!(
        !result.contains("All tests pass.\n") && result.contains("Atlas removed 1 sentence(s)"),
        "{result}"
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_runs_the_task_again_with_the_reasons_in_its_prompt() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    use_agent_bin(&weakening_agent(tmp.path()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "fix the cart").await;
    wait_task_state(
        &client,
        &id,
        &task,
        "waiting_action",
        Duration::from_secs(30),
    )
    .await;

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "retry"}),
        )
        .await
        .unwrap();
    // The same task runs again, so it parks again (the fake agent weakens the test again), and its
    // prompt now carries what was refused.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let t = task_of(&client, &id, &task).await;
        let prompt = t["prompt"].as_str().unwrap();
        if t["state"] == "waiting_action" && prompt.contains("refused by Atlas's guards") {
            assert!(prompt.starts_with("fix the cart"), "{prompt}");
            assert!(
                prompt.contains("removes 1 assertion(s) from src/tests.rs"),
                "{prompt}"
            );
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never re-ran: {t}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn honest_work_finishes_done_with_no_card() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Nothing to change.").script(),
    );
    use_agent_bin(&agent);
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "look at the cart").await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    assert_eq!(done["result"], "Nothing to change.");
    assert!(done["action"].is_null(), "{done}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claim_backed_by_a_test_run_with_output_is_kept() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    let hooks = Hooks::new()
        .sleep(1)
        .post(json!({"event": "pre_tool_use", "tool_name": "Bash",
                     "tool_input": {"command": "cargo test"}}))
        .post(json!({"event": "post_tool_use", "tool_name": "Bash",
                     "tool_input": {"command": "cargo test"},
                     "tool_response": {"stdout": "test result: ok. 4 passed; 0 failed"}}))
        .done("Fixed the cart. All tests pass.");
    use_agent_bin(&fake_agent(tmp.path(), "claude", &hooks.script()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "fix the cart").await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    assert_eq!(done["result"], "Fixed the cart. All tests pass.", "{done}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unproven_claim_is_stripped_from_the_result() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let proj = project(tmp.path());
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new()
            .done("Fixed the cart. All tests pass.")
            .script(),
    );
    use_agent_bin(&agent);
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = crewmate_on(&client, &proj).await;
    let task = send(&client, &id, "fix the cart").await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    let result = done["result"].as_str().unwrap();
    assert!(result.starts_with("Fixed the cart."), "{result}");
    assert!(
        !result.starts_with("Fixed the cart. All tests pass."),
        "{result}"
    );
    assert!(result.contains("Atlas removed 1 sentence(s)"), "{result}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}
