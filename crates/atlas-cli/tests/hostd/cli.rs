//! The `atlas hostd` subcommands (check, reconcile, restart, ensure, pair) and recovery of a host the app cannot use.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use atlas_host::{HostClient, paths};
use serde_json::{Value, json};

use crate::common::*;

#[tokio::test]
async fn check_and_reconcile_on_a_current_daemon_are_no_ops() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let pid = c.call("info", json!({})).await.unwrap()["pid"].clone();

    let o = hostd_cmd(dir, &["--check"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stdout).contains("is current"));

    let o = hostd_cmd(dir, &["--reconcile"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stdout).contains("is current"));
    let after = c.call("info", json!({})).await.unwrap();
    assert_eq!(
        after["pid"], pid,
        "reconcile must not touch a current daemon"
    );
}

#[tokio::test]
async fn check_reports_not_running_with_exit_1_and_reconcile_with_exit_0() {
    let tmp = tempfile::tempdir().unwrap();
    let o = hostd_cmd(tmp.path(), &["--check"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("not running"));
    let o = hostd_cmd(tmp.path(), &["--reconcile"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("not running"));
}

/// A stand-in for an old daemon (proto 1, no caps, no `upgrade`): answers hello, `info`,
/// `pty_list` and `shutdown`, and removes its socket when told to stop, like the real one.
/// `ptys` running terminals are reported.
fn fake_old_hostd(dir: &Path, ptys: usize) -> tokio::task::JoinHandle<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let sock = paths::socket(dir);
    paths::ensure_token(dir).unwrap();
    std::fs::write(paths::pid(dir), "1").unwrap();
    let l = tokio::net::UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            let sock = sock.clone();
            tokio::spawn(async move {
                let (r, mut w) = s.into_split();
                let mut lines = BufReader::new(r).lines();
                let _ = lines.next_line().await; // hello
                let info = json!({"version": "0.6.2", "proto": 1, "pid": 1,
                    "uptime_s": 144000, "ptys": ptys});
                let hello = json!({"ok": true, "data": info});
                let _ = w.write_all(format!("{hello}\n").as_bytes()).await;
                while let Ok(Some(line)) = lines.next_line().await {
                    let v: Value = serde_json::from_str(&line).unwrap();
                    let data = match v["cmd"].as_str() {
                        Some("info") => info.clone(),
                        Some("pty_list") => json!({"ptys": (0..ptys).map(|i| json!({
                            "pty": format!("p{i}"), "cwd": format!("/w/{i}"),
                            "argv": ["zsh"], "exited": null})).collect::<Vec<_>>()}),
                        _ => {
                            let r = json!({"id": v["id"], "ok": true, "data": {}});
                            let _ = w.write_all(format!("{r}\n").as_bytes()).await;
                            let _ = std::fs::remove_file(&sock);
                            return;
                        }
                    };
                    let r = json!({"id": v["id"], "ok": true, "data": data});
                    let _ = w.write_all(format!("{r}\n").as_bytes()).await;
                }
            });
        }
    })
}

