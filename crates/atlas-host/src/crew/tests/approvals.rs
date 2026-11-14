//! Approval prompts of a crewmate's pane: held for a client that can answer, and how long they wait.

use std::time::Duration;

use serde_json::json;

#[tokio::test]
async fn crewmate_pane_approval_held_with_desktop_client_not_held_without() {
    use crate::agents::AgentHub;
    use std::sync::Arc;

    let hub = Arc::new(AgentHub::default());

    // No subscribers, no telegram, no push tokens: ask returns None immediately.
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        hub.ask(
            "crew:testbot:task1",
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

#[test]
fn crewmate_pane_decision_wait_is_longer() {
    use crate::agents::hook_server::{CREWMATE_DECISION_WAIT, DECISION_WAIT};
    assert!(
        CREWMATE_DECISION_WAIT > DECISION_WAIT,
        "CREWMATE_DECISION_WAIT ({:?}) must exceed DECISION_WAIT ({:?})",
        CREWMATE_DECISION_WAIT,
        DECISION_WAIT,
    );
    // 30 minutes.
    assert_eq!(CREWMATE_DECISION_WAIT.as_secs(), 30 * 60);
}

// ── Test: omp extension file is written into crewmate_dir when approval==ask ──

/// POST a permission prompt for crewmate pane `pane` to the hook server of the hostd in `dir`
/// (what a crewmate's claude does through `atlas:approve`) and return the reply body.
async fn decision(dir: &std::path::Path, pane: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = json!({"pane_key": pane, "payload": {"tool_name": "Bash", "tool_input": {"command": "ls"}}})
        .to_string();
    let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT")))
        .await
        .unwrap();
    let req = format!(
        "POST /hook/claude/decision HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).await.unwrap();
    r.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

// Crew review P0, PR #106 comment 6874910355: a registered push token alone counted as "someone can
// answer", so with the only phone offline (or its Mac off) a crewmate's tool prompt waited the full
// 30 minutes for an answer nobody could give, then failed the task. A token is a way to notify, not
// to answer: only a live client, a phone connected now, or a linked Telegram chat count.
#[tokio::test(flavor = "multi_thread")]
async fn a_push_token_alone_is_nobody_who_can_answer_a_crewmate_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = super::fixture::daemon(&dir).await;
    client
        .call(
            "push_register",
            json!({"token": "apns-offline-phone", "platform": "ios"}),
        )
        .await
        .unwrap();
    // While a desktop client is connected the prompt is held for it (that is what it is for).
    // Close our only connection, as a closed laptop would: the token stays registered.
    drop(client);

    // The daemon notices the disconnect within moments; from then on the ask must return at once.
    // Each attempt is bounded, so a regression (a 30-minute wait) fails here instead of hanging.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let attempt =
            tokio::time::timeout(Duration::from_millis(1500), decision(&dir, "crew:c1:t1")).await;
        match attempt {
            Ok(body) => {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&body).unwrap(),
                    json!({"decision": null}),
                    "nobody can answer: the prompt must come back with no decision"
                );
                return;
            }
            Err(_) if std::time::Instant::now() < deadline => {
                // Still held for the client that was connected a moment ago; try again.
                continue;
            }
            Err(_) => panic!(
                "a crewmate prompt with only a push token registered was held, waiting for an answer nobody can give"
            ),
        }
    }
}

// The other side of the same rule: a connected desktop client IS someone who can answer, so the
// prompt is held (it would be answered from the client). Guards against fixing the push-token bug by
// refusing every prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_connected_client_still_holds_a_crewmate_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = super::fixture::daemon(&dir).await;

    let held =
        tokio::time::timeout(Duration::from_millis(1500), decision(&dir, "crew:c1:t2")).await;
    assert!(
        held.is_err(),
        "with a live client the prompt must be held for it"
    );
    client.call("shutdown", json!({})).await.ok();
}
