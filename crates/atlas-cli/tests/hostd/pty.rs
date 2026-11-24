//! Terminals through the daemon: replay and live attach, sizes, exec/fs/auth, a client that never reads.

use std::time::Duration;

use atlas_host::{HostClient, paths};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use crate::common::*;

// Guards the empty agent terminal on the phone: the whole replay (up to 2 MiB of
// scrollback, ~2.7 MB in base64) went out as one line, and every client drops a
// line over MAX_LINE_BYTES, so a long-running agent's history never arrived.
#[tokio::test]
async fn a_long_replay_arrives_whole_in_lines_under_the_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "head -c 1500000 /dev/zero | tr '\\0' a; echo END; sleep 30"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    wait_for("output", || {
        std::fs::read_dir(dir.join("hostd/scrollback"))
            .map(|d| {
                d.flatten()
                    .any(|e| e.metadata().map_or(0, |m| m.len()) > 1_500_000)
            })
            .unwrap_or(false)
    })
    .await;
    let mut ev = c.events();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    let (mut kinds, mut got) = (Vec::new(), String::new());
    // Until `END`, the last thing the program prints: a quiet window ended this loop early on a loaded runner,
    // and the program then waits in `sleep 30`, so nothing else ends the stream.
    while !got.contains("END") {
        let e = tokio::time::timeout(Duration::from_secs(10), ev.recv())
            .await
            .expect("replay never reached END")
            .unwrap();
        let k = e["ev"].as_str().unwrap_or_default().to_string();
        if k != "pty_replay" && k != "pty_output" {
            continue;
        }
        assert!(
            e.to_string().len() < MAX_LINE,
            "a replay line over the limit"
        );
        got.push_str(&b64(&e["data"]));
        kinds.push(k);
    }
    assert!(
        got.contains("END"),
        "replay incomplete: {} bytes",
        got.len()
    );
    assert_eq!(got.matches('a').count(), 1_500_000);
    // One reset for clients that clear the screen on pty_replay, then the rest.
    assert_eq!(kinds[0], "pty_replay");
    assert!(kinds[1..].iter().all(|k| k == "pty_output"), "{kinds:?}");
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
}

const MAX_LINE: usize = 1024 * 1024;

// The phone sizes its terminal from pty_info before attaching: the replay is drawn
// into its grid before the attach reply (which also carries the size) arrives, so
// a missing size drew a 180-column TUI into 80x24 and cut it apart.
#[tokio::test]
async fn pty_info_reports_the_current_size() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "sleep 30"], "cols": 132, "rows": 41}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    let size = |v: Value| (v["cols"].as_u64(), v["rows"].as_u64());
    let info = c.call("pty_info", json!({"pty": pty})).await.unwrap();
    assert_eq!(size(info.clone()), (Some(132), Some(41)), "{info}");
    c.call("pty_resize", json!({"pty": pty, "cols": 90, "rows": 20}))
        .await
        .unwrap();
    let info = c.call("pty_info", json!({"pty": pty})).await.unwrap();
    assert_eq!(size(info.clone()), (Some(90), Some(20)), "{info}");
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
}

