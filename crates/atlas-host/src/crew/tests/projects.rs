//! The project a crewmate works in: Git checkout before the first task, the "downloading" state,
//! and folders that do not exist.

use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};

// Incident: Jorge's first task sat "Queued" ~90 s on atlas-test-ship while the task itself cloned the repo. The
// checkout now starts when the crewmate gets its project, so it is ready before anyone sends a message.
#[tokio::test(flavor = "multi_thread")]
async fn a_git_project_is_checked_out_before_the_first_task() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "one",
        ][..],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&src)
                .status()
                .unwrap()
                .success()
        );
    }
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "Jorge", "role": "r", "agent": "claude", "approval": "auto",
                   "project": {"kind": "git", "url": format!("file://{}", src.display())}}),
        )
        .await
        .unwrap();
    let work = dir
        .join("crew")
        .join(crewmate["id"].as_str().unwrap())
        .join("work");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !work.join(".git").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "no checkout without a task: {}",
            work.display()
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    client.call("shutdown", json!({})).await.ok();
}

// Incident: a first task on a big repo showed a bare "Queued" for over a minute while the project downloaded.
// While the task waits for the checkout it carries `preparing` (the repo), and it is gone once the wait ends, here
// with a clone that fails so the wait is never skipped by a finished background clone.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_says_it_is_downloading_the_project_while_it_waits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let missing = format!("file://{}/no-such-repo", tmp.path().display());
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "J", "role": "r", "agent": "claude", "approval": "auto",
                   "project": {"kind": "git", "url": missing}}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();
    let task = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let tid = task["task"]["id"].as_str().unwrap().to_string();
    let done = wait_task_state(&client, &id, &tid, "failed", Duration::from_secs(20)).await;
    assert!(
        done.get("preparing").is_none(),
        "cleared once the wait ended: {done}"
    );

    let log = std::fs::read_to_string(dir.join("crew").join(&id).join("tasks.jsonl")).unwrap();
    let shown: Vec<Value> = log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|t| t["id"] == tid.as_str() && t.get("preparing").is_some())
        .collect();
    assert!(
        !shown.is_empty(),
        "the task never said it was downloading the project"
    );
    assert!(
        shown[0]["preparing"]
            .as_str()
            .unwrap()
            .ends_with("no-such-repo")
    );
    client.call("shutdown", json!({})).await.ok();
}

// Incident: the setup saved the text "Something else" as a project folder; the crewmate accepted it and every task
// then failed with "project prepare: path error: No such file or directory". Setting a folder that does not exist
// on the crewmate's computer must be refused when it is set, with the reason, and leave the old project in place.
#[tokio::test(flavor = "multi_thread")]
async fn setting_a_missing_project_folder_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "J", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    let err = client
        .call(
            "crewmate_update",
            json!({"id": id, "project": {"kind": "path", "path": "Something else"}}),
        )
        .await
        .expect_err("a folder that does not exist must be refused");
    assert!(
        err.to_string().contains("Something else"),
        "the error names the folder: {err}"
    );

    let real = tmp.path().join("proj");
    std::fs::create_dir(&real).unwrap();
    client
        .call(
            "crewmate_update",
            json!({"id": id, "project": {"kind": "path", "path": real}}),
        )
        .await
        .expect("an existing folder is accepted");
    client.call("shutdown", json!({})).await.ok();
}

// ── Test 20: project path that doesn't exist → task fails ────────────────

#[tokio::test(flavor = "multi_thread")]
async fn engine_project_invalid_path_fails_task() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Fake engine that always succeeds — but project::prepare should fail first.
    let fake_bin = fake_agent(
        tmp.path(),
        "eng",
        "echo '{\"type\":\"result\",\"session_id\":\"s\",\"result\":\"ok\",\"is_error\":false}'",
    );
    crate::crew::engine::set_test_engine_bin(Some(&fake_bin.to_string_lossy()));

    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({
                "name": "ProjMate",
                "role": "r",
                "agent": "claude",
                "approval": "auto",
                "engine": {"kind": "claude"},
                "project": {"kind": "path", "path": "/nonexistent/absolutely/missing"}
            }),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    let v = client
        .call(
            "crewmate_task",
            json!({"id":id,"prompt":"go","source":"test"}),
        )
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();
    let task = wait_task_state(&client, &id, &task_id, "failed", Duration::from_secs(15)).await;
    assert_eq!(task["state"], "failed");
    let err = task["error"].as_str().unwrap_or("");
    assert!(
        err.contains("project")
            || err.contains("path")
            || err.contains("not")
            || err.contains("missing")
            || err.contains("exist"),
        "error must mention the bad path, got: {err}"
    );

    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── Test 21: two engine turns keep the same session_id ───────────────────
