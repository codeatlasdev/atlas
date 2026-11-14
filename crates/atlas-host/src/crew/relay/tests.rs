//! A real Mac daemon relaying to a real crewmate-computer daemon (both `serve_with` on temp dirs);
//! only ssh is replaced, by a connector that dials the crew server's socket directly.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Connector, Link};
use crate::HostClient;
use crate::mobile::{Scope, line_scope, required_scope};
use crate::protocol::HostCmd;
use crate::server::ServeOpts;

async fn connect(dir: &Path) -> HostClient {
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

async fn daemon(dir: &Path, opts: ServeOpts) -> HostClient {
    let d = dir.to_path_buf();
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    connect(dir).await
}

/// "ssh" to `box` = the crew server's own socket in `crewmate_dir`.
fn direct(crewmate_dir: PathBuf) -> Connector {
    Connector(Arc::new(move |host| {
        let dir = crewmate_dir.clone();
        Box::pin(async move {
            assert_eq!(host, "atlas-crew@box");
            let client = HostClient::connect_dir(&dir, "crew")
                .await
                .map_err(|e| e.to_string())?;
            Ok(Link { client, keep: None })
        })
    }))
}

struct Rig {
    _tmp: tempfile::TempDir,
    crewmate: HostClient,
    mac: HostClient,
}

impl Rig {
    async fn new() -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let crewmate_dir = tmp.path().join("crewmate");
        let crewmate = daemon(
            &crewmate_dir,
            ServeOpts {
                no_mobile_relay: true,
                crew_host: true,
                ..Default::default()
            },
        )
        .await;
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("desktop")).unwrap();
        std::fs::write(
            home.join("desktop/crew-servers.json"),
            json!([
                {"host": "atlas-crew@box", "name": "Ship"},
                {"host": "-oProxyCommand=evil", "name": "never dialled"},
            ])
            .to_string(),
        )
        .unwrap();
        let mac = daemon(
            &tmp.path().join("mac"),
            ServeOpts {
                no_mobile_relay: true,
                registry: Some(home),
                crew_relay: Some(direct(crewmate_dir)),
                ..Default::default()
            },
        )
        .await;
        Rig {
            _tmp: tmp,
            crewmate,
            mac,
        }
    }
}