#[tokio::test]
async fn pty_exec_fs_auth_and_cold_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;

    // PTY: output, replay, exit code. The program exits only after `go`, created once the client
    // is attached: a fixed `sleep` let a loaded runner end it before the attach (#198).
    let mut ev = c.events();
    let go = dir.join("go-exit");
    let script = format!(
        "echo hi; while [ ! -e '{}' ]; do sleep 0.01; done",
        go.display()
    );
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", script]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    tokio::time::sleep(Duration::from_millis(200)).await;
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    std::fs::write(&go, "").unwrap();
    let (mut replay, mut code) = (String::new(), None);
    while code.is_none() {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .unwrap()
            .unwrap();
        match e["ev"].as_str().unwrap() {
            "pty_replay" | "pty_output" => replay.push_str(&b64(&e["data"])),
            "pty_exit" => code = Some(e["code"].clone()),
            _ => {}
        }
    }
    assert!(replay.contains("hi"), "{replay:?}");
    assert_eq!(code, Some(json!(0)));
    let text = c.call("pty_read", json!({"pty": pty})).await.unwrap();
    assert_eq!(text["text"], "hi");
    let err = c
        .call("pty_write", json!({"pty": pty, "data": B64.encode("x")}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("exited"));

    // Pipe mode: stdin/stdout only (line protocols); writes reach stdin, output is replayable.
    let mut ev = c.events();
    let pipe = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "read l; echo got:$l"], "mode": "pipe"}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pipe, "replay": true}))
        .await
        .unwrap();
    c.call(
        "pty_write",
        json!({"pty": pipe, "data": B64.encode("hello\n")}),
    )
    .await
    .unwrap();
    let (mut out, mut code) = (String::new(), None);
    while code.is_none() {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .unwrap()
            .unwrap();
        if e["pty"] != pipe.as_str() {
            continue;
        }
        match e["ev"].as_str().unwrap() {
            "pty_replay" | "pty_output" => out.push_str(&b64(&e["data"])),
            "pty_exit" => code = Some(e["code"].clone()),
            _ => {}
        }
    }
    assert_eq!(out, "got:hello\n");
    assert_eq!(code, Some(json!(0)));
    c.call("pty_kill", json!({"pty": pipe})).await.unwrap();

    // exec
    let r = c
        .call("exec", json!({"cwd": dir, "argv": ["git", "--version"]}))
        .await
        .unwrap();
    assert_eq!(r["code"], 0);
    assert!(b64(&r["stdout"]).starts_with("git version"));
    let r = c
        .call(
            "exec",
            json!({"cwd": dir, "argv": ["sleep", "5"], "timeout_ms": 100}),
        )
        .await
        .unwrap();
    assert_eq!(r["timed_out"], true);

    // fs round-trip + not_found
    let f = dir.join("a.txt");
    c.call("fs_write", json!({"path": f, "data": B64.encode("hello")}))
        .await
        .unwrap();
    let r = c.call("fs_read", json!({"path": f})).await.unwrap();
    assert_eq!(b64(&r["data"]), "hello");
    // Chunked read: offset + max, `truncated` while bytes remain.
    let r = c
        .call("fs_read", json!({"path": f, "offset": 1, "max": 2}))
        .await
        .unwrap();
    assert_eq!(b64(&r["data"]), "el");
    assert_eq!(r["truncated"], true);
    let r = c
        .call("fs_read", json!({"path": f, "offset": 3, "max": 10}))
        .await
        .unwrap();
    assert_eq!(b64(&r["data"]), "lo");
    assert_eq!(r["truncated"], false);
    let e = c
        .call("fs_stat", json!({"path": dir.join("nope")}))
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), "not_found");

    // Wrong token is rejected.
    let bad = HostClient::connect(&paths::socket(dir), "bad", "test").await;
    assert!(bad.err().unwrap().to_string().contains("unauthorized"));

    // Restart: the exited session comes back cold with its scrollback.
    let _ = d.0.kill();
    let _ = d.0.wait();
    let (_d2, c2) = start(dir).await;
    let list = c2.call("pty_list", json!({})).await.unwrap();
    let cold = &list["ptys"].as_array().unwrap()[0];
    assert_eq!(cold["pty"], pty.as_str());
    assert_eq!(cold["argv"], json!([]));
    let text = c2.call("pty_read", json!({"pty": pty})).await.unwrap();
    assert_eq!(text["text"], "hi");
    c2.call("pty_kill", json!({"pty": pty})).await.unwrap();
    assert!(
        !paths::scrollback_dir(dir)
            .join(format!("{pty}.bin"))
            .exists()
    );
}

