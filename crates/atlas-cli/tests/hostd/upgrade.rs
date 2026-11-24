//! Live upgrade: ptys and agents move to a new hostd; a failed or concurrent upgrade leaves one working daemon.

use std::process::Command;
use std::time::Duration;

use atlas_host::{HostClient, paths};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use crate::common::*;

// Guards "no agent running" after an upgrade: agent state lived only in the old
// daemon's memory, so every hostd upgrade forgot every agent until it next spoke,
// and the phone, which needs an agent to reach its terminal, had nothing to open.
#[tokio::test]
async fn agent_state_survives_an_upgrade() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let old_pid = c.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    c.call(
        "pty_create",
        json!({"cwd": dir, "argv": ["sh", "-c", "sleep 30"]}),
    )
    .await
    .unwrap();
    c.call(
        "agent_bind",
        json!({"pane": "p1", "agent": "omp", "cwd": "/w"}),
    )
    .await
    .unwrap();
    let r = post_hook(
        dir,
        "omp",
        &json!({"pane_key": "p1", "payload": {"event": "agent_end", "text": "done it"}}),
        None,
    );
    assert!(r.starts_with("HTTP/1.1 2"), "{r}");
    let before = c.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(before["panes"]["p1"]["state"], "done", "{before}");
    assert_eq!(before["panes"]["p1"]["last_message"], "done it", "{before}");

    c.call("upgrade", json!({"exe": BIN})).await.unwrap();
    let c2 = connect_retry(dir, old_pid).await;
    let _kill_new = Pid(c2.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap() as u32);
    let after = c2.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(after["panes"]["p1"], before["panes"]["p1"], "{after}");
    // The cwd binding moves too: a hook without a pane key still finds p1.
    let r = post_hook(
        dir,
        "omp",
        &json!({"payload": {"event": "agent_start", "cwd": "/w"}}),
        None,
    );
    assert!(r.starts_with("HTTP/1.1 2"), "{r}");
    let later = c2.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(later["panes"]["p1"]["state"], "working", "{later}");
}

// Regression: #670. `atlas update` with no terminal open failed every time ("waiting for the new hostd: failed to
// fill whole buffer"): the old daemon's empty first chunk was taken for the end of the handoff, the successor never
// received the pid-file lock, found it held and exited. Two real processes, no pty.
#[tokio::test]
async fn upgrade_with_no_terminal_hands_the_daemon_over_with_its_lock_and_agents() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;
    let old_pid = c.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    c.call(
        "agent_bind",
        json!({"pane": "p1", "agent": "omp", "cwd": "/w"}),
    )
    .await
    .unwrap();
    let r = post_hook(
        dir,
        "omp",
        &json!({"pane_key": "p1", "payload": {"event": "agent_end", "text": "done it"}}),
        None,
    );
    assert!(r.starts_with("HTTP/1.1 2"), "{r}");

    let r = c.call("upgrade", json!({"exe": BIN})).await.unwrap();
    assert_eq!(r["handed_off"], 0, "{r}");
    let c2 = connect_retry(dir, old_pid).await;
    let new_pid = c2.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    let _kill_new = Pid(new_pid as u32);
    assert_eq!(
        std::fs::read_to_string(paths::pid(dir)).unwrap().trim(),
        new_pid.to_string(),
        "the pid file names the successor"
    );
    wait_for("the old hostd to exit", || {
        d.0.try_wait().unwrap().is_some()
    })
    .await;
    let agents = c2.call("agent_status_all", json!({})).await.unwrap();
    assert_eq!(agents["panes"]["p1"]["state"], "done", "{agents}");
    assert_eq!(agents["panes"]["p1"]["last_message"], "done it", "{agents}");
}

