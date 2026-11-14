//! What starts a task by itself: routines (cron), webhooks, a crewmate finishing another's task,
//! and the validation of every trigger kind.

use std::time::Duration;

use serde_json::json;

use super::fixture::{Hooks, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state};

#[tokio::test(flavor = "multi_thread")]
async fn invalid_cron_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "CronMate", "role": "crons", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let crewmate_id = crewmate["id"].as_str().unwrap().to_string();

    let result = client
        .call(
            "crewmate_routine_set",
            json!({
                "id": crewmate_id,
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

    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "RoutineMate", "role": "routines", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let crewmate_id = crewmate["id"].as_str().unwrap().to_string();

    // Set a routine that fires every minute.
    client
        .call(
            "crewmate_routine_set",
            json!({
                "id": crewmate_id,
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
        .expect("crewmate_routine_set failed");

    // Wait up to 90 s for a task to appear with source="routine".
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        let v = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 20}))
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

// ── Test 7: crewmate_create with unknown `env` field is silently ignored ────────
// Security: env injection through the protocol must not reach the pty.

#[test]
fn trigger_validation() {
    use crate::crew::store::{Trigger, TriggerKind};
    use crate::crew::triggers::validate_trigger;

    fn t(kind: TriggerKind) -> Trigger {
        Trigger {
            id: "t".into(),
            kind,
            enabled: true,
            prompt: "p".into(),
            last_fired: None,
            tz: None,
            next_fire: None,
        }
    }

    assert!(
        validate_trigger(&t(TriggerKind::Schedule {
            cron: "* * * * *".into()
        }))
        .is_ok()
    );
    assert!(
        validate_trigger(&t(TriggerKind::Schedule {
            cron: "not a cron".into()
        }))
        .is_err()
    );

    assert!(validate_trigger(&t(TriggerKind::Interval { every_secs: 60 })).is_ok());
    assert!(validate_trigger(&t(TriggerKind::Interval { every_secs: 59 })).is_err());

    assert!(
        validate_trigger(&t(TriggerKind::Github {
            repo: "owner/repo".into(),
            events: vec!["pr_opened".into()],
        }))
        .is_ok()
    );
    assert!(
        validate_trigger(&t(TriggerKind::Github {
            repo: "".into(),
            events: vec![],
        }))
        .is_err()
    );

    assert!(
        validate_trigger(&t(TriggerKind::Crewmate {
            from_crewmate: "abc".into(),
            on: "done".into(),
        }))
        .is_ok()
    );
    assert!(
        validate_trigger(&t(TriggerKind::Crewmate {
            from_crewmate: "abc".into(),
            on: "invalid".into(),
        }))
        .is_err()
    );

    assert!(
        validate_trigger(&t(TriggerKind::Service {
            project_dir: "/x".into(),
            service: "web".into(),
            on: "failed".into(),
        }))
        .is_ok()
    );
    assert!(
        validate_trigger(&t(TriggerKind::Service {
            project_dir: "/x".into(),
            service: "web".into(),
            on: "bad".into(),
        }))
        .is_err()
    );
}

// ── Test 17: trigger render_prompt substitution (pure unit) ─────────────

#[test]
fn trigger_render_prompt() {
    use crate::crew::triggers::render_prompt;
    assert_eq!(
        render_prompt("New {{event}} arrived", "PR #42"),
        "New PR #42 arrived"
    );
    assert_eq!(render_prompt("no placeholder", "x"), "no placeholder");
    assert_eq!(render_prompt("{{event}} and {{event}}", "y"), "y and y");
}

// ── Test 18: crewmate_secret_set persists 0600 file; value not in crewmate_list ────

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_triggers_set_validates() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name":"TrigMate","role":"r","agent":"claude","approval":"auto"}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap();

    // Valid schedule trigger — must succeed.
    client
        .call(
            "crewmate_triggers_set",
            json!({"id":id,"triggers":[
                {"id":"t1","kind":{"type":"schedule","cron":"* * * * *"},"enabled":true,"prompt":"hello"}
            ]}),
        )
        .await
        .expect("valid trigger must succeed");

    // Invalid cron — must be rejected.
    let r = client
        .call(
            "crewmate_triggers_set",
            json!({"id":id,"triggers":[
                {"id":"t2","kind":{"type":"schedule","cron":"bad cron"},"enabled":true,"prompt":"p"}
            ]}),
        )
        .await;
    let rejected = match &r {
        Err(_) => true,
        Ok(v) => v.get("error").is_some(),
    };
    assert!(rejected, "bad cron must be rejected, got: {r:?}");

    // Interval under 60 s — must be rejected.
    let r2 = client
        .call(
            "crewmate_triggers_set",
            json!({"id":id,"triggers":[
                {"id":"t3","kind":{"type":"interval","every_secs":5},"enabled":true,"prompt":"p"}
            ]}),
        )
        .await;
    let rejected2 = match &r2 {
        Err(_) => true,
        Ok(v) => v.get("error").is_some(),
    };
    assert!(rejected2, "interval < 60 s must be rejected, got: {r2:?}");

    client.call("shutdown", json!({})).await.ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn webhook_wrong_token_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "WebMate", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    // Add a webhook trigger with a known token.
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": [
                {"id": "wh1", "kind": {"type": "webhook", "token": "correct-token"},
                 "enabled": true, "prompt": "webhook: {{event}}"}
            ]}),
        )
        .await
        .expect("crewmate_triggers_set failed");

    // POST with the WRONG token — should get 403 or no task.
    let hook_env =
        std::fs::read_to_string(dir.join("desktop").join("hook-endpoint.env")).unwrap_or_default();
    let mut port = 0u16;
    let mut tok = String::new();
    for line in hook_env.lines() {
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_PORT=") {
            port = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_TOKEN=") {
            tok = v.trim().to_string();
        }
    }

    if port > 0 {
        let body = b"hello";
        let req = format!(
            "POST /crew/{id}/hook/WRONG-TOKEN HTTP/1.1\r\nContent-Length: {}\r\nX-Atlas-Hook-Token: {tok}\r\n\r\n",
            body.len()
        );
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        if let Ok(mut s) = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await {
            let _ = s.write_all(req.as_bytes()).await;
            let _ = s.write_all(body).await;
            let mut resp = String::new();
            let _ = s.read_to_string(&mut resp).await;
            assert!(
                resp.contains("403") || resp.contains("404"),
                "wrong webhook token must be 403/404, got: {resp}"
            );
        }
    }

    // No tasks must have been queued.
    let tasks = client
        .call("crewmate_tasks", json!({"id": id, "limit": 10}))
        .await
        .unwrap();
    assert_eq!(
        tasks["tasks"].as_array().map(|a| a.len()).unwrap_or(0),
        0,
        "no tasks must be queued after wrong-token webhook"
    );

    client.call("shutdown", json!({})).await.ok();
}

