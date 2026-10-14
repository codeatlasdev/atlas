//! `atlas hostd` end-to-end on Windows: the real binary, an isolated `ATLAS_HOSTD_DIR`, its named
//! pipe and ConPTY terminals running `cmd.exe`. The Unix twin (`hostd.rs`) drives `sh`.
#![cfg(windows)]

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
    String::from_utf8_lossy(&B64.decode(v.as_str().unwrap()).unwrap()).into_owned()
}

fn cmd(script: &str) -> Value {
    // `/v:on`: `!VAR!` expands when the command runs (`%VAR%` expands when the line is read).
    json!(["cmd.exe", "/d", "/v:on", "/c", script])
}

/// Collect output and the exit code of `pty` from `ev` until it exits.
async fn until_exit(
    ev: &mut tokio::sync::broadcast::Receiver<atlas_host::client::HostEvent>,
    pty: &str,
) -> (String, Value) {
    let mut out = String::new();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(10), ev.recv())
            .await
            .expect("no pty_exit within 10 s")
            .unwrap();
        if e["pty"] != pty {
            continue;
        }
        match e["ev"].as_str().unwrap() {
            "pty_replay" | "pty_output" => out.push_str(&b64(&e["data"])),
            "pty_exit" => return (out, e["code"].clone()),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ptys_exec_fs_auth_and_cold_restore_over_the_named_pipe() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (mut d, c) = start(dir).await;
    let caps = c.call("info", json!({})).await.unwrap()["caps"].clone();
    assert_eq!(
        caps,
        json!(["pty_attach_raw", "lsp"]),
        "no live upgrade on Windows"
    );

    // ConPTY: output, exit code, input refused once exited.
    let mut ev = c.events();
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": cmd("echo hi&& exit 4")}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    let (out, code) = until_exit(&mut ev, &pty).await;
    assert!(out.contains("hi"), "{out:?}");
    assert_eq!(code, json!(4));
    let text = c.call("pty_read", json!({"pty": pty})).await.unwrap();
    assert!(text["text"].as_str().unwrap().contains("hi"), "{text}");
    let err = c
        .call("pty_write", json!({"pty": pty, "data": B64.encode("x")}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("exited"));

    // Typed input reaches the program; `KILL` ends a running one.
    let mut ev = c.events();
    let live = c
        .call("pty_create", json!({"cwd": dir, "argv": cmd("set /p L=&& echo got:!L!&& ping -n 60 127.0.0.1 >NUL")}))
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": live, "replay": true}))
        .await
        .unwrap();
    c.call(
        "pty_write",
        json!({"pty": live, "data": B64.encode("hello\r")}),
    )
    .await
    .unwrap();
    let mut seen = String::new();
    while !seen.contains("got:hello") {
        let e = tokio::time::timeout(Duration::from_secs(10), ev.recv())
            .await
            .expect("typed line never echoed")
            .unwrap();
        if e["pty"] == live.as_str() && e["ev"] == "pty_output" {
            seen.push_str(&b64(&e["data"]));
        }
    }
    c.call("pty_signal", json!({"pty": live, "signal": "KILL"}))
        .await
        .unwrap();
    until_exit(&mut ev, &live).await;
    c.call("pty_kill", json!({"pty": live})).await.unwrap();

    // Pipe mode: stdin/stdout only.
    let mut ev = c.events();
    let pipe = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": cmd("set /p L=&& echo got:!L!"), "mode": "pipe"}),
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
        json!({"pty": pipe, "data": B64.encode("hello\r\n")}),
    )
    .await
    .unwrap();
    let (out, code) = until_exit(&mut ev, &pipe).await;
    assert_eq!(out.trim_end(), "got:hello");
    assert_eq!(code, json!(0));
    c.call("pty_kill", json!({"pty": pipe})).await.unwrap();

    // exec, with a timeout that ends the program.
    let r = c
        .call(
            "exec",
            json!({"cwd": dir, "argv": ["cmd.exe", "/d", "/c", "echo exec-ok"]}),
        )
        .await
        .unwrap();
    assert_eq!(r["code"], 0);
    assert_eq!(b64(&r["stdout"]).trim_end(), "exec-ok");
    let r = c
        .call(
            "exec",
            json!({"cwd": dir, "argv": ["ping", "-n", "30", "127.0.0.1"], "timeout_ms": 200}),
        )
        .await
        .unwrap();
    assert_eq!(r["timed_out"], true);

    // fs round-trip (atomic replace of an existing file) + not_found.
    let f = dir.join("a.txt");
    for body in ["hello", "replaced"] {
        c.call("fs_write", json!({"path": f, "data": B64.encode(body)}))
            .await
            .unwrap();
        let r = c.call("fs_read", json!({"path": f})).await.unwrap();
        assert_eq!(b64(&r["data"]), body);
    }
    let e = c
        .call("fs_stat", json!({"path": dir.join("nope")}))
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), "not_found");

    // Wrong token is rejected.
    let bad = HostClient::connect(&paths::socket(dir), "bad", "test").await;
    assert!(bad.err().unwrap().to_string().contains("unauthorized"));

    // A second daemon on the same dir refuses to start (pid lock), and does not steal the pipe.
    let second = Command::new(BIN)
        .args(["hostd", "--dir"])
        .arg(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert!(HostClient::connect_dir(dir, "test").await.is_ok());

    // Restart: the exited session comes back cold with its scrollback.
    drop(c);
    let _ = d.0.kill();
    let _ = d.0.wait();
    let (_d2, c2) = start(dir).await;
    let list = c2.call("pty_list", json!({})).await.unwrap();
    let cold = list["ptys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pty"] == pty.as_str())
        .expect("cold session restored")
        .clone();
    assert_eq!(cold["argv"], json!([]));
    let text = c2.call("pty_read", json!({"pty": pty})).await.unwrap();
    assert!(text["text"].as_str().unwrap().contains("hi"));
    c2.call("pty_kill", json!({"pty": pty})).await.unwrap();
    assert!(
        !paths::scrollback_dir(dir)
            .join(format!("{pty}.bin"))
            .exists()
    );
}

