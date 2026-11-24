//! What every `hostd` suite shares: the real `atlas` binary, a daemon that is killed when it drops, and the waits.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use atlas_host::{HostClient, paths};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

pub const BIN: &str = env!("CARGO_BIN_EXE_atlas");

pub struct Daemon(pub Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub async fn start(dir: &Path) -> (Daemon, HostClient) {
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

pub fn b64(v: &Value) -> String {
    String::from_utf8(B64.decode(v.as_str().unwrap()).unwrap()).unwrap()
}

/// Kills the successor daemon (not our child after a takeover) when the test ends.
pub struct Pid(pub u32);
impl Drop for Pid {
    fn drop(&mut self) {
        let _ = Command::new("kill").arg(self.0.to_string()).status();
    }
}

pub async fn connect_retry(dir: &Path, pid_not: u64) -> HostClient {
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

/// Start `atlas hostd --dir <dir>` with extra env, without waiting for it.
pub fn spawn_with(dir: &Path, env: &[(&str, &str)]) -> Daemon {
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

/// Wait until the terminal has exited, as its own info says.
pub async fn wait_exited(c: &HostClient, pty: &str) {
    for _ in 0..200 {
        if !c.call("pty_info", json!({"pty": pty})).await.unwrap()["exited"].is_null() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{pty} never exited");
}

pub async fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
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
pub async fn connect_when_up(dir: &Path) -> HostClient {
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd never accepted a client in {}", dir.display());
}

pub fn hostd_cmd(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg("hostd")
        .args(args)
        .arg("--dir")
        .arg(dir)
        .env_remove("ATLAS_UPDATE_PROGRESS")
        .env("ATLAS_LANG", "en")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// Send one hook event the way `atlas hook` does (raw HTTP to the published port), with
/// the published token unless `token` overrides it; the raw response.
pub fn post_hook(dir: &Path, agent: &str, body: &Value, token: Option<&str>) -> String {
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