#[tokio::test]
async fn raw_attach_streams_replay_then_live_bytes_and_ends_with_the_pty() {
    use tokio::io::AsyncReadExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    // `after` waits for `go`, created once the raw attach is in place: with `sleep 0.4` a loaded
    // runner (load 66) finished the program before the attach and `exited` was already set.
    // `before` may land before or after the attach; either way it reaches the reader (replay or live).
    let go = dir.join("go-raw");
    let script = format!(
        "printf before; while [ ! -e '{}' ]; do sleep 0.01; done; printf after",
        go.display()
    );
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", script]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut raw = atlas_host::client::attach_raw(dir, &pty, true)
        .await
        .unwrap();
    assert!(raw.info["exited"].is_null());
    std::fs::write(&go, b"").unwrap();
    let mut out = Vec::new();
    // EOF arrives when the pty exits; bytes are unframed (no JSON, no base64).
    tokio::time::timeout(Duration::from_secs(5), raw.reader.read_to_end(&mut out))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out), "beforeafter");

    // Events-only attach (`output: false`): exit arrives, output does not. The program waits for
    // `go`, created after the attach: with `sleep 0.2` alone a loaded runner could finish it before
    // the attach, and its `pty_exit` went to nobody (seen at load 46). Same gate as the title tests.
    let mut ev = c.events();
    let go = dir.join("go-events-only");
    let script = format!(
        "while [ ! -e '{}' ]; do sleep 0.01; done; printf x",
        go.display()
    );
    let pty2 = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", script]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty2, "output": false}))
        .await
        .unwrap();
    std::fs::write(&go, b"").unwrap();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .unwrap()
            .unwrap();
        assert_ne!(e["ev"], "pty_output", "{e}");
        if e["ev"] == "pty_exit" {
            break;
        }
    }

    // A connection already used for events can't switch to raw.
    c.call("pty_attach", json!({"pty": pty})).await.unwrap();
    let err = c
        .call("pty_attach_raw", json!({"pty": pty}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("fresh connection"), "{err}");

    // Attaching after exit: replay, then immediate EOF.
    let mut raw = atlas_host::client::attach_raw(dir, &pty, true)
        .await
        .unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), raw.reader.read_to_end(&mut out))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out), "beforeafter");
}

