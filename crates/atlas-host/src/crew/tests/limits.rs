//! The documented size limits of a crewmate (docs/crew.md): name 1–40, role ≤ 2000 and a prompt of
//! 1–20000 characters. They are characters, not bytes: a name of 40 accented letters is 80 bytes.

use serde_json::json;

use super::fixture::{create_crewmate, daemon};

// Crew review P0, PR #106 comment 6874910355: the limits were checked with `str::len()` (bytes), so
// a 40-letter name with accents was refused, and a 20000-character prompt limit did not exist.
#[tokio::test(flavor = "multi_thread")]
async fn name_and_role_limits_count_characters_not_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;

    // 40 characters, 80 bytes: allowed. 41 characters: refused.
    let name40 = "é".repeat(40);
    let v = client
        .call(
            "crewmate_create",
            json!({"name": name40, "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .expect("a 40-character name (80 bytes) must be accepted");
    let id = v["id"].as_str().unwrap().to_string();
    let too_long = client
        .call(
            "crewmate_create",
            json!({"name": "é".repeat(41), "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await;
    assert!(too_long.is_err(), "41 characters must be refused");

    // 2000 characters (6000 bytes): allowed. 2001: refused. Same on update.
    let role2000 = "ã".repeat(2000);
    client
        .call(
            "crewmate_create",
            json!({"name": "r", "role": role2000, "agent": "claude", "approval": "auto"}),
        )
        .await
        .expect("a 2000-character role (4000 bytes) must be accepted");
    client
        .call(
            "crewmate_create",
            json!({"name": "r2", "role": "ã".repeat(2001), "agent": "claude", "approval": "auto"}),
        )
        .await
        .expect_err("2001 characters must be refused");
    client
        .call("crewmate_update", json!({"id": id, "name": "é".repeat(40)}))
        .await
        .expect("update accepts a 40-character name too");
    client
        .call("crewmate_update", json!({"id": id, "name": "é".repeat(41)}))
        .await
        .expect_err("update refuses 41 characters");

    client.call("shutdown", json!({})).await.ok();
}

// Crew review P0: docs/crew.md says "Prompts are 1 to 20000 characters"; nothing enforced it, so a
// huge prompt reached the agent's argv and failed as a spawn error.
#[tokio::test(flavor = "multi_thread")]
async fn task_prompt_is_limited_to_20000_characters() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Limits").await;

    // 20000 multibyte characters (40000 bytes) are fine …
    let edge = "ç".repeat(20_000);
    let ok = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": edge, "source": "test"}),
        )
        .await;
    assert!(ok.is_ok(), "20000 characters must be accepted: {ok:?}");

    // … 20001 are not, and nothing is stored for the refused one.
    let err = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "ç".repeat(20_001), "source": "test"}),
        )
        .await
        .expect_err("20001 characters must be refused");
    assert!(err.to_string().contains("20000"), "{err}");
    let tasks = client
        .call("crewmate_tasks", json!({"id": id, "limit": 50}))
        .await
        .unwrap();
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 1, "{tasks}");

    // The limit is on the trimmed prompt: padding spaces do not count against it.
    let padded = format!("  {}  ", "x".repeat(20_000));
    client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": padded, "source": "test"}),
        )
        .await
        .expect("whitespace around a 20000-character prompt is trimmed first");

    client.call("shutdown", json!({})).await.ok();
}
