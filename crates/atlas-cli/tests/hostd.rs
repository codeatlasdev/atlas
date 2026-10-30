//! `atlas hostd` end-to-end: real binary, isolated `ATLAS_HOSTD_DIR`.

// Unix scripts (sh -c) and Unix-only live upgrade; Windows has hostd_windows.rs.
#![cfg(unix)]

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
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(500), ev.recv()).await {
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

/// Send one hook event the way `atlas hook` does (raw HTTP to the published port), with
/// the published token unless `token` overrides it; the raw response.
fn post_hook(dir: &Path, agent: &str, body: &Value, token: Option<&str>) -> String {
    use std::io::{Read, Write};
    let env = std::fs::read_to_string(paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = body.to_string();
    let mut s =
        std::net::TcpStream::connect(("127.0.0.1", get("ATLAS_HOOK_PORT").parse::<u16>().unwrap()))
            .unwrap();
    write!(
        s,
        "POST /hook/{agent} HTTP/1.1\r\nHost: x\r\nX-Atlas-Hook-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        token.map_or_else(|| get("ATLAS_HOOK_TOKEN"), str::to_string),
        body.len()
    )
    .unwrap();
    let mut r = String::new();
    let _ = s.read_to_string(&mut r);
    r
}

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
        let mut args = json!({"cwd": dir, "argv": ["sh", "-c", "echo \"[$ATLAS_PANE_KEY|$ATLAS_HOOK_ENDPOINT]\"; sleep 1"], "env": env});
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
    c.call(
        "pty_create",
        json!({"cwd": dir, "argv": ["sh", "-c", "sleep 1"], "scope": {"pane_key": "p1"}}),
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

/// Start `atlas hostd --dir <dir>` with extra env, without waiting for it.
fn spawn_with(dir: &Path, env: &[(&str, &str)]) -> Daemon {
    let mut cmd = Command::new(BIN);
    cmd.args(["hostd", "--dir"])
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    Daemon(cmd.spawn().unwrap())
}

async fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Connect once the daemon really serves. The socket file appears at `bind`, before `listen`
/// and the token are ready, so "the file exists" raced into `connection refused` under load
/// (it failed the v1.5.0-alpha.9 release run).
async fn connect_when_up(dir: &Path) -> HostClient {
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd never accepted a client in {}", dir.display());
}

#[tokio::test]
async fn idle_hostd_with_no_terminal_and_no_client_exits_and_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut d = spawn_with(dir, &[("ATLAS_HOSTD_IDLE_SECS", "1")]);
    // Connect once, then hang up: a client resets the clock, and its absence restarts it.
    drop(connect_when_up(dir).await);
    let t0 = std::time::Instant::now();
    wait_for("idle exit", || matches!(d.0.try_wait(), Ok(Some(_)))).await;
    assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
    assert!(d.0.try_wait().unwrap().unwrap().success());
    assert!(!paths::socket(dir).exists(), "socket removed");
    assert!(!paths::pid(dir).exists(), "pid file removed");
}

#[tokio::test]
async fn hostd_with_a_connected_client_or_a_live_pty_does_not_idle_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut d = spawn_with(dir, &[("ATLAS_HOSTD_IDLE_SECS", "1")]);
    let c = connect_when_up(dir).await;
    // Connected client: well past the limit, still up.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        d.0.try_wait().unwrap().is_none(),
        "exited with a client connected"
    );

    // A running pty keeps it alive after the client leaves.
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c", "sleep 30"], "cols": 80, "rows": 24}),
        )
        .await
        .unwrap()["pty"]
        .as_str()
        .unwrap()
        .to_string();
    drop(c);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(d.0.try_wait().unwrap().is_none(), "exited with a live pty");

    // Once the pty is gone the clock runs and it exits.
    let c = HostClient::connect_dir(dir, "test").await.unwrap();
    c.call("pty_kill", json!({"pty": pty})).await.unwrap();
    drop(c);
    wait_for("idle exit after the pty ended", || {
        matches!(d.0.try_wait(), Ok(Some(_)))
    })
    .await;
}

#[tokio::test]
async fn hostd_exits_when_its_dir_disappears() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd-dir");
    // A long idle limit, so only the missing dir can end it; a pty keeps it "busy".
    let mut d = spawn_with(&dir, &[("ATLAS_HOSTD_IDLE_SECS", "2")]);
    let c = connect_when_up(&dir).await;
    c.call(
        "pty_create",
        json!({"cwd": tmp.path(), "argv": ["sh", "-c", "sleep 30"], "cols": 80, "rows": 24}),
    )
    .await
    .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    wait_for("exit after the dir was removed", || {
        matches!(d.0.try_wait(), Ok(Some(_)))
    })
    .await;
}

fn hostd_cmd(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg("hostd")
        .args(args)
        .arg("--dir")
        .arg(dir)
        .env_remove("ATLAS_UPDATE_PROGRESS")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

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
            "language features, fast terminal rendering and agent status are off until it restarts"
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
    // ~100 MiB of output, then a marker, then stay alive so a reattach has a live pty.
    let pty = c
        .call(
            "pty_create",
            json!({"cwd": dir, "argv": ["sh", "-c",
                "head -c 100000000 /dev/zero | tr '\\0' a; echo END; sleep 60"]}),
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

    // A healthy client keeps receiving the whole time.
    let mut ev = c.events();
    c.call("pty_attach", json!({"pty": pty, "replay": false}))
        .await
        .unwrap();
    let mut healthy_bytes = 0usize;
    let mut max_queued = 0u64;
    let t0 = std::time::Instant::now();
    let mut saw_end = false;
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
            Ok(_) => {}
            Err(_) => {}
        }
        let info = c.call("info", json!({})).await.unwrap();
        max_queued = max_queued.max(info["queued"]["total_bytes"].as_u64().unwrap_or(u64::MAX));
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

    // Reading the stuck socket now: its backlog ends with the reason it was dropped.
    let mut all = Vec::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        match tokio::time::timeout(Duration::from_millis(500), stuck.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => all.extend_from_slice(&buf[..n]),
            _ => break,
        }
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