/// Pid of the real daemon that took the dir over; killed when the test ends.
async fn real_daemon_pid(dir: &Path) -> Pid {
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await
            && let Ok(i) = c.call("info", json!({})).await
            && i["version"] == env!("CARGO_PKG_VERSION")
        {
            return Pid(i["pid"].as_u64().unwrap() as u32);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no current daemon came up");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_reports_a_stale_daemon_with_its_terminals_and_exits_3() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let _fake = fake_old_hostd(dir, 2);
    let o = hostd_cmd(dir, &["--check"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(3), "{out}");
    assert!(
        out.contains("Terminal host 0.6.2 (pid 1, up 40h, 2 terminals) is older than Atlas"),
        "{out}"
    );
    assert!(
        out.contains(
            "language features, fast terminal rendering, and agent status are off until it restarts"
        ),
        "{out}"
    );
    assert!(
        out.contains("/w/0  zsh") && out.contains("/w/1  zsh"),
        "{out}"
    );
    assert!(
        out.contains("--reconcile") && out.contains("--restart"),
        "{out}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_restarts_a_stale_daemon_with_no_terminal_without_asking() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let _fake = fake_old_hostd(dir, 0);
    let o = hostd_cmd(dir, &["--reconcile"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{out} {}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains("restarted"), "{out}");
    let _real = real_daemon_pid(dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_never_ends_running_terminals_without_consent() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let _fake = fake_old_hostd(dir, 2);
    // No TTY here: the prompt cannot be answered, so the daemon is kept and the user told how.
    let o = hostd_cmd(dir, &["--reconcile"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(0), "{out}");
    assert!(
        out.contains("Kept as is") && out.contains("atlas hostd --restart"),
        "{out}"
    );
    let c = HostClient::connect_dir(dir, "test").await.unwrap();
    assert_eq!(c.call("info", json!({})).await.unwrap()["version"], "0.6.2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_replaces_a_stale_daemon_after_explicit_consent() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let _fake = fake_old_hostd(dir, 2);
    let o = hostd_cmd(dir, &["--restart"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{out} {}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("restarted") && out.contains("2 terminals ended"),
        "{out}"
    );
    let _real = real_daemon_pid(dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_emits_the_progress_protocol_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let _fake = fake_old_hostd(dir, 3);
    let o = Command::new(BIN)
        .args(["hostd", "--reconcile", "--dir"])
        .arg(dir)
        .env("ATLAS_UPDATE_PROGRESS", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("@@atlas phase terminal-host"), "{out}");
    assert!(out.contains("@@atlas host needs-restart 3"), "{out}");
}

// ---- recovery of a terminal host the app cannot use (incident 1.5.0-alpha.5) ----

fn pid_of(dir: &Path) -> i32 {
    std::fs::read_to_string(paths::pid(dir))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// A daemon that holds the singleton lock but that the app cannot talk to (its token file was
/// replaced, or it is wedged) used to be permanent: `ensure_in` spawned a second hostd, which
/// died with "another atlas hostd is running", forever. The app showed "hostd is not reachable"
/// and every tab "Terminal is gone", and Restart went through the same dead path.
#[tokio::test]
async fn ensure_in_replaces_a_live_daemon_it_cannot_authenticate_to() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (d, c) = start(dir).await;
    let mut d = d;
    drop(c);
    let old = pid_of(dir);
    // The token on disk no longer matches the one the daemon loaded at start.
    std::fs::write(paths::token(dir), "0123456789abcdef").unwrap();
    assert!(HostClient::connect_dir(dir, "test").await.is_err());

    // Reap the old daemon concurrently: it is this test's child, and a zombie would still look
    // alive to `retire_unusable` (a real daemon is detached and reaped by init).
    let reaper = std::thread::spawn(move || {
        let _ = d.0.wait();
    });
    let c = atlas_host::client::ensure_in(dir, Path::new(BIN), "test")
        .await
        .expect("the stuck daemon is replaced, not retried forever");
    let info = c.call("info", json!({})).await.unwrap();
    assert_ne!(info["pid"].as_u64().unwrap() as i32, old);
    assert!(!alive(old), "the old daemon is gone");
    reaper.join().unwrap();
    let _ = c.call("shutdown", json!({})).await;
}

/// Exact-PID rule (AGENTS.md): recovery signals the pid in `hostd.pid` only when that process is
/// an `atlas ... hostd`. A recycled pid in a stale file must never be signalled.
#[tokio::test]
async fn recovery_never_signals_a_pid_that_is_not_an_atlas_hostd() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut bystander = Command::new("sleep").arg("60").spawn().unwrap();
    std::fs::write(paths::pid(dir), bystander.id().to_string()).unwrap();
    // Token exists, nothing listens: connect fails, the pid file names an unrelated live process.
    paths::ensure_token(dir).unwrap();

    let c = atlas_host::client::ensure_in(dir, Path::new(BIN), "test")
        .await
        .expect("a fresh daemon starts");
    let info = c.call("info", json!({})).await.unwrap();
    assert_ne!(info["pid"].as_u64().unwrap() as u32, bystander.id());
    assert!(
        alive(bystander.id() as i32),
        "the unrelated process was killed"
    );
    let _ = bystander.kill();
    let _ = bystander.wait();
    let _ = c.call("shutdown", json!({})).await;
}

/// SIGKILLed daemon: stale socket and pid file, nothing alive. The next connect starts a new one.
#[tokio::test]
async fn ensure_in_starts_over_a_stale_socket_and_pid_after_a_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;
    drop(c);
    let old = pid_of(dir);
    // SAFETY: `old` is the daemon this test just started.
    unsafe { libc::kill(old, libc::SIGKILL) };
    let _ = d.0.wait(); // reap it: a zombie still answers `kill(pid, 0)`
    assert!(!alive(old));
    assert!(paths::socket(dir).exists(), "a crash leaves the socket");

    let c = atlas_host::client::ensure_in(dir, Path::new(BIN), "test")
        .await
        .unwrap();
    assert_ne!(
        c.call("info", json!({})).await.unwrap()["pid"]
            .as_u64()
            .unwrap() as i32,
        old
    );
    let _ = c.call("shutdown", json!({})).await;
}

/// The app reports what it did about a host it could not use (incident + diagnostics), so it asks
/// `unusable_hostd_pid` first: `Some(pid)` only for a live atlas hostd that refuses this client.
#[tokio::test]
async fn unusable_hostd_pid_names_only_a_live_daemon_that_refuses_us() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert_eq!(
        atlas_host::client::unusable_hostd_pid(dir).await,
        None,
        "nothing runs"
    );
    let (_d, c) = start(dir).await;
    drop(c);
    assert_eq!(
        atlas_host::client::unusable_hostd_pid(dir).await,
        None,
        "a daemon that answers is usable"
    );
    let pid = pid_of(dir) as u32;
    std::fs::write(paths::token(dir), "0123456789abcdef").unwrap();
    assert_eq!(atlas_host::client::unusable_hostd_pid(dir).await, Some(pid));
}

// #356: `atlas crew pair` runs `atlas hostd --mobile-pair` on the crew server. It prints ONE JSON object, the
// daemon's own `mobile_pair_start` reply (the one pairing protocol: a code, the addresses a phone can dial), and
// the code it prints is the one the daemon accepts: asking again replaces it, so only the last is live. Relays
// off here (`ATLAS_MOBILE_RELAY=0`): the wait for a relay address ends and the direct addresses go out.
#[tokio::test]
async fn hostd_mobile_pair_prints_the_daemons_live_invite() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let child = Command::new(BIN)
        .args(["hostd", "--dir"])
        .arg(dir)
        .env("ATLAS_MOBILE_RELAY", "0")
        .env("ATLAS_MOBILE_LOOPBACK", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _d = Daemon(child);
    for _ in 0..100 {
        if HostClient::connect_dir(dir, "test").await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let out = tokio::process::Command::new(BIN)
        .args(["hostd", "--mobile-pair", "--dir"])
        .arg(dir)
        .env("ATLAS_MOBILE_RELAY", "0")
        .output()
        .await
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let invite: Value = serde_json::from_slice(&out.stdout).expect("stdout is one JSON object");
    let code = invite["code"].as_str().unwrap();
    assert_eq!(code.len(), 6, "{invite}");
    assert!(
        !invite["addrs"].as_array().unwrap().is_empty(),
        "an invite a phone cannot dial is no invite: {invite}"
    );
    assert!(invite["node_id"].as_str().is_some_and(|n| !n.is_empty()));
    // The link the CLI builds from it carries exactly that code and node.
    let url = atlas_host::mobile::pair_url(&invite).unwrap();
    assert!(url.contains(&format!("code={code}")), "{url}");
    assert!(url.contains(&format!("node={}", invite["node_id"].as_str().unwrap())));
    // The daemon is the one that issued it: a stopped daemon cannot, and says so.
    let c = HostClient::connect_dir(dir, "test").await.unwrap();
    c.call("shutdown", json!({})).await.ok();
    for _ in 0..100 {
        if HostClient::connect_dir(dir, "test").await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let down = tokio::process::Command::new(BIN)
        .args(["hostd", "--mobile-pair", "--dir"])
        .arg(dir)
        .env("ATLAS_LANG", "en")
        .output()
        .await
        .unwrap();
    assert!(!down.status.success());
    assert!(
        String::from_utf8_lossy(&down.stderr).contains("not running"),
        "{}",
        String::from_utf8_lossy(&down.stderr)
    );
}
