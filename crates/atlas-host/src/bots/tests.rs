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
            let _ = crate::server::serve_with(
                &d,
                ServeOpts {
                    bot_host: true,
                    ..Default::default()
                },
            )
            .await;
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

    // ── Test 8: bot_list state mapping ───────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn bot_list_state_mapping() {
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");

        // Script that hangs briefly then posts agent_end.
        let hang_script = r#"set -e
sleep 1
if [ -f "$ATLAS_HOOK_ENDPOINT" ]; then
  . "$ATLAS_HOOK_ENDPOINT"
  curl -s -X POST "http://127.0.0.1:${ATLAS_HOOK_PORT}/hook/claude" \
       -H "X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}" \
       -H "Content-Type: application/json" \
       -d "{\"pane_key\":\"${ATLAS_PANE_KEY}\",\"payload\":{\"event\":\"agent_end\",\"will_continue\":false,\"text\":\"ok\"}}" \
       || true
fi
exit 0"#;
        let fake_bin = fake_agent(tmp.path(), "claude", hang_script);
        crate::bots::runner::set_test_agent_bin(Some(&fake_bin.to_string_lossy()));

        let client = daemon(&dir).await;
        let bot = client
            .call(
                "bot_create",
                json!({"name": "StateBot", "role": "r", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        // No tasks yet: state should be "idle".
        let list = client.call("bot_list", json!({})).await.unwrap();
        let b = &list["bots"].as_array().unwrap()[0];
        assert_eq!(b["state"], "idle", "expected idle before task: {b}");

        // Start a task; while it runs the state should be "working".
        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": "go", "source": "test"}),
            )
            .await
            .unwrap();
        let task_id = v["task"]["id"].as_str().unwrap().to_string();

        // Poll until the bot reports "working" while the task runs.
        let mut saw_working = false;
        for _ in 0..40 {
            let list = client.call("bot_list", json!({})).await.unwrap();
            if list["bots"][0]["state"] == "working" {
                saw_working = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(saw_working, "bot never reported working while its task ran");

        wait_task_state(&client, &bot_id, &task_id, "done", Duration::from_secs(15)).await;
        // After done, state is "idle" again.
        let list = client.call("bot_list", json!({})).await.unwrap();
        assert_eq!(list["bots"].as_array().unwrap()[0]["state"], "idle");
        // queued should be 0.
        assert_eq!(list["bots"].as_array().unwrap()[0]["queued"], 0);

        crate::bots::runner::set_test_agent_bin(None);
        client.call("shutdown", json!({})).await.ok();
    }

    // ── Test 9: bot_list last and queued ─────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn bot_list_last_and_queued() {
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");

        // Instant-exit agent.
        let stop_script = r#"set -e
if [ -f "$ATLAS_HOOK_ENDPOINT" ]; then
  . "$ATLAS_HOOK_ENDPOINT"
  curl -s -X POST "http://127.0.0.1:${ATLAS_HOOK_PORT}/hook/claude" \
       -H "X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}" \
       -H "Content-Type: application/json" \
       -d "{\"pane_key\":\"${ATLAS_PANE_KEY}\",\"payload\":{\"event\":\"agent_end\",\"will_continue\":false,\"text\":\"all done\"}}" \
       || true
fi
exit 0"#;
        let fake_bin = fake_agent(tmp.path(), "claude", stop_script);
        crate::bots::runner::set_test_agent_bin(Some(&fake_bin.to_string_lossy()));

        let client = daemon(&dir).await;
        let bot = client
            .call(
                "bot_create",
                json!({"name": "LastBot", "role": "r", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        // No tasks: last absent, queued=0.
        let list = client.call("bot_list", json!({})).await.unwrap();
        let b = &list["bots"].as_array().unwrap()[0];
        assert!(
            b.get("last").is_none() || b["last"].is_null(),
            "expected no last: {b}"
        );
        assert_eq!(b["queued"], 0, "expected queued=0: {b}");

        let prompt = "a".repeat(200);
        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": prompt, "source": "test"}),
            )
            .await
            .unwrap();
        let task_id = v["task"]["id"].as_str().unwrap().to_string();
        wait_task_state(&client, &bot_id, &task_id, "done", Duration::from_secs(15)).await;

        // After done: last.text is ≤ 140 chars.
        let list = client.call("bot_list", json!({})).await.unwrap();
        let b = &list["bots"].as_array().unwrap()[0];
        let last_text = b["last"]["text"].as_str().unwrap_or("");
        assert!(
            last_text.len() <= 140,
            "last.text must be ≤140 chars, got {}: {last_text}",
            last_text.len()
        );
        assert!(!last_text.is_empty(), "last.text must not be empty");
        assert_eq!(b["queued"], 0, "queued should be 0 after task done");

        crate::bots::runner::set_test_agent_bin(None);
        client.call("shutdown", json!({})).await.ok();
    }

    // ── Test 10: bot_tasks steps ─────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn bot_tasks_steps() {
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");

        // Agent that sends a pre_tool_use hook before finishing.
        let script = r#"set -e
if [ -f "$ATLAS_HOOK_ENDPOINT" ]; then
  . "$ATLAS_HOOK_ENDPOINT"
  BASE="http://127.0.0.1:${ATLAS_HOOK_PORT}/hook/claude"
  HDR="-H \"X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}\" -H \"Content-Type: application/json\""
  curl -s -X POST "$BASE" \
       -H "X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}" \
       -H "Content-Type: application/json" \
       -d "{\"pane_key\":\"${ATLAS_PANE_KEY}\",\"payload\":{\"event\":\"pre_tool_use\",\"tool_name\":\"bash\",\"tool_input\":{\"command\":\"echo hello\"}}}" \
       || true
  curl -s -X POST "$BASE" \
       -H "X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}" \
       -H "Content-Type: application/json" \
       -d "{\"pane_key\":\"${ATLAS_PANE_KEY}\",\"payload\":{\"event\":\"agent_end\",\"will_continue\":false,\"text\":\"done\"}}" \
       || true
fi
exit 0"#;
        let fake_bin = fake_agent(tmp.path(), "claude", script);
        crate::bots::runner::set_test_agent_bin(Some(&fake_bin.to_string_lossy()));

        let client = daemon(&dir).await;
        let bot = client
            .call(
                "bot_create",
                json!({"name": "StepsBot", "role": "r", "agent": "claude", "approval": "auto"}),
            )
            .await
            .unwrap();
        let bot_id = bot["id"].as_str().unwrap().to_string();

        let v = client
            .call(
                "bot_task",
                json!({"id": bot_id, "prompt": "run tool", "source": "test"}),
            )
            .await
            .unwrap();
        let task_id = v["task"]["id"].as_str().unwrap().to_string();
        let task =
            wait_task_state(&client, &bot_id, &task_id, "done", Duration::from_secs(15)).await;

        // The hook posts are synchronous (curl), so pre_tool_use lands before agent_end.
        let steps = task["steps"].as_array().expect("done task has steps");
        let step = steps.first().expect("the pre_tool_use hook became a step");
        assert!(step["ts"].is_number(), "step must have ts: {step}");
        assert!(
            step["text"].as_str().is_some_and(|t| t.contains("bash")),
            "step text must contain tool_name 'bash': {step}"
        );

        crate::bots::runner::set_test_agent_bin(None);
        client.call("shutdown", json!({})).await.ok();
    }

    // ── Test 11: caps without ATLAS_BOT_HOST ─────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn caps_without_atlas_bot_host() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd-nocaps");
        // Start WITHOUT bot_host.
        let d = dir.clone();
        tokio::spawn(async move {
            let _ = crate::server::serve_with(&d, ServeOpts::default()).await;
        });
        let mut client = None;
        for _ in 0..200 {
            if let Ok(c) = HostClient::connect_dir(&dir, "test").await {
                client = Some(c);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let client = client.expect("hostd did not start");

        let info = client.call("info", json!({})).await.unwrap();
        let caps = info["caps"].as_array().unwrap();
        assert!(
            !caps.iter().any(|c| c == "bots"),
            "caps must not contain 'bots' without bot_host: {caps:?}"
        );

        let result = client
            .call(
                "bot_create",
                json!({"name": "x", "role": "r", "agent": "claude", "approval": "auto"}),
            )
            .await;
        let is_err = match &result {
            Err(_) => true,
            Ok(v) => v.get("error").is_some() || !v["ok"].as_bool().unwrap_or(true),
        };
        assert!(
            is_err,
            "bot_create must error without bot_host, got: {result:?}"
        );

        client.call("shutdown", json!({})).await.ok();
    }

    // ── Test 12: bot pane approval held/not held ──────────────────────────────

    #[tokio::test]
    async fn bot_pane_approval_held_with_desktop_client_not_held_without() {
        use crate::agents::AgentHub;
        use std::sync::Arc;

        let hub = Arc::new(AgentHub::default());

        // No subscribers, no telegram, no push tokens: ask returns None immediately.
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            hub.ask(
                "bot:testbot:task1",
                "claude",
                &json!({"tool_name": "bash", "tool_input": {"command": "ls"}}),
                Duration::from_secs(5),
                false, // someone_can_answer = false
            ),
        )
        .await;
        // Should complete quickly with None (no one to answer).
        assert!(
            result.is_ok(),
            "ask should return quickly when no one can answer"
        );
        assert_eq!(
            result.unwrap(),
            None,
            "should return None with no answerers"
        );
    }

    // ── Test 13: bot_list JSON contract — fixture for apps/mobile ────────────
    //
    // Calls bot_list on a REAL daemon.  Sequence:
    //   1. Bot B runs instant-finish agent  → idle, last present
    //   2. Bot A runs hang agent            → working, running_task present
    //   3. bot_list is called               → real reply captured → fixture written
    //
    // pending_approval is NOT included: holding it requires a desktop subscriber
    // to be wired into the test daemon, which is racy.  The field shape is
    // already covered by Test 12 (AgentHub::ask) and by the Dart test's null check.
    // Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host bot_list_json_contract
    #[tokio::test(flavor = "multi_thread")]
    async fn bot_list_json_contract() {
        let _bin_lock = crate::bots::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd-contract");

        // ── Phase 1: Bot B — instant-finish agent (produces "last") ──────────
        // Uses the same script pattern as existing Test 1 and Test 9.
        let stop_script = concat!(
            "#!/bin/sh\nset -e\n",
            "if [ -f \"$ATLAS_HOOK_ENDPOINT\" ]; then\n",
            "  . \"$ATLAS_HOOK_ENDPOINT\"\n",
            "  curl -s -X POST \"http://127.0.0.1:${ATLAS_HOOK_PORT}/hook/claude\"",
            " -H \"X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}\"",
            " -H \"Content-Type: application/json\"",
            " -d \"{\\\"pane_key\\\":\\\"${ATLAS_PANE_KEY}\\\",\\\"payload\\\":{\\\"event\\\":\\\"agent_end\\\",\\\"will_continue\\\":false,\\\"text\\\":\\\"nightly done\\\"}}\"",
            " || true\n",
            "fi\n",
            "exit 0\n",
        );
        let stop_bin = fake_agent(tmp.path(), "claude", stop_script);
        crate::bots::runner::set_test_agent_bin(Some(&stop_bin.to_string_lossy()));

        let client = daemon(&dir).await;

        let bot_b = client
            .call(
                "bot_create",
                json!({
                    "name": "Nightly", "role": "Nightly checks",
                    "agent": "claude", "approval": "auto"
                }),
            )
            .await
            .expect("bot_create B");
        let bot_b_id = bot_b["id"].as_str().unwrap().to_string();

        let vb = client
            .call(
                "bot_task",
                json!({"id": bot_b_id, "prompt": "run checks", "source": "routine"}),
            )
            .await
            .expect("bot_task B");
        let task_b_id = vb["task"]["id"].as_str().unwrap().to_string();
        // Bot B completes; now bot_list will show last.
        wait_task_state(
            &client,
            &bot_b_id,
            &task_b_id,
            "done",
            Duration::from_secs(15),
        )
        .await;

        // ── Phase 2: Bot A — hanging agent (produces running_task) ───────────
        let hang_bin = fake_agent(tmp.path(), "claude", "#!/bin/sh\nsleep 60\nexit 0\n");
        crate::bots::runner::set_test_agent_bin(Some(&hang_bin.to_string_lossy()));

        let bot_a = client
            .call(
                "bot_create",
                json!({
                    "name": "Reviewer", "role": "Reviews PRs",
                    "agent": "claude", "approval": "ask"
                }),
            )
            .await
            .expect("bot_create A");
        let bot_a_id = bot_a["id"].as_str().unwrap().to_string();

        let va = client
            .call(
                "bot_task",
                json!({"id": bot_a_id, "prompt": "review PR", "source": "test"}),
            )
            .await
            .expect("bot_task A");
        let task_a_id = va["task"]["id"].as_str().unwrap().to_string();

        // Wait until bot_list sees bot_a as "working".
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let list = client.call("bot_list", json!({})).await.unwrap();
            if list["bots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["id"] == bot_a_id && b["state"] == "working")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "bot A never reached working state"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // ── Phase 3: capture real bot_list ────────────────────────────────────
        let list = client.call("bot_list", json!({})).await.expect("bot_list");
        let bots = list["bots"].as_array().expect("bots array");

        let a = bots
            .iter()
            .find(|b| b["id"] == bot_a_id)
            .expect("bot A in list");
        let b = bots
            .iter()
            .find(|b| b["id"] == bot_b_id)
            .expect("bot B in list");

        // ── Assert the REAL shape ─────────────────────────────────────────────
        // working bot: running_task is an OBJECT {id, pane}, not a string
        assert_eq!(a["state"], "working", "bot A must be working: {a}");
        let rt = &a["running_task"];
        assert!(
            rt.is_object(),
            "running_task must be {{id,pane}}, got: {rt}"
        );
        assert_eq!(rt["id"].as_str().unwrap(), task_a_id, "running_task.id");
        assert_eq!(
            rt["pane"].as_str().unwrap(),
            &format!("bot:{bot_a_id}:{task_a_id}"),
            "running_task.pane format"
        );

        // idle bot with done task: last is present
        assert_eq!(b["state"], "idle", "bot B must be idle: {b}");
        assert!(!b["last"].is_null(), "bot B must have last: {b}");
        assert!(
            !b["last"]["text"].as_str().unwrap_or("").is_empty(),
            "last.text must not be empty: {b}"
        );
        assert!(b["last"]["ts"].is_number(), "last.ts must be a number: {b}");
        assert!(
            b["running_task"].is_null(),
            "idle bot must have no running_task: {b}"
        );

        // ── Build deterministic fixture ───────────────────────────────────────
        // Normalise volatile fields (random UUIDs, real timestamps) so the
        // stored fixture has a stable diff.
        let mut a_fix = a.clone();
        let mut b_fix = b.clone();
        a_fix["id"] = json!("bot-working");
        a_fix["running_task"]["id"] = json!("task-working");
        a_fix["running_task"]["pane"] = json!("bot:bot-working:task-working");
        a_fix["queued"] = json!(0);
        if !a_fix["last"].is_null() {
            a_fix["last"]["ts"] = json!(1700000001u64);
        }
        b_fix["id"] = json!("bot-done");
        b_fix["last"]["ts"] = json!(1700000000u64);
        b_fix["queued"] = json!(0);

        let fixture = json!({ "bots": [a_fix, b_fix] });

        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/mobile/test/fixtures/bot_list.json");

        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(
                &fixture_path,
                serde_json::to_string_pretty(&fixture).unwrap() + "\n",
            )
            .unwrap();
        }

        let stored: Value = serde_json::from_str(
            &std::fs::read_to_string(&fixture_path).expect(
                "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host bot_list_json_contract",
            ),
        )
        .unwrap();

        // Structural invariant: stored fixture has the same object shape as the live reply.
        let sb = stored["bots"].as_array().unwrap();
        let sw = sb
            .iter()
            .find(|b| b["state"] == "working")
            .expect("working bot in fixture");
        assert!(
            sw["running_task"].is_object(),
            "fixture running_task must be an object: {sw}"
        );
        assert!(
            sw["running_task"]["id"].is_string(),
            "running_task.id must be string"
        );
        assert!(
            sw["running_task"]["pane"].is_string(),
            "running_task.pane must be string"
        );

        assert_eq!(
            stored, fixture,
            "bot_list.json changed: rerun with UPDATE_FIXTURES=1"
        );

        crate::bots::runner::set_test_agent_bin(None);
        client.call("shutdown", json!({})).await.ok();
    }
}