// ── Test Trigger-2: webhook POST with right token → task enqueued ────────

#[tokio::test(flavor = "multi_thread")]
async fn webhook_correct_token_enqueues_task() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Instant-exit fake agent so the queued task doesn't block.
    let fake_bin = fake_agent(tmp.path(), "claude", "exit 0");
    use_agent_bin(&fake_bin);

    let client = daemon(&dir).await;
    let crewmate = client
        .call(
            "crewmate_create",
            json!({"name": "WebMate2", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let id = crewmate["id"].as_str().unwrap().to_string();

    let webhook_token = "my-secret-webhook-abc";
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": [
                {"id": "wh1", "kind": {"type": "webhook", "token": webhook_token},
                 "enabled": true, "prompt": "webhook: {{event}}"}
            ]}),
        )
        .await
        .expect("crewmate_triggers_set failed");

    // Read hook server port and auth token.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let (port, tok) = loop {
        let hook_env = std::fs::read_to_string(dir.join("desktop").join("hook-endpoint.env"))
            .unwrap_or_default();
        let mut p = 0u16;
        let mut t = String::new();
        for line in hook_env.lines() {
            if let Some(v) = line.strip_prefix("ATLAS_HOOK_PORT=") {
                p = v.trim().parse().unwrap_or(0);
            }
            if let Some(v) = line.strip_prefix("ATLAS_HOOK_TOKEN=") {
                t = v.trim().to_string();
            }
        }
        if p > 0 && !t.is_empty() {
            break (p, t);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "hook server did not start"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // The current URL and the one webhooks were configured with before the Bot -> Crew rename
    // (`/bots/…`, pasted into GitHub and other services on servers installed from the branch): both
    // must fire, or every existing webhook would silently stop.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (n, prefix) in [(1, "crew"), (2, "bots")] {
        let body = b"pr opened";
        let req = format!(
            "POST /{prefix}/{id}/hook/{webhook_token} HTTP/1.1\r\nContent-Length: {}\r\nX-Atlas-Hook-Token: {tok}\r\n\r\n",
            body.len()
        );
        let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to hook server");
        s.write_all(req.as_bytes()).await.unwrap();
        s.write_all(body).await.unwrap();
        let mut resp = String::new();
        let _ = s.read_to_string(&mut resp).await;
        assert!(
            resp.contains("204"),
            "/{prefix}/…/hook with the right token must return 204, got: {resp}"
        );

        // Its task must appear within 5 s.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let tasks = client
                .call("crewmate_tasks", json!({"id": id, "limit": 10}))
                .await
                .unwrap();
            let count = tasks["tasks"].as_array().map(|a| a.len()).unwrap_or(0);
            if count >= n {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no task appeared after the /{prefix}/ webhook"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── Test Trigger-4: crewmate-chain trigger fires on_task_finished ─────────────

#[tokio::test(flavor = "multi_thread")]
async fn crewmate_chain_trigger_fires_on_finish() {
    let _bin_lock = lock_agent_bin().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");

    // Instant-finish fake agent.
    let stop_script = Hooks::new().done("done").script();
    let fake_bin = fake_agent(tmp.path(), "claude", &stop_script);
    use_agent_bin(&fake_bin);

    let client = daemon(&dir).await;

    // Crewmate A: the trigger source.
    let crewmate_a = client
        .call(
            "crewmate_create",
            json!({"name": "MateA", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let id_a = crewmate_a["id"].as_str().unwrap().to_string();

    // Crewmate B: watches Crewmate A with a "crewmate" trigger on "done".
    let crewmate_b = client
        .call(
            "crewmate_create",
            json!({"name": "MateB", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    let id_b = crewmate_b["id"].as_str().unwrap().to_string();

    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id_b, "triggers": [
                {"id": "chain1",
                 "kind": {"type": "crewmate", "from_crewmate": id_a, "on": "done"},
                 "enabled": true,
                 "prompt": "crewmate A finished: {{event}}"}
            ]}),
        )
        .await
        .expect("crewmate_triggers_set failed");

    // Trigger Crewmate A.
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id_a, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task_a = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&client, &id_a, &task_a, "done", Duration::from_secs(15)).await;

    // Crewmate B should have a task within 5 s (triggered by Crewmate A's completion).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = client
            .call("crewmate_tasks", json!({"id": id_b, "limit": 10}))
            .await
            .unwrap();
        if tasks["tasks"].as_array().map(|a| a.len()).unwrap_or(0) > 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "crewmate B must get a chained task within 5 s of crewmate A finishing"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── Test CrewmateUsage: after a task with usage, crewmate_usage returns record ──────
