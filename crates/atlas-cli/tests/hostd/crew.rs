//! hostd and the crew: crewmates as their own users, and the engine login asked of the daemon.

use std::process::{Command, Stdio};

use atlas_host::HostClient;
use serde_json::json;

use crate::common::*;

// Incident (found by the real-systemd end-to-end test, scripts/crew-e2e.sh): `atlas hostd` never told the
// daemon about the helper (`ServeOpts::isolation` was left at its default), so on a real crew server with both
// helper sockets up every crewmate still ran as the crew user, with no Linux user of its own. The daemon must
// be isolated exactly when the helper's socket exists: with it, creating a crewmate goes to the provisioning
// service (here a path nobody listens on, so the create fails saying so); without it, it works as before.
#[tokio::test]
async fn hostd_runs_crewmates_as_their_own_users_when_the_helpers_socket_exists() {
    let create = |client: HostClient| async move {
        client
            .call(
                "crewmate_create",
                json!({"name": "Iso", "role": "r", "agent": "claude", "approval": "auto"}),
            )
            .await
    };
    let tmp = tempfile::tempdir().unwrap();

    // The helper's socket exists (bound, never served); the provisioning service is not there.
    let _spawn = std::os::unix::net::UnixListener::bind(tmp.path().join("spawn.sock")).unwrap();
    let dir = tmp.path().join("with-helper");
    let _d = spawn_with(
        &dir,
        &[
            ("ATLAS_CREW_HOST", "1"),
            (
                "ATLAS_CREW_SPAWN_SOCK",
                tmp.path().join("spawn.sock").to_str().unwrap(),
            ),
            (
                "ATLAS_CREW_PROVISION_SOCK",
                tmp.path().join("no-such.sock").to_str().unwrap(),
            ),
        ],
    );
    let err = create(connect_when_up(&dir).await)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("cannot create the crewmate's Linux user"),
        "with the helper's socket present the crewmate must be provisioned as its own user: {err}"
    );

    // No helper socket: the crewmate is the crew user's, as on a developer's machine.
    let dir = tmp.path().join("without-helper");
    let _d = spawn_with(
        &dir,
        &[
            ("ATLAS_CREW_HOST", "1"),
            (
                "ATLAS_CREW_SPAWN_SOCK",
                tmp.path().join("absent.sock").to_str().unwrap(),
            ),
        ],
    );
    let made = create(connect_when_up(&dir).await).await.unwrap();
    assert!(made["id"].is_string(), "{made}");
}

// Security review 4, C2: `atlas crew login --crewmate` runs this on the server. It must ask hostd for the sign-in
// (the request the phone sends) and carry nothing else: on a server where crewmates are not their own Linux users
// hostd refuses it, and the command must fail with that reason, not fall back to starting the engine itself or to the
// crew user's own sign-in. No terminal may have been opened.
#[tokio::test]
async fn crew_login_asks_hostd_and_never_starts_the_engine_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    // A crew server whose crewmates are the crew user's own (no helper socket).
    let _d = spawn_with(
        &dir,
        &[
            ("ATLAS_CREW_HOST", "1"),
            (
                "ATLAS_CREW_SPAWN_SOCK",
                tmp.path().join("absent.sock").to_str().unwrap(),
            ),
        ],
    );
    let c = connect_when_up(&dir).await;
    let out = Command::new(BIN)
        .args([
            "hostd",
            "--crew-login",
            "claude",
            "--login-as",
            "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b",
            "--dir",
        ])
        .arg(&dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("as their own Linux user"), "{err}");
    let ptys = c.call("pty_list", json!({})).await.unwrap();
    assert_eq!(ptys["ptys"].as_array().unwrap().len(), 0, "{ptys}");
}
