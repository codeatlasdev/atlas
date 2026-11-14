//! Setting up Telegram from a client: `crew_telegram_status` / `_set` / `_link` against a real hostd,
//! and the status payload Atlas Desktop's Telegram settings read.

use serde_json::{Value, json};

use super::fixture::daemon;

// Incident: the Telegram bridge worked but nothing could set it up (no client ever called
// `crew_telegram_set`), and the docs described a settings screen that did not exist. The desktop
// screen needs to know whether a token is set without ever reading it back.
#[tokio::test(flavor = "multi_thread")]
async fn telegram_is_set_up_linked_and_cleared_without_the_token_ever_coming_back() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let status = || async {
        client
            .call("crew_telegram_status", json!({}))
            .await
            .unwrap()
    };

    assert_eq!(status().await, json!({"configured": false, "chats": 0}));

    let token = "123456:secret-token";
    client
        .call("crew_telegram_set", json!({"token": token}))
        .await
        .unwrap();
    let configured = status().await;
    assert_eq!(configured, json!({"configured": true, "chats": 0}));
    assert!(!configured.to_string().contains("secret-token"));

    let code = client.call("crew_telegram_link", json!({})).await.unwrap();
    let code = code["code"].as_str().unwrap();
    assert!(
        code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()),
        "{code}"
    );

    // An empty token clears it (the settings' Remove button sends `null`).
    client
        .call("crew_telegram_set", json!({"token": Value::Null}))
        .await
        .unwrap();
    assert_eq!(status().await, json!({"configured": false, "chats": 0}));

    // The payload the desktop parses (`crew/telegram.test.ts`), from this real reply.
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/__fixtures__/crew_telegram_status.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&configured).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&fixture_path).expect(
        "fixture missing — run: UPDATE_FIXTURES=1 cargo test -p atlas-host telegram_is_set_up_linked",
    ))
    .unwrap();
    assert_eq!(
        stored, configured,
        "crew_telegram_status.json changed: rerun with UPDATE_FIXTURES=1"
    );

    client.call("shutdown", json!({})).await.ok();
}
