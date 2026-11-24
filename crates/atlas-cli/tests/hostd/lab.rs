//! `hostd --lab`: a daemon beside the main one that follows it and never touches it.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use atlas_host::paths;
use serde_json::json;

use crate::common::*;

/// `atlas hostd --lab` (no `--dir`) with `ATLAS_HOSTD_DIR=<main>`: the shadow daemon, started like the
/// maintainer does it. Its files go to `<main>/lab`; main's own socket, pid and token are untouched.
fn spawn_lab(main: &Path) -> Daemon {
    Daemon(
        Command::new(BIN)
            .args(["hostd", "--lab"])
            .env("ATLAS_HOSTD_DIR", main)
            .env("ATLAS_MOBILE_RELAY", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn lab_hostd(main: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(["hostd", "--lab"])
        .args(args)
        .env("ATLAS_HOSTD_DIR", main)
        .output()
        .unwrap()
}

// Guards the lab lane's promise: the shadow daemon lives under `<main>/lab`, shows the real agents of
// the main hostd, and `--stop` stops ONLY it (the maintainer's hostd, with his terminals, keeps running).
#[tokio::test]
async fn lab_runs_beside_main_follows_it_and_stop_leaves_main_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path();
    let (_m, c) = start(main).await;
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": main, "argv": ["sh", "-c", "sleep 60"], "scope": {"pane_key": "p1"}}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    let main_pid = std::fs::read_to_string(paths::pid(main)).unwrap();
    let main_token = std::fs::read_to_string(paths::token(main)).unwrap();

    let _lab = spawn_lab(main);
    let lab_dir = main.join("lab");
    let lab = connect_when_up(&lab_dir).await;
    let info = lab.call("info", json!({})).await.unwrap();
    assert_ne!(info["pid"], json!(std::process::id()));
    // The lab lists main's terminal, which it does not own.
    let list = lab.call("pty_list", json!({})).await.unwrap();
    assert!(list.to_string().contains(&pty), "{list}");

    // Its own identity and files, never main's.
    assert!(lab_dir.join("hostd.sock").exists());
    assert_ne!(
        std::fs::read_to_string(paths::token(&lab_dir)).unwrap(),
        main_token
    );
    assert_eq!(std::fs::read_to_string(paths::pid(main)).unwrap(), main_pid);

    // `--status` / `--stop` with `--lab` address the lab, not main.
    let st = lab_hostd(main, &["--status"]);
    assert!(st.status.success(), "{st:?}");
    assert!(String::from_utf8_lossy(&st.stdout).contains("\"pid\""));
    let stop = lab_hostd(main, &["--stop"]);
    assert!(stop.status.success(), "{stop:?}");
    wait_for("the lab to stop", || !lab_dir.join("hostd.sock").exists()).await;

    // Main and its terminal are still there.
    let l = c.call("pty_list", json!({})).await.unwrap();
    assert!(l["ptys"][0]["exited"].is_null(), "{l}");
    assert_eq!(std::fs::read_to_string(paths::pid(main)).unwrap(), main_pid);
}

// Guards "exits cleanly if the main hostd is absent": a non-zero exit with a message that says what
// to do, and no lab directory left behind.
#[test]
fn lab_without_a_main_hostd_exits_with_a_clear_error_and_creates_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("nomain");
    std::fs::create_dir_all(&main).unwrap();
    let out = lab_hostd(&main, &[]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no main atlas hostd"), "{err}");
    assert!(!main.join("lab").exists(), "a failed lab start left files");
}

// Guards the maintainer's daily case: `atlas update` restarts his hostd. The lab must survive that,
// answer forwarded calls with a clear error meanwhile and follow the new main when it is back.
#[tokio::test]
async fn lab_survives_main_restarting_and_follows_the_new_one() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path();
    let (m, _c) = start(main).await;
    let _lab = spawn_lab(main);
    let lab = connect_when_up(&main.join("lab")).await;
    assert!(lab.call("pty_list", json!({})).await.is_ok());

    drop(m); // kills the real main process
    let err = {
        let mut e = None;
        for _ in 0..100 {
            if let Err(x) = lab.call("pty_list", json!({})).await {
                e = Some(x.to_string());
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        e.expect("forwarded calls never failed while main was down")
    };
    assert!(err.contains("main atlas hostd"), "{err}");
    lab.call("info", json!({}))
        .await
        .expect("the lab itself keeps serving");

    let (_m2, c2) = start(main).await;
    c2.call(
        "pty_create",
        json!({"cwd": main, "argv": ["sh", "-c", "sleep 60"], "scope": {"pane_key": "again"}}),
    )
    .await
    .unwrap();
    let mut n = 0;
    for _ in 0..100 {
        if let Ok(l) = lab.call("pty_list", json!({})).await {
            n = l["ptys"].as_array().map_or(0, Vec::len);
            if n == 1 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(n, 1, "the lab never reached the restarted main");
}
