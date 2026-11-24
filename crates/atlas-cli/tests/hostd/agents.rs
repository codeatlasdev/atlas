//! Agent state in the daemon: integrations, which terminal reports an agent, hook events pushed to clients.

use std::process::Command;
use std::time::Duration;

use atlas_host::paths;
use serde_json::{Value, json};

use crate::common::*;

// Guards a phone that cannot tell why an agent never shows: hooks off is invisible from
// the phone, so hostd reports per installed agent whether its integration is on.
#[tokio::test]
async fn agent_integrations_report_installed_agents_and_whether_hooks_are_on() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, home, bin) = (
        tmp.path().join("d"),
        tmp.path().join("home"),
        tmp.path().join("bin"),
    );
    for d in [&dir, &home, &bin] {
        std::fs::create_dir_all(d).unwrap();
    }
    // A fake `claude` on PATH; no omp, no codex.
    let claude = bin.join("claude");
    std::fs::write(&claude, "#!/bin/sh\necho 2.0.0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let cfg = home.join(".claude");
    let env = [
        ("HOME", home.to_str().unwrap()),
        ("PATH", path.as_str()),
        ("SHELL", "/bin/sh"),
        ("CLAUDE_CONFIG_DIR", cfg.to_str().unwrap()),
    ];
    let _d = spawn_with(&dir, &env);
    let c = connect_when_up(&dir).await;
    // hostd resolves agents through the login PATH, so a dev machine with omp or codex in
    // /opt/homebrew/bin lists them too: check claude by id, not the whole list.
    let claude = async || {
        c.call("agent_integrations", json!({})).await.unwrap()["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "claude")
            .cloned()
            .expect("claude on PATH is listed")
    };
    assert_eq!(
        claude().await,
        json!({"id": "claude", "label": "Claude Code", "setup": "claude-code", "state": "missing"})
    );
    let mut setup = Command::new(BIN);
    setup
        .args(["setup", "claude-code", "--apply"])
        .current_dir(&home);
    for (k, v) in env {
        setup.env(k, v);
    }
    let o = setup.output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(claude().await["state"], "ok");
}

// Guards agents invisible to Atlas and the phone: only the app's agent launcher set
// ATLAS_HOOK_ENDPOINT / ATLAS_PANE_KEY, so omp or claude typed into a plain terminal sent
// no status at all (the omp extension returns without ATLAS_HOOK_ENDPOINT).
#[tokio::test]
async fn every_terminal_of_a_pane_can_report_its_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let mut ev = c.events();
    let out = |pane: Option<&str>, env: Value| {
        // `read` keeps the terminal alive until hostd closes it: with `sleep 1` it could end
        // before the attach on a loaded runner (#198).
        let mut args = json!({"cwd": dir, "argv": ["sh", "-c", "echo \"[$ATLAS_PANE_KEY|$ATLAS_HOOK_ENDPOINT]\"; read x"], "env": env});
        if let Some(p) = pane {
            args["scope"] = json!({"pane_key": p});
        }
        args
    };
    let mut read = async |args: Value| {
        let pty = c.call("pty_create", args).await.unwrap()["pty"]
            .as_str()
            .unwrap()
            .to_string();
        c.call("pty_attach", json!({"pty": pty, "replay": true}))
            .await
            .unwrap();
        let mut text = String::new();
        while !text.contains(']') {
            let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
                .await
                .unwrap()
                .unwrap();
            if e["pty"] == pty && matches!(e["ev"].as_str(), Some("pty_output" | "pty_replay")) {
                text.push_str(&b64(&e["data"]));
            }
        }
        text
    };
    let endpoint = paths::hook_endpoint(dir).to_string_lossy().into_owned();
    let t = read(out(Some("tab-1"), json!({}))).await;
    assert!(t.contains(&format!("[tab-1|{endpoint}]")), "{t:?}");
    // The launcher's own values win (an agent tab of another pane key).
    let t = read(out(Some("tab-2"), json!({"ATLAS_PANE_KEY": "agent-9"}))).await;
    assert!(t.contains("[agent-9|"), "{t:?}");
    // No pane, nothing to report to.
    let t = read(out(None, json!({}))).await;
    assert!(t.contains("[|]"), "{t:?}");
}

// Guards a phone showing a dead agent as working forever: only the desktop told hostd
// a pane's process exited (`agent_pane_exited`), so with the app closed, the case agent
// state moved to hostd for, the pane never went back to idle.
#[tokio::test]
async fn an_agent_pane_whose_process_exits_goes_idle_without_the_app() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    // The process exits only after `go`, created once "working" is checked: with `sleep 1` a
    // loaded runner saw it already idle (#198).
    let go = dir.join("go-exit");
    let script = format!("while [ ! -e '{}' ]; do sleep 0.01; done", go.display());
    c.call(
        "pty_create",
        json!({"cwd": dir, "argv": ["sh", "-c", script], "scope": {"pane_key": "p1"}}),
    )
    .await
    .unwrap();
    let r = post_hook(
        dir,
        "omp",
        &json!({"pane_key": "p1", "payload": {"event": "agent_start"}}),
        None,
    );
    assert!(r.starts_with("HTTP/1.1 2"), "{r}");
    let state = || async {
        c.call("agent_status_all", json!({})).await.unwrap()["panes"]["p1"]["state"].clone()
    };
    assert_eq!(state().await, "working");
    std::fs::write(&go, "").unwrap();
    let mut s = Value::Null;
    for _ in 0..50 {
        s = state().await;
        if s == "idle" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(s, "idle");
}

/// Agent state lives in hostd (not the app) so it survives the window closing and
/// every client sees it: a hook POST and a pane exit both reach clients as `agent_status`.
#[tokio::test]
async fn hook_events_drive_agent_status_pushed_to_clients() {
    let tmp = tempfile::tempdir().unwrap();
    let (_d, c) = start(tmp.path()).await;
    let mut ev = c.events();
    let env = std::fs::read_to_string(paths::hook_endpoint(tmp.path())).unwrap();
    let port = env
        .lines()
        .find_map(|l| l.strip_prefix("ATLAS_HOOK_PORT="))
        .unwrap()
        .to_string();
    let hi = json!({"pane_key":"p1","payload":{"event":"agent_start","prompt":"hi"}});
    assert!(post_hook(tmp.path(), "omp", &hi, Some("wrong")).starts_with("HTTP/1.1 401"));
    assert!(post_hook(tmp.path(), "omp", &hi, None).starts_with("HTTP/1.1 204"));
    async fn next(
        ev: &mut tokio::sync::broadcast::Receiver<atlas_host::client::HostEvent>,
    ) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let e = ev.recv().await.unwrap();
                if e["ev"] == "agent_status" {
                    return (*e).clone();
                }
            }
        })
        .await
        .expect("agent_status push")
    }
    let working = next(&mut ev).await;
    assert_eq!(working["pane_key"], "p1");
    assert_eq!(working["status"]["state"], "working");
    assert_eq!(working["status"]["prompt"], "hi");

    let mut ev = c.events();
    c.call("agent_pane_exited", json!({"pane": "p1"}))
        .await
        .unwrap();
    let idle = next(&mut ev).await;
    assert_eq!(idle["status"]["state"], "idle");
    let all = c.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(all["panes"]["p1"]["state"], "idle");
    assert_eq!(all["hook_port"].to_string(), port);
}
