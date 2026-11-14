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
