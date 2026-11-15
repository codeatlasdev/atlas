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
                // `admin` is the desktop's own field (the address the install ran against): the relay must keep
                // reading an entry that has it.
                {"host": "atlas-crew@box", "name": "Ship", "admin": {"target": "ubuntu@box", "port": 2222}},
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

/// The crew server's `push/tokens.json` entries as `(token, node_id)`.
#[cfg(unix)]
fn crew_tokens(dir: &Path) -> Vec<(String, String)> {
    let v: Vec<Value> = std::fs::read(dir.join("push/tokens.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    v.iter()
        .map(|t| {
            (
                t["token"].as_str().unwrap().to_string(),
                t["node_id"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

// Guards PhoneCrewAudit gap 6: a phone's push token relayed through the Mac (`crew_call`) reached
// the crew server with node id "" (the relay's connection is no phone), so revoking the phone on the
// Mac never took it out and the revoked phone kept getting crew pushes. The real path: a paired phone
// over iroh registers through `crew_call`, then the Mac revokes it.
// Unix like `mobile::tests`, whose real phone it uses.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn revoking_a_phone_on_the_mac_clears_its_tokens_on_the_crew_server() {
    let r = Rig::new().await;
    let info = crate::mobile::tests::pair_start(&r.mac).await;
    let mut phone = crate::mobile::tests::Phone::dial(&info).await;
    let paired = phone
        .call("pair", json!({"code": info["code"], "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(paired["ok"], true, "{paired}");
    let node = paired["data"]["device"]["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    // Another phone's token, registered by a client that is no phone, must stay.
    r.crewmate
        .call(
            "push_register",
            json!({"token": "fcm-other", "platform": "fcm", "grant": "g"}),
        )
        .await
        .unwrap();
    let reg = phone
        .call(
            "crew_call",
            json!({"server": "atlas-crew@box", "cmd": "push_register",
                   "args": {"token": "fcm-1", "platform": "fcm", "grant": "g1"}}),
        )
        .await
        .unwrap();
    assert_eq!(reg["ok"], true, "{reg}");
    let crew_dir = r._tmp.path().join("crewmate");
    assert!(
        crew_tokens(&crew_dir).contains(&("fcm-1".into(), node.clone())),
        "the relayed token must carry the phone's node id: {:?}",
        crew_tokens(&crew_dir)
    );

    r.mac
        .call("mobile_revoke", json!({"node_id": node}))
        .await
        .unwrap();
    let mut left = Vec::new();
    for _ in 0..100 {
        left = crew_tokens(&crew_dir);
        if !left.iter().any(|(t, _)| t == "fcm-1") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        left.iter().all(|(t, _)| t != "fcm-1"),
        "the revoked phone still has a token on the crew server: {left:?}"
    );
    assert!(left.iter().any(|(t, _)| t == "fcm-other"), "{left:?}");
}

// Issue #355: the relay dialled every crew server on port 22. The real `ssh_link` runs against a fake `ssh` that
// records its argv: the probe and the tunnel of a server registered on 2222 both carry `-p 2222` before the
// target, and a server without a port gets no `-p` (a port in ~/.ssh/config still applies).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_relay_dials_a_crew_server_on_its_registered_port() {
    use std::os::unix::fs::PermissionsExt as _;
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let calls = tmp.path().join("calls");
    let fake = bin.join("ssh");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\037' \"$a\"; done >> '{c}'\necho >> '{c}'\n\
             case \"$*\" in *' -N '*|*'-N'*) exit 0;; esac\nprintf '/home/u\\ntok\\n'\n",
            c = calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let registry = tmp.path().join("crew-servers.json");
    std::fs::write(
        &registry,
        json!([
            {"host": "atlas-crew@alt", "name": "Alt", "port": 2222},
            {"host": "atlas-crew@plain", "name": "Plain"},
        ])
        .to_string(),
    )
    .unwrap();
    // The fake is found through PATH, as the real ssh is; this test is the only one that sets it.
    let old = std::env::var("PATH").unwrap_or_default();
    // SAFETY: no other test in this binary reads PATH while this one runs its two connects.
    unsafe { std::env::set_var("PATH", format!("{}:{old}", bin.display())) };
    for host in ["atlas-crew@alt", "atlas-crew@plain"] {
        let _ = super::ssh_link(registry.clone(), tmp.path().join("links"), host.to_string()).await;
    }
    unsafe { std::env::set_var("PATH", old) };
    let seen: Vec<Vec<String>> = std::fs::read_to_string(&calls)
        .unwrap()
        .lines()
        .map(|l| {
            l.split('\x1f')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .collect();
    let for_host = |h: &str| -> Vec<&Vec<String>> {
        seen.iter().filter(|c| c.iter().any(|a| a == h)).collect()
    };
    let alt = for_host("atlas-crew@alt");
    assert_eq!(alt.len(), 2, "a probe and a tunnel: {seen:?}");
    for c in alt {
        let p = c
            .iter()
            .position(|a| a == "-p")
            .expect("-p on every call to the alt server");
        assert_eq!(c[p + 1], "2222");
        assert!(p < c.iter().position(|a| a == "atlas-crew@alt").unwrap());
    }
    for c in for_host("atlas-crew@plain") {
        assert!(!c.iter().any(|a| a == "-p"), "{c:?}");
    }
}
