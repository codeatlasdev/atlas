use serde_json::json;

use super::*;
use crate::config::ProjectConfig;

fn cfg() -> ProjectConfig {
    serde_norway::from_str(
        r#"
name: demo
services:
  api: { command: "sleep 1", port: 3000 }
  web: { command: "sleep 1" }
actions:
  - { key: m, label: Migrate, command: "true" }
  - { key: x, label: Wipe DB, command: "true", confirm: "This wipes the DB." }
"#,
    )
    .unwrap()
}

#[test]
fn parse_requires_service_arg() {
    assert!(ControlCmd::parse("restart_service", &json!({})).is_err());
    assert!(ControlCmd::parse("restart_service", &json!({"service": "  "})).is_err());
    assert_eq!(
        ControlCmd::parse("stop_service", &json!({"service": "api"})).unwrap(),
        ControlCmd::StopService("api".into())
    );
}

#[test]
fn parse_rejects_unknown_cmd() {
    let e = ControlCmd::parse("rm_rf", &json!({})).unwrap_err();
    assert!(e.contains("unknown cmd"));
}

#[test]
fn resolve_rejects_unknown_service_listing_known() {
    let e = resolve(ControlCmd::RestartService("nope".into()), &cfg()).unwrap_err();
    assert!(e.contains("api, web"), "{e}");
}

#[test]
fn resolve_action_by_key_or_label() {
    let by_key = resolve(
        ControlCmd::RunAction {
            target: "m".into(),
            confirmed: false,
        },
        &cfg(),
    )
    .unwrap();
    let by_label = resolve(
        ControlCmd::RunAction {
            target: "migrate".into(),
            confirmed: false,
        },
        &cfg(),
    )
    .unwrap();
    assert_eq!(by_key, by_label);
}

#[test]
fn confirm_action_refused_without_confirmed_and_returns_text() {
    let cmd = |confirmed| ControlCmd::RunAction {
        target: "x".into(),
        confirmed,
    };
    let e = resolve(cmd(false), &cfg()).unwrap_err();
    assert!(e.contains("This wipes the DB."), "{e}");
    assert!(e.contains("confirmed: true"), "{e}");
    assert_eq!(
        resolve(cmd(true), &cfg()).unwrap(),
        Resolved::RunAction {
            key: 'x',
            label: "Wipe DB".into()
        }
    );
}

#[tokio::test]
async fn call_without_server_is_not_running() {
    let dir = tempfile::TempDir::new().unwrap();
    let e = call(dir.path(), "restart_service", json!({"service": "api"}))
        .await
        .unwrap_err();
    assert_eq!(e, NOT_RUNNING);
}

// Mode bits: Unix sockets only (Windows serves a named pipe).
#[cfg(unix)]
#[tokio::test]
async fn socket_roundtrip_perms_and_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    // A stale socket file from a crashed run must be replaced.
    std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
    std::fs::write(socket_path(dir.path()), "stale").unwrap();

    let (server, mut rx) = ControlServer::bind(dir.path()).unwrap();
    let mode = std::fs::metadata(socket_path(dir.path()))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let root = dir.path().to_path_buf();
    let client =
        tokio::spawn(
            async move { call(&root, "restart_service", json!({"service": "api"})).await },
        );
    let req = rx.recv().await.unwrap();
    assert_eq!(req.cmd, ControlCmd::RestartService("api".into()));
    req.reply
        .send(ControlResponse::ok(json!({"x": 1})))
        .unwrap();
    let resp = client.await.unwrap().unwrap();
    assert!(resp.ok);
    assert_eq!(resp.data.unwrap()["x"], 1);

    // Malformed input gets an error response, not a hang or a loop request.
    let bad = call(dir.path(), "nope", json!({})).await.unwrap();
    assert!(!bad.ok);
    assert!(rx.try_recv().is_err());

    drop(server);
    assert!(!socket_path(dir.path()).exists());
}