// Guards 2026-11-12: the crewmates live on a crew server only the desktop reached (ssh), so
// Atlas Mobile, which talks to the Mac alone, showed an empty Crew.
#[tokio::test(flavor = "multi_thread")]
async fn the_mac_runs_a_phone_s_crewmate_command_on_the_crew_server() {
    let r = Rig::new().await;
    r.crewmate
        .call(
            "crewmate_create",
            json!({"name": "Jorge", "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .unwrap();
    // The Mac itself is no crew server: only the relay reaches the crew.
    assert!(r.mac.call("crewmate_list", json!({})).await.is_err());
    let servers = r.mac.call("crew_servers", json!({})).await.unwrap();
    assert_eq!(
        servers["servers"],
        json!([{"id": "atlas-crew@box", "name": "Ship", "state": "connecting"}]),
        "an ssh-unsafe entry is never listed: {servers}"
    );
    let list = r
        .mac
        .call(
            "crew_call",
            json!({"server": "atlas-crew@box", "cmd": "crewmate_list", "args": {}}),
        )
        .await
        .unwrap();
    assert_eq!(list["crewmates"][0]["name"], "Jorge", "{list}");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_crew_servers_in_the_desktop_registry_are_reachable() {
    let r = Rig::new().await;
    let e = r
        .mac
        .call(
            "crew_call",
            json!({"server": "-oProxyCommand=evil", "cmd": "crewmate_list", "args": {}}),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown crew server"), "{e}");
}

/// A crew server's pushes reach the Mac's clients wrapped with the server they came from.
#[tokio::test(flavor = "multi_thread")]
async fn the_crew_server_s_pushes_are_relayed_with_their_server() {
    let r = Rig::new().await;
    let mut events = r.mac.events();
    // Connect first (a crew call waits for the link), then make the crew server push.
    r.mac
        .call(
            "crew_call",
            json!({"server": "atlas-crew@box", "cmd": "crewmate_list", "args": {}}),
        )
        .await
        .unwrap();
    r.crewmate
        .call(
            "mobile_invite",
            json!({"url": "atlas-mobile://x", "name": "Ship"}),
        )
        .await
        .unwrap();
    let ev: Value = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let e = events.recv().await.unwrap();
            if e["ev"] == "crew_event" {
                return (*e).clone();
            }
        }
    })
    .await
    .expect("no crew_event");
    assert_eq!(ev["server"], "atlas-crew@box");
    assert_eq!(ev["event"]["ev"], "computer_invite", "{ev}");
}

fn crew_call(cmd: &str, args: Value) -> HostCmd {
    serde_json::from_value(json!({
        "cmd": "crew_call",
        "args": {"server": "s", "cmd": cmd, "args": args},
    }))
    .unwrap()
}

/// A relayed command needs the scope it would need sent to the Mac directly; terminals and
/// nested relays are never relayed.
#[test]
fn a_relayed_command_needs_its_own_scope() {
    assert_eq!(
        required_scope(&crew_call("crewmate_list", json!({}))),
        Ok(Scope::Read)
    );
    assert_eq!(
        required_scope(&crew_call(
            "crewmate_task",
            json!({"id": "b", "prompt": "p", "source": "mobile"})
        )),
        Ok(Scope::Act)
    );
    assert_eq!(
        required_scope(&crew_call(
            "agent_approve",
            json!({"pane": "p", "req": "r", "allow": true})
        )),
        Ok(Scope::Approve)
    );
    for (cmd, args) in [
        ("pty_write", json!({"pty": "x", "data": ""})),
        ("pty_attach", json!({"pty": "x"})),
        (
            "crewmate_secret_set",
            json!({"id": "b", "key": "k", "value": "v"}),
        ),
        ("crew_servers", json!({})),
        (
            "crew_call",
            json!({"server": "s", "cmd": "crewmate_list", "args": {}}),
        ),
        ("no_such_command", json!({})),
    ] {
        assert!(
            required_scope(&crew_call(cmd, args)).is_err(),
            "{cmd} must not relay"
        );
    }
}

#[test]
fn a_relayed_push_needs_the_scope_of_the_push_inside() {
    let wrap =
        |ev: &str| json!({"ev": "crew_event", "server": "s", "event": {"ev": ev}}).to_string();
    assert_eq!(line_scope(&wrap("crewmate_task")), Some(Scope::Read));
    assert_eq!(line_scope(&wrap("agent_approval")), Some(Scope::Approve));
    assert_eq!(line_scope(&wrap("pty_output")), None);
    assert_eq!(line_scope(&wrap("fs_changed")), None);
    assert_eq!(line_scope(r#"{"ev":"crewmate_task"}"#), Some(Scope::Read));
}

// Guards the rename Bot -> Crew: a Mac whose desktop has not run since the update still has the
// registry under its old name (`desktop/bot-hosts.json`); the phone must keep seeing its crew servers.
#[tokio::test(flavor = "multi_thread")]
async fn a_registry_still_under_its_old_name_is_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join("desktop")).unwrap();
    std::fs::write(
        home.join("desktop/bot-hosts.json"),
        json!([{"host": "atlas-bot@box", "name": "Ship"}]).to_string(),
    )
    .unwrap();
    let mac = daemon(
        &tmp.path().join("mac"),
        ServeOpts {
            no_mobile_relay: true,
            registry: Some(home.clone()),
            crew_relay: Some(direct(tmp.path().join("nowhere"))),
            ..Default::default()
        },
    )
    .await;
    let servers = mac.call("crew_servers", json!({})).await.unwrap();
    assert_eq!(servers["servers"][0]["id"], "atlas-bot@box", "{servers}");
    assert!(home.join("desktop/crew-servers.json").exists());
    assert!(!home.join("desktop/bot-hosts.json").exists());
}
