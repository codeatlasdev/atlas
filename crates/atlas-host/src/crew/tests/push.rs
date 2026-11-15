//! `push_register`: what a crew server keeps of a phone's registration (the FCM token and the grant
//! the push relay issued for it, issue #205) and the wire shape the phone sends.

use serde_json::{Value, json};

use super::fixture::daemon;
use crate::protocol::HostCmd;

fn stored(dir: &std::path::Path) -> Vec<Value> {
    let b = std::fs::read(dir.join("push").join("tokens.json")).unwrap_or_default();
    serde_json::from_slice(&b).unwrap_or_default()
}

// #205: the grant must reach tokens.json, or hostd could never authenticate a notify to the relay.
#[tokio::test(flavor = "multi_thread")]
async fn push_register_stores_the_grant_with_the_token() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    client
        .call(
            "push_register",
            json!({"token": "fcm-1", "platform": "fcm", "grant": "grant-1"}),
        )
        .await
        .unwrap();
    let t = stored(&dir);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0]["token"], "fcm-1");
    assert_eq!(t[0]["grant"], "grant-1");
}

// Transition: an app from before grants sends no `grant`. It must still register (it will not get
// notified until it updates and re-registers), not fail with a parse error.
#[tokio::test(flavor = "multi_thread")]
async fn push_register_without_a_grant_still_registers() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    client
        .call("push_register", json!({"token": "old", "platform": "fcm"}))
        .await
        .unwrap();
    let t = stored(&dir);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0]["grant"], "");
}

// Re-registering the same token (token refresh, a relaunch after an upgrade) replaces the entry:
// the old grantless one must not linger beside the new one.
#[tokio::test(flavor = "multi_thread")]
async fn re_registering_a_token_replaces_its_grant() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    client
        .call("push_register", json!({"token": "t", "platform": "fcm"}))
        .await
        .unwrap();
    client
        .call(
            "push_register",
            json!({"token": "t", "platform": "fcm", "grant": "g"}),
        )
        .await
        .unwrap();
    let t = stored(&dir);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0]["grant"], "g");
}

// Contract with the phone. `apps/mobile/test/fixtures/push_register.json` is the args map the phone's
// registration code really builds (the Flutter test writes it; hostd's `HostCmd` is Deserialize
// only, so the real parser is the thing to check here). A field renamed on one side fails both.
#[test]
fn the_phones_push_register_args_parse_into_the_command() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/push_register.json");
    let args: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect(
        "fixture missing: run `flutter test test/push_registration_test.dart` with UPDATE_FIXTURES=1",
    ))
    .unwrap();
    let wire = json!({"cmd": "push_register", "args": args});
    match serde_json::from_value::<HostCmd>(wire).expect("the phone's push_register must parse") {
        HostCmd::PushRegister {
            token,
            platform,
            grant,
            node,
        } => {
            assert!(!token.is_empty() && platform == "fcm");
            assert!(!grant.is_empty(), "the phone must send the grant");
            // The phone never names itself: its connection (or the Mac's relay) does.
            assert!(node.is_empty(), "the phone's args carry no node");
        }
        other => panic!("parsed as {other:?}"),
    }
}

// Incident (#291): a `tokens.json` that failed to parse loaded as "no phones"; the next register wrote a
// file with one entry and every other phone stopped getting notified, with nothing in any log. The
// unreadable file is set aside, and the phone that registers next starts a clean one.
#[tokio::test(flavor = "multi_thread")]
async fn a_corrupt_push_tokens_file_is_set_aside_not_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    std::fs::create_dir_all(dir.join("push")).unwrap();
    let torn = br#"[{"node_id":"phone:a","token":"fcm-old","platfo"#;
    std::fs::write(dir.join("push").join("tokens.json"), torn).unwrap();

    let client = daemon(&dir).await;
    client
        .call(
            "push_register",
            json!({"token": "fcm-new", "platform": "fcm"}),
        )
        .await
        .unwrap();

    let aside: Vec<_> = std::fs::read_dir(dir.join("push"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("tokens.json.corrupt-")
        })
        .collect();
    assert_eq!(aside.len(), 1, "the unreadable file is kept aside");
    assert_eq!(std::fs::read(aside[0].path()).unwrap(), torn);
    let t = stored(&dir);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0]["token"], "fcm-new");
}