// Regression: #109. A client that stopped reading (a phone on a bad network, a paused webview) made
// hostd queue every `pty_output` without limit; hostd is the process that keeps the agents alive.
// Real daemon: the stuck client is a raw socket that attaches and never reads again.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_never_reads_is_dropped_with_a_reason_and_can_reattach() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const CAP: u64 = 8 * 1024 * 1024;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    // ~100 MiB of output, then a marker, then stay alive so a reattach has a live pty. The output
    // waits for a line on stdin, sent once both clients are attached: with `replay: false` a chunk
    // written before the healthy client attached never reached it (CI: 99 934 469 of 100 000 000
    // bytes, one 64 KiB read short, PR #106).
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c",
                "read go; head -c 100000000 /dev/zero | tr '\\0' a; echo END; sleep 60"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();

    // The stuck client: hello + pty_attach, then silence (its socket buffer fills, hostd's queue grows).
    let token = paths::read_token(dir).unwrap();
    let mut stuck = tokio::net::UnixStream::connect(paths::socket(dir))
        .await
        .unwrap();
    let req = json!({"id": 1, "cmd": "pty_attach", "args": {"pty": pty, "replay": false}});
    stuck
        .write_all(
            format!(
                "{}\n{req}\n",
                atlas_host::protocol::hello_line(&token, "stuck")
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    // Its attach is answered before it goes silent, so it really holds a subscription when the
    // output starts (otherwise the cap below would be checked against nothing).
    let mut reply = Vec::new();
    while !String::from_utf8_lossy(&reply).contains("\"id\":1") {
        let mut buf = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(10), stuck.read(&mut buf))
            .await
            .expect("the stuck client's attach was never answered")
            .unwrap();
        assert!(n > 0, "hostd closed the stuck client before its attach");
        reply.extend_from_slice(&buf[..n]);
    }

    // A healthy client keeps receiving the whole time.
    let mut ev = c.events();
    c.call("pty_attach", json!({"pty": pty, "replay": false}))
        .await
        .unwrap();
    c.call("pty_write", json!({"pty": pty, "data": B64.encode("go\n")}))
        .await
        .unwrap();
    let mut healthy_bytes = 0usize;
    let mut max_queued = 0u64;
    let t0 = std::time::Instant::now();
    let mut saw_end = false;
    // Sampled, not per event: an `info` round trip after every chunk made this reader slower than
    // hostd, so the client's own event buffer lagged and dropped output (CI: 99.87 MB of 100).
    let mut sampled = std::time::Instant::now() - Duration::from_secs(1);
    while !saw_end {
        assert!(
            t0.elapsed() < Duration::from_secs(90),
            "healthy client stalled at {healthy_bytes} bytes"
        );
        match tokio::time::timeout(Duration::from_millis(100), ev.recv()).await {
            Ok(Ok(e)) if e["pty"] == pty.as_str() && e["ev"] == "pty_output" => {
                let b = B64.decode(e["data"].as_str().unwrap()).unwrap();
                healthy_bytes += b.len();
                saw_end |= String::from_utf8_lossy(&b).contains("END");
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                panic!("the test's own reader fell behind and lost {n} events")
            }
            Ok(_) | Err(_) => {}
        }
        if sampled.elapsed() >= Duration::from_millis(100) {
            sampled = std::time::Instant::now();
            let info = c.call("info", json!({})).await.unwrap();
            max_queued = max_queued.max(info["queued"]["total_bytes"].as_u64().unwrap_or(u64::MAX));
        }
    }
    assert!(
        healthy_bytes >= 100_000_000,
        "healthy client got {healthy_bytes} bytes"
    );
    // What hostd held for the stuck connection never went past the cap (plus the healthy client's
    // own small backlog and one coalesced chunk), where it used to be ~100 MiB.
    assert!(
        max_queued < CAP + 4 * 1024 * 1024,
        "hostd queued {max_queued} bytes for clients (cap {CAP})"
    );
    let info = c.call("info", json!({})).await.unwrap();
    assert_eq!(info["queued"]["cap_bytes"], CAP);
    assert!(info["queued"]["dropped"].as_u64().unwrap() >= 1, "{info}");

    // Reading the stuck socket now: its backlog ends with the reason it was dropped. Read to that line (the
    // event), not until the socket has been quiet for a while: hostd's writer feeds the backlog out as this
    // side drains it, and a loaded runner can pause it longer than any quiet window.
    let mut all = Vec::new();
    let mut buf = vec![0u8; 256 * 1024];
    while !String::from_utf8_lossy(&all).contains(r#""ev":"pty_dropped""#) {
        let n = tokio::time::timeout(Duration::from_secs(30), stuck.read(&mut buf))
            .await
            .unwrap_or_else(|_| panic!("no pty_dropped among {} bytes", all.len()))
            .unwrap();
        assert!(
            n > 0,
            "the socket closed after {} bytes without a pty_dropped",
            all.len()
        );
        all.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&all);
    let dropped = text
        .lines()
        .rev()
        .find(|l| l.contains(r#""ev":"pty_dropped""#))
        .unwrap_or_else(|| panic!("no pty_dropped among {} bytes", all.len()));
    let dropped: Value = serde_json::from_str(dropped).unwrap();
    assert_eq!(
        (&dropped["pty"], &dropped["reason"]),
        (&json!(pty), &json!("slow_client"))
    );

    // Reattach with replay: the ring comes back, ends at the program's last output, and the
    // pty is still running (the program was never held up).
    let c2 = HostClient::connect_dir(dir, "again").await.unwrap();
    let mut ev2 = c2.events();
    c2.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    let (mut replayed, mut kinds) = (String::new(), Vec::new());
    while !replayed.contains("END") {
        let e = tokio::time::timeout(Duration::from_secs(5), ev2.recv())
            .await
            .expect("replay")
            .unwrap();
        let k = e["ev"].as_str().unwrap_or_default().to_string();
        if k == "pty_replay" || k == "pty_output" {
            replayed.push_str(&b64(&e["data"]));
            kinds.push(k);
        }
    }
    assert_eq!(kinds[0], "pty_replay");
    assert!(
        replayed.len() > 1_000_000,
        "replay was {} bytes",
        replayed.len()
    );
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
}

// Incident (CI 45b9bd74, Linux): `atlas hostd --lab --stop` failed with "hostd disconnected". The daemon queued the
// shutdown reply and woke the accept loop, which ended the process before the connection's writer flushed it. A
// shutdown must always be answered before the daemon exits. Many rounds: the race is a timing window.
#[tokio::test]
async fn shutdown_is_always_answered_before_the_daemon_exits() {
    let mut lost = 0;
    for _ in 0..30 {
        let tmp = tempfile::tempdir().unwrap();
        let (_d, c) = start(tmp.path()).await;
        if c.call("shutdown", json!({})).await.is_err() {
            lost += 1;
        }
    }
    assert_eq!(lost, 0, "{lost}/30 shutdown replies lost");
}