/// The default terminal is a real shell (PowerShell or cmd) that answers typed commands.
#[tokio::test(flavor = "multi_thread")]
async fn the_default_terminal_is_an_interactive_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let mut ev = c.events();
    let pty = c
        .call("pty_create", json!({"cwd": dir, "cols": 100, "rows": 30}))
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    c.call("pty_resize", json!({"pty": pty, "cols": 120, "rows": 40}))
        .await
        .unwrap();
    c.call(
        "pty_write",
        json!({"pty": pty, "data": B64.encode("echo atlas-shell-ok\r")}),
    )
    .await
    .unwrap();
    let mut seen = String::new();
    // The command line itself echoes "echo atlas-shell-ok"; its output is the bare word.
    while seen.matches("atlas-shell-ok").count() < 2 {
        let e = tokio::time::timeout(Duration::from_secs(15), ev.recv())
            .await
            .expect("the shell never ran the command")
            .unwrap();
        if e["pty"] == pty.as_str() && matches!(e["ev"].as_str(), Some("pty_output" | "pty_replay"))
        {
            seen.push_str(&b64(&e["data"]));
        }
    }
    c.call(
        "pty_write",
        json!({"pty": pty, "data": B64.encode("exit\r")}),
    )
    .await
    .unwrap();
    until_exit(&mut ev, &pty).await;
}

/// `ensure_in` starts a detached daemon the client can reach, and a second call reuses it.
#[tokio::test(flavor = "multi_thread")]
async fn ensure_in_starts_a_detached_daemon_and_reuses_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let c = atlas_host::client::ensure_in(dir, Path::new(BIN), "test")
        .await
        .unwrap();
    let pid = c.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap() as u32;
    assert_eq!(atlas_host::client::live_hostd_pid(dir), Some(pid));
    let again = atlas_host::client::ensure_in(dir, Path::new(BIN), "test")
        .await
        .unwrap();
    let pid2 = again.call("info", json!({})).await.unwrap()["pid"]
        .as_u64()
        .unwrap() as u32;
    assert_eq!(pid, pid2, "reused, not restarted");
    let _ = c.call("shutdown", json!({})).await;
    for _ in 0..100 {
        if !atlas_core::sys::pid_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd {pid} did not stop on shutdown");
}

/// Shell scripts from atlas.yaml (`worktree.setup`, `archive`) keep their own quoting: `exec`
/// and terminals pass a shell-form argv to cmd verbatim, and a bare program name finds its
/// `.cmd` shim the way npm installs tools (`claude`, `codex`).
#[tokio::test(flavor = "multi_thread")]
async fn shell_scripts_keep_their_quoting_and_cmd_shims_are_found() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (_d, c) = start(dir).await;
    let script = r#"echo "a  b"&& echo 100%% done"#;
    let argv = atlas_core::sys::shell_argv(script);
    let r = c
        .call("exec", json!({"cwd": dir, "argv": argv}))
        .await
        .unwrap();
    assert_eq!(r["code"], 0);
    let out = b64(&r["stdout"]);
    assert!(out.contains("\"a  b\""), "{out:?}");

    // A terminal running the same shell form.
    let mut ev = c.events();
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": atlas_core::sys::shell_argv(r#"echo "x y"&& exit 5"#)}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    let (out, code) = until_exit(&mut ev, &pty).await;
    assert!(out.contains("\"x y\""), "{out:?}");
    assert_eq!(code, json!(5));
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
    assert!(
        !paths::scrollback_dir(dir)
            .join(format!("{pty}.cmd"))
            .exists()
    );

    // `tool` resolves to tool.cmd, never the extensionless POSIX script npm puts beside it.
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("tool"), "#!/bin/sh\necho posix\n").unwrap();
    std::fs::write(bin.join("tool.cmd"), "@echo shim:%1\r\n").unwrap();
    let path = format!("{};{}", bin.display(), std::env::var("PATH").unwrap());
    let r = c
        .call(
            "exec",
            json!({"cwd": dir, "argv": ["tool", "ok"], "env": {"PATH": path}}),
        )
        .await
        .unwrap();
    assert_eq!(b64(&r["stdout"]).trim_end(), "shim:ok");
    let mut ev = c.events();
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["tool", "tty"], "env": {"PATH": path}}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pty_attach", json!({"pty": pty, "replay": true}))
        .await
        .unwrap();
    let (out, _) = until_exit(&mut ev, &pty).await;
    assert!(out.contains("shim:tty"), "{out:?}");
}