#[tokio::test]
async fn upgrade_hands_live_ptys_to_a_new_hostd_without_killing_the_program() {
    use tokio::io::AsyncReadExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;
    let info = c.call("info", json!({})).await.unwrap();
    assert_eq!(info["proto"], 2);
    assert!(
        info["caps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "upgrade")
    );
    let old_pid = info["pid"].as_u64().unwrap();

    let create = |argv: Value| {
        let c = c.clone();
        async move {
            c.call(
                "pty_create",
                json!({"cwd": dir, "argv": argv, "cols": 100, "rows": 30}),
            )
            .await
            .unwrap()["pty"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let pty = create(json!(["sh", "-c", "echo before; read x; echo after:$x"])).await;
    // A finished pty must survive the handoff as exited, with its replay.
    let done = create(json!(["sh", "-c", "echo gone"])).await;
    wait_exited(&c, &done).await;
    let child_pid = c.call("pty_info", json!({"pty": pty})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();

    // Takeover with the same binary.
    let r = c.call("upgrade", json!({"exe": BIN})).await.unwrap();
    assert_eq!(r["handed_off"], 1, "{r}");
    let c2 = connect_retry(dir, old_pid).await;
    let new_pid = c2.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    let _kill_new = Pid(new_pid as u32);
    assert_ne!(new_pid, old_pid);
    // The old process is gone; the program is the very same process.
    // The old daemon is our child: reaping it proves it exited (a zombie would pass `kill -0`).
    let mut gone = false;
    for _ in 0..100 {
        if d.0.try_wait().unwrap().is_some() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the old hostd process must exit after the handoff");
    let list = c2.call("pty_list", json!({})).await.unwrap();
    let item = list["ptys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pty"] == pty.as_str())
        .unwrap();
    assert_eq!(item["pid"].as_u64().unwrap(), child_pid);
    assert!(item["exited"].is_null(), "{item}");
    let gone = list["ptys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pty"] == done.as_str())
        .unwrap();
    assert!(!gone["exited"].is_null(), "{gone}");
    let alive = Command::new("kill")
        .args(["-0", &child_pid.to_string()])
        .status()
        .unwrap();
    assert!(alive.success(), "the program must still be running");

    // Replay has the earlier output; IO continues through the new host.
    let mut raw = atlas_host::client::attach_raw(dir, &pty, true)
        .await
        .unwrap();
    assert_eq!(
        (raw.info["cols"].as_u64(), raw.info["rows"].as_u64()),
        (Some(100), Some(30))
    );
    c2.call("pty_write", json!({"pty": pty, "data": B64.encode("hi\n")}))
        .await
        .unwrap();
    let mut out = Vec::new();
    // EOF arrives when the (adopted) program exits.
    tokio::time::timeout(Duration::from_secs(10), raw.reader.read_to_end(&mut out))
        .await
        .expect("adopted pty should end after `after:hi`")
        .unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("before"), "{text:?}");
    assert!(text.contains("after:hi"), "{text:?}");
    // The exit of a non-child is detected. The code comes from kqueue on macOS; elsewhere
    // it may be null (unknown), never a wrong number.
    let mut after = Value::Null;
    for _ in 0..50 {
        after = c2.call("pty_info", json!({"pty": pty})).await.unwrap();
        if !after["exited"].is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!after["exited"].is_null(), "{after}");
    let code = &after["exited"]["code"];
    assert!(code.is_null() || *code == 0, "{after}");
    #[cfg(target_os = "macos")]
    assert_eq!(
        *code, 0,
        "kqueue reports the exit status of adopted programs"
    );

    // New terminals work on the successor too.
    let p3 = c2
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "echo fresh"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    // `echo fresh` ends by itself; its output is what is waited for, not a fixed time.
    let mut text = Value::Null;
    for _ in 0..200 {
        text = c2.call("pty_read", json!({"pty": p3})).await.unwrap()["text"].clone();
        if text == "fresh" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(text, "fresh");
}

/// Regression: #500. `atlas update` and the app both noticed the old daemon and called `upgrade` at
/// once; the second call rebound the first's handoff socket, so the daemon was handed over twice
/// (`hostd.log`: two `took over 8 pty(s)` lines for one update) and every client lost its host
/// twice. One daemon, however many callers: one successor.
#[tokio::test]
async fn concurrent_upgrades_of_one_daemon_hand_over_once() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let old_pid = c.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "read x; echo got:$x"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    let other = HostClient::connect_dir(dir, "test").await.unwrap();

    let (a, b) = tokio::join!(
        c.call("upgrade", json!({"exe": BIN})),
        other.call("upgrade", json!({"exe": BIN})),
    );
    // The loser is told the handoff is already done, never that it failed.
    let (a, b) = (a.unwrap(), b.unwrap());
    let handed: Vec<_> = [&a, &b].iter().map(|r| r["handed_off"].as_u64()).collect();
    assert!(
        handed.contains(&Some(1)) && handed.contains(&Some(0)),
        "one call moves the terminal, the other is a no-op: {a} / {b}"
    );

    let c2 = connect_retry(dir, old_pid).await;
    let new_pid = c2.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap();
    let _kill_new = Pid(new_pid as u32);
    // Let any second (wrong) successor finish its takeover before counting.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let log = std::fs::read_to_string(paths::log(dir)).unwrap();
    assert_eq!(
        log.matches("took over").count(),
        1,
        "one update, one handoff:\n{log}"
    );
    let now = connect_retry(dir, old_pid).await;
    let info = now.call("info", json!({})).await.unwrap();
    assert_eq!(info["pid"].as_u64(), Some(new_pid), "{info}");
    let list = now.call("pty_list", json!({})).await.unwrap();
    assert!(
        list["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["pty"] == pty.as_str() && p["exited"].is_null()),
        "{list}"
    );
}

#[tokio::test]
async fn failed_upgrade_leaves_the_old_hostd_and_its_ptys_running() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "read x; echo got:$x"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    // Not a file, then a file that is not atlas (`true` exits at once, never connects).
    assert!(
        c.call("upgrade", json!({"exe": "/nonexistent/atlas"}))
            .await
            .is_err()
    );
    let err = c
        .call("upgrade", json!({"exe": "/usr/bin/true"}))
        .await
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("old host keeps running"),
        "{err}"
    );

    // Still the same daemon, and the pty still reads and writes.
    let mut ev = c.events();
    c.call("pty_attach", json!({"pty": pty})).await.unwrap();
    c.call("pty_write", json!({"pty": pty, "data": B64.encode("ok\n")}))
        .await
        .unwrap();
    let mut out = String::new();
    while !out.contains("got:ok") {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .unwrap()
            .unwrap();
        if e["ev"] == "pty_output" {
            out.push_str(&b64(&e["data"]));
        }
    }
}

/// The successor connects, takes the bundle, and dies before confirming: the old daemon
/// must take its ptys back (reader resumed) and keep serving.
#[tokio::test]
async fn successor_dying_before_confirming_gives_the_ptys_back() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "echo before; read x; echo after:$x"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    // Fake successor: connect to `--takeover <sock>`, read a little, exit without `R`.
    let fake = dir.join("fake-atlas");
    std::fs::write(
        &fake,
        "#!/bin/sh\nsock=\"$4\"\n(sleep 1) | nc -U \"$sock\" >/dev/null\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let err = c.call("upgrade", json!({"exe": fake})).await.unwrap_err();
    assert!(
        format!("{err:#}").contains("old host keeps running"),
        "{err:#}"
    );

    // Same daemon, pty resumed: output produced after the failure is still captured.
    let mut ev = c.events();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    c.call("pty_write", json!({"pty": pty, "data": B64.encode("z\n")}))
        .await
        .unwrap();
    let mut out = String::new();
    while !out.contains("after:z") {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .expect("pty output after a failed takeover")
            .unwrap();
        if matches!(e["ev"].as_str(), Some("pty_output" | "pty_replay")) {
            out.push_str(&b64(&e["data"]));
        }
    }
    assert!(out.contains("before"), "{out:?}");
}
