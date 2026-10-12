//! `atlas hostd` end-to-end: real binary, isolated `ATLAS_HOSTD_DIR`.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use atlas_host::{HostClient, paths};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn start(dir: &Path) -> (Daemon, HostClient) {
    let child = Command::new(BIN)
        .args(["hostd", "--dir"])
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let d = Daemon(child);
    for _ in 0..100 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return (d, c);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

fn b64(v: &Value) -> String {
    String::from_utf8(B64.decode(v.as_str().unwrap()).unwrap()).unwrap()
}

#[tokio::test]
async fn pty_exec_fs_auth_and_cold_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;

    // PTY: output, replay, exit code.
    let mut ev = c.events();
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "echo hi; sleep 0.5"]}),
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
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "printf before; sleep 0.4; printf after"]}),
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
    let mut out = Vec::new();
    // EOF arrives when the pty exits; bytes are unframed (no JSON, no base64).
    tokio::time::timeout(Duration::from_secs(5), raw.reader.read_to_end(&mut out))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out), "beforeafter");

    // Events-only attach (`output: false`): exit arrives, output does not.
    let mut ev = c.events();
    let pty2 = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "sleep 0.2; printf x"]}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty2, "output": false}))
        .await
        .unwrap();
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

/// Kills the successor daemon (not our child after a takeover) when the test ends.
struct Pid(u32);
impl Drop for Pid {
    fn drop(&mut self) {
        let _ = Command::new("kill").arg(self.0.to_string()).status();
    }
}

async fn connect_retry(dir: &Path, pid_not: u64) -> HostClient {
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await
            && let Ok(i) = c.call("info", json!({})).await
            && i["pid"].as_u64() != Some(pid_not)
        {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("successor hostd did not come up");
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
    tokio::time::sleep(Duration::from_millis(400)).await;
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
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        c2.call("pty_read", json!({"pty": p3})).await.unwrap()["text"],
        "fresh"
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
