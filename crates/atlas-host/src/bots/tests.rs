//! Integration tests for the bot subsystem.
//!
//! Each test that spawns ptys needs a real daemon and a real shell:
//! annotated with `#[tokio::test(flavor = "multi_thread")]`.

#[cfg(all(test, unix))]
mod bot_integration {
    use std::path::Path;
    use std::time::Duration;

    use serde_json::{Value, json};

    use crate::HostClient;
    use crate::server::ServeOpts;

    // ── daemon helper ────────────────────────────────────────────────────────

    async fn daemon(dir: &Path) -> HostClient {
        let d = dir.to_path_buf();
        tokio::spawn(async move {
            let _ = crate::server::serve_with(&d, ServeOpts::default()).await;
        });
        for _ in 0..200 {
            if let Ok(c) = HostClient::connect_dir(dir, "test").await {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("hostd did not start in 10 s");
    }

    /// Write a shell script to `dir/bin/<name>` (0755) and return its absolute path.
    fn fake_agent(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
        let bin_dir = dir.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let path = bin_dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    /// Wait up to `timeout` for a task on `bot_id` to reach `state`. Polls every 250 ms.
    async fn wait_task_state(
        client: &HostClient,
        bot_id: &str,
        task_id: &str,
        state: &str,
        timeout: Duration,
    ) -> Value {
        let start = std::time::Instant::now();
        loop {
            let v = client
                .call("bot_tasks", json!({"id": bot_id, "limit": 20}))
                .await
                .expect("bot_tasks failed");
            if let Some(tasks) = v["tasks"].as_array()
                && let Some(t) = tasks.iter().find(|t| t["id"] == task_id)
                && t["state"].as_str() == Some(state)
            {
                return t.clone();
            }
            if start.elapsed() > timeout {
                panic!("task {task_id} did not reach state '{state}' within {timeout:?}");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    // ── Test 1: task with fake agent script ends done ────────────────────────
    // Uses the #[cfg(test)] binary override in runner.rs — no env injection via
    // the protocol.

    #[tokio::test(flavor = "multi_thread")]
    async fn task_with_fake_agent_ends_done() {
        // Serialise tests that override the agent binary.
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");

        // Fake "claude": POST agent_end to the hook endpoint then exit 0.
        let stop_script = r#"set -e
if [ -f "$ATLAS_HOOK_ENDPOINT" ]; then
  . "$ATLAS_HOOK_ENDPOINT"
  curl -s -X POST "http://127.0.0.1:${ATLAS_HOOK_PORT}/hook/claude" \
       -H "X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}" \
       -H "Content-Type: application/json" \
       -d "{\"pane_key\":\"${ATLAS_PANE_KEY}\",\"payload\":{\"event\":\"agent_end\",\"will_continue\":false,\"text\":\"ok\"}}" \
       || true
fi
exit 0"#;
        let fake_bin = fake_agent(tmp.path(), "claude", stop_script);
        crate::bots::runner::set_test_agent_bin(Some(&fake_bin.to_string_lossy()));

        let client = daemon(&dir).await;
        let bot = client
            .call(
                "bot_create",
                json!({
                    "name": "TestBot", "role": "test bot", "agent": "claude", "approval": "auto"
                }),
            )
            .await
            .expect("bot_create failed");
        let bot_id = bot["id"].as_str().expect("no id").to_string();

        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": "hello", "source": "test"}),
            )
            .await
            .expect("bot_task failed");
        let task_id = v["task"]["id"].as_str().expect("no task id").to_string();

        let task =
            wait_task_state(&client, &bot_id, &task_id, "done", Duration::from_secs(15)).await;
        assert_eq!(
            task["state"], "done",
            "unexpected state: {} error: {}",
            task["state"], task["error"]
        );

        crate::bots::runner::set_test_agent_bin(None);
    }

    // ── Test 2: agent exits 3 → failed ──────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_exit_3_yields_failed() {
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let fake_bin = fake_agent(tmp.path(), "claude", "exit 3");
        crate::bots::runner::set_test_agent_bin(Some(&fake_bin.to_string_lossy()));

        let client = daemon(&dir).await;
        let bot = client
            .call(
                "bot_create",
                json!({
                    "name": "ExitBot", "role": "exits", "agent": "claude", "approval": "auto"
                }),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": "go", "source": "test"}),
            )
            .await
            .unwrap();
        let task_id = v["task"]["id"].as_str().unwrap().to_string();

        let task = wait_task_state(
            &client,
            &bot_id,
            &task_id,
            "failed",
            Duration::from_secs(15),
        )
        .await;
        assert_eq!(task["state"], "failed");
        let err = task["error"].as_str().unwrap_or("");
        assert!(
            err.contains("code 3") || err.contains("exited"),
            "expected exit-code error, got: {err}"
        );
        crate::bots::runner::set_test_agent_bin(None);
    }

    // ── Test 3: queued task survives restart ─────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn queued_task_survives_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let client = daemon(&dir).await;

        let bot = client
            .call(
                "bot_create",
                json!({"name": "PersistBot", "role": "persists", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        // Enqueue a task.
        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": "persist me", "source": "test"}),
            )
            .await
            .unwrap();
        let task_id = v["task"]["id"].as_str().unwrap().to_string();

        // Stop the daemon via shutdown.
        let _ = client.call("shutdown", json!({})).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Restart.
        let client2 = daemon(&dir).await;

        // The task should exist (possibly re-tried or marked interrupted).
        let v2 = client2
            .call("bot_tasks", json!({"id": bot_id, "limit": 20}))
            .await
            .unwrap();
        let tasks = v2["tasks"].as_array().unwrap();
        let found = tasks.iter().any(|t| t["id"] == task_id);
        assert!(found, "task {task_id} not found after restart");
    }

    // ── Test 4: BotCreate omp+ask rejected ───────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn bot_create_omp_ask_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let client = daemon(&dir).await;

        let result = client
            .call(
                "bot_create",
                json!({"name": "OmpBot", "role": "x", "agent": "omp", "approval": "ask"}),
            )
            .await;
        // Expect an error response.
        match result {
            Err(e) => {
                assert!(
                    e.to_string().contains("omp bots run with approval: auto"),
                    "unexpected error: {e}"
                );
            }
            Ok(v) => {
                // The client may return the error inside a value.
                assert!(
                    v.get("error").is_some() || !v["ok"].as_bool().unwrap_or(true),
                    "expected error for omp+ask, got: {v}"
                );
            }
        }
    }

    // ── Test 5: invalid cron rejected ────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_cron_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let client = daemon(&dir).await;

        let bot = client
            .call(
                "bot_create",
                json!({"name": "CronBot", "role": "crons", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        let result = client
            .call(
                "bot_routine_set",
                json!({
                    "id": bot_id,
                    "routines": [
                        {"id": "r1", "cron": "not a cron", "prompt": "hi", "enabled": true}
                    ]
                }),
            )
            .await;

        match result {
            Err(e) => assert!(
                e.to_string().contains("invalid cron") || e.to_string().contains("not a cron"),
                "unexpected error: {e}"
            ),
            Ok(v) => assert!(
                v.get("error").is_some(),
                "expected error for invalid cron, got: {v}"
            ),
        }
    }

    // ── Test 6: routine "* * * * *" enqueues within 90 s ─────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn routine_every_minute_fires_within_90s() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let client = daemon(&dir).await;

        let bot = client
            .call(
                "bot_create",
                json!({"name": "RoutineBot", "role": "routines", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        // Set a routine that fires every minute.
        client
            .call(
                "bot_routine_set",
                json!({
                    "id": bot_id,
                    "routines": [
                        {
                            "id": "r1",
                            "cron": "* * * * *",
                            "prompt": "routine task",
                            "enabled": true
                        }
                    ]
                }),
            )
            .await
            .expect("bot_routine_set failed");

        // Wait up to 90 s for a task to appear with source="routine".
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            let v = client
                .call("bot_tasks", json!({"id": bot_id, "limit": 20}))
                .await
                .unwrap();
            let tasks = v["tasks"].as_array().unwrap();
            if tasks.iter().any(|t| t["source"] == "routine") {
                return; // success
            }
            if std::time::Instant::now() > deadline {
                panic!("routine task did not appear within 90 s");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    // ── Test 7: bot_create with unknown `env` field is silently ignored ────────
    // Security: env injection through the protocol must not reach the pty.

    #[tokio::test(flavor = "multi_thread")]
    async fn bot_create_ignores_unknown_env_field() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let client = daemon(&dir).await;

        // Attempt to inject env via bot_create — the field is not in the protocol.
        // serde ignores unknown fields by default, so this must succeed.
        let bot = client
            .call(
                "bot_create",
                json!({
                    "name": "EnvBot",
                    "role": "test",
                    "agent": "claude",
                    "approval": "auto",
                    "env": {"PATH": "/injected/bin:/usr/bin"}
                }),
            )
            .await
            .expect("bot_create with unknown env field should succeed (field ignored)");

        // The bot was created; the env key must not appear in the returned object.
        assert!(
            bot.get("env").is_none() || bot["env"].is_null(),
            "env must not be stored in the bot object: {bot}"
        );
        // And it must not appear when the bot is fetched.
        let list = client.call("bot_list", json!({})).await.unwrap();
        let found = list["bots"]
            .as_array()
            .and_then(|bs| bs.iter().find(|b| b["name"] == "EnvBot"))
            .cloned()
            .unwrap();
        assert!(
            found.get("env").is_none() || found["env"].is_null(),
            "env must not appear in bot_list: {found}"
        );
        client.call("shutdown", json!({})).await.ok();
    }
}
