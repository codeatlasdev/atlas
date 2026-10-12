//! Runner lifecycle against the real binary: `atlas dev --detach` → `--status` →
//! attach handshake (subscribe) → control over the socket → client hang-up →
//! `atlas down`. Services are plain `sh` loops, no network. A guard stops the runner
//! (and so every service) on drop, even when an assertion fails.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

mod common;
use common::{libc_kill, wait_for};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

fn alive(pid: i64) -> bool {
    unsafe { libc_kill(pid as i32, 0) == 0 }
}

/// A project dir with two looping services and one action. Each service's command
/// line carries `<token>-api` / `<token>-web`, unique per project, so process lookups
/// never see another test's services (tests run in parallel).
struct Project {
    dir: tempfile::TempDir,
    token: String,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let token = format!(
            "rt{}",
            dir.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(['.', '_'], "")
        );
        std::fs::write(
            dir.path().join("atlas.yaml"),
            format!(
                r#"name: runner-test
services:
  api:
    command: "sh -c 'while true; do echo api-tick; sleep 1; done # {token}-api'"
  web:
    command: "sh -c 'while true; do echo web-tick; sleep 1; done # {token}-web'"
actions:
  - {{ key: x, label: Hello, command: "echo hello-from-action" }}
"#
            ),
        )
        .unwrap();
        Self { dir, token }
    }

    /// Live `sh` loops of service `name` (`api` / `web`) belonging to this project.
    fn pids(&self, name: &str) -> Vec<i64> {
        pids_matching(&format!("{}-{name}", self.token))
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn atlas(&self, args: &[&str]) -> std::process::Output {
        Command::new(BIN)
            .args(args)
            .args(["--dir", self.root().to_str().unwrap()])
            .stdin(Stdio::null())
            .output()
            .expect("run atlas")
    }

    fn atlas_ok(&self, args: &[&str]) -> String {
        let out = self.atlas(args);
        assert!(
            out.status.success(),
            "atlas {args:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn sock(&self) -> PathBuf {
        self.root().join(".logs/atlas.sock")
    }

    /// Runner pid from the status file (None when not running).
    fn runner_pid(&self) -> Option<i64> {
        let text = std::fs::read_to_string(self.root().join(".logs/.tui-status.json")).ok()?;
        serde_json::from_str::<Value>(&text).ok()?["pid"].as_i64()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // Best effort: `down` is graceful; fall back to SIGKILL of the runner.
        let _ = Command::new(BIN)
            .args(["down", "--dir", self.root().to_str().unwrap()])
            .output();
        if let Some(pid) = self.runner_pid()
            && alive(pid)
        {
            unsafe { libc_kill(pid as i32, 9) };
        }
    }
}

/// One-shot request/response over the control socket.
fn call(sock: &Path, cmd: &str, args: Value) -> Value {
    let mut s = UnixStream::connect(sock).expect("connect control socket");
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    writeln!(s, "{}", json!({"cmd": cmd, "args": args})).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

/// A subscriber: snapshot first, then a line per event.
struct Subscriber {
    reader: BufReader<UnixStream>,
    snapshot: Value,
}

impl Subscriber {
    fn connect(sock: &Path) -> Self {
        let mut s = UnixStream::connect(sock).expect("connect control socket");
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        writeln!(s, r#"{{"cmd":"subscribe","args":{{}}}}"#).unwrap();
        let mut reader = BufReader::new(s);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let resp: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["ok"], true, "{resp}");
        Self {
            reader,
            snapshot: resp["data"].clone(),
        }
    }

    /// Next event, or None on EOF.
    fn next(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
        }
    }

    /// Read events until `pred` matches one (bounded by the socket read timeout).
    fn until(&mut self, what: &str, mut pred: impl FnMut(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let ev = self
                .next()
                .unwrap_or_else(|| panic!("stream closed waiting for {what}"));
            if pred(&ev) {
                return ev;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
        }
    }
}

fn service_state(snapshot: &Value, name: &str) -> String {
    snapshot["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no service {name} in {snapshot}"))["state"]
        .as_str()
        .unwrap()
        .to_string()
}

fn pids_matching(pattern: &str) -> Vec<i64> {
    let out = Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

#[test]
fn detach_status_attach_control_detach_down() {
    let p = Project::new();

    // 1. --detach: daemonized, prints the contract line, exits 0 with the socket up.
    let out = p.atlas_ok(&["dev", "--detach"]);
    assert!(out.contains("atlas runner started (pid "), "{out}");
    assert!(
        out.contains("attach: atlas dev") && out.contains("stop: atlas down"),
        "{out}"
    );
    let pid = p.runner_pid().expect("runner wrote its status file");
    assert!(alive(pid), "runner outlives the command that started it");
    assert!(p.sock().exists());
    assert!(
        p.root().join(".logs/runner.log").exists(),
        "stdout/stderr go to runner.log"
    );

    // 2. --status: mode runner, no clients yet; services reach running.
    wait_for("both services running", Duration::from_secs(30), || {
        let s = p.atlas_ok(&["dev", "--status"]);
        (s.matches("running").count() >= 3).then_some(s) // header line + 2 rows
    });
    let status = p.atlas_ok(&["dev", "--status"]);
    assert!(status.contains("mode: runner · 0 attached"), "{status}");

    // 3. Attach handshake: snapshot carries config summary, rows and the log tail.
    let api_pid_before = wait_for("api tick process", Duration::from_secs(10), || {
        p.pids("api").first().copied()
    });
    let mut a = Subscriber::connect(&p.sock());
    assert_eq!(a.snapshot["project"], "runner-test");
    assert_eq!(a.snapshot["mode"], "runner");
    assert_eq!(a.snapshot["pid"], pid);
    assert_eq!(service_state(&a.snapshot, "api"), "running");
    assert_eq!(service_state(&a.snapshot, "web"), "running");
    assert!(
        a.snapshot["logs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["name"] == "api" && l["line"] == "api-tick"),
        "recent log tail is in the snapshot: {}",
        a.snapshot["logs"]
    );
    let status = p.atlas_ok(&["dev", "--status"]);
    assert!(status.contains("mode: runner · 1 attached"), "{status}");

    // 4. A second client sees the same stream; control requests use their own connection.
    let mut b = Subscriber::connect(&p.sock());
    let r = call(&p.sock(), "restart_service", json!({"service": "api"}));
    assert_eq!(r["ok"], true, "{r}");
    for sub in [&mut a, &mut b] {
        sub.until("api to be reported stopped/starting", |ev| {
            ev["t"] == "state" && ev["name"] == "api" && ev["state"] != "running"
        });
        sub.until("api running again", |ev| {
            ev["t"] == "state" && ev["name"] == "api" && ev["state"] == "running"
        });
    }
    let api_pid_after = wait_for("restarted api process", Duration::from_secs(10), || {
        p.pids("api").into_iter().find(|p| *p != api_pid_before)
    });
    assert_ne!(
        api_pid_before, api_pid_after,
        "the restart really replaced the process"
    );

    // An action runs on the runner and its output reaches the subscribers.
    let r = call(
        &p.sock(),
        "run_action",
        json!({"action": "x", "confirmed": true}),
    );
    assert_eq!(r["ok"], true, "{r}");
    a.until("action output", |ev| {
        ev["t"] == "log" && ev["line"] == "hello-from-action"
    });
    a.until("action completion", |ev| {
        ev["t"] == "action_done" && ev["ok"] == true
    });

    // 5. A client hanging up changes nothing for the runner or the other client.
    drop(b);
    let web_pid = p.pids("web")[0];
    wait_for(
        "attached count to drop to 1",
        Duration::from_secs(10),
        || {
            p.atlas_ok(&["dev", "--status"])
                .contains("mode: runner · 1 attached")
                .then_some(())
        },
    );
    assert!(
        alive(pid) && alive(web_pid),
        "services survive a client leaving"
    );
    drop(a);
    wait_for(
        "attached count to drop to 0",
        Duration::from_secs(10),
        || {
            p.atlas_ok(&["dev", "--status"])
                .contains("mode: runner · 0 attached")
                .then_some(())
        },
    );
    assert!(alive(web_pid), "services survive every client leaving");

    // A fresh client can reattach to the same, still-running state.
    let c = Subscriber::connect(&p.sock());
    assert_eq!(service_state(&c.snapshot, "web"), "running");
    drop(c);

    // 6. A second --detach is a no-op, not an error and not a second runner.
    let again = p.atlas_ok(&["dev", "--detach"]);
    assert!(
        again.contains("already running") && again.contains(&pid.to_string()),
        "{again}"
    );
    assert_eq!(p.runner_pid(), Some(pid));

    // 7. down: graceful, services gone, lock released, socket removed.
    let out = p.atlas_ok(&["down"]);
    assert!(out.contains("atlas stopped"), "{out}");
    assert!(!alive(pid), "runner exited");
    wait_for("services to be gone", Duration::from_secs(10), || {
        (p.pids("api").is_empty() && p.pids("web").is_empty()).then_some(())
    });
    assert!(!p.sock().exists(), "socket removed on exit");
    let out = p.atlas_ok(&["down"]);
    assert!(
        out.contains("not running"),
        "down with nothing running is exit 0: {out}"
    );
    let st = p.atlas(&["dev", "--status"]);
    assert_eq!(st.status.code(), Some(3));
}

#[test]
fn detach_reports_a_bad_config_on_the_terminal_without_spawning() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: t\nservices:\n  a:\n    commmand: x\n",
    )
    .unwrap();
    let out = Command::new(BIN)
        .args(["dev", "--detach", "--dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("config error"));
    assert!(
        !dir.path().join(".logs/atlas.sock").exists(),
        "nothing was started"
    );
}

#[test]
fn detach_with_a_selection_starts_only_that_subset() {
    let p = Project::new();
    p.atlas_ok(&["dev", "--detach", "web"]);
    wait_for("web running", Duration::from_secs(30), || {
        let s = p.atlas_ok(&["dev", "--status"]);
        s.lines()
            .any(|l| l.starts_with("web") && l.contains("running"))
            .then_some(())
    });
    let s = p.atlas_ok(&["dev", "--status"]);
    let api = s.lines().find(|l| l.starts_with("api")).unwrap();
    assert!(api.contains("off"), "api is not part of this run: {s}");
    assert!(p.pids("api").is_empty());
    p.atlas_ok(&["down"]);
}

#[test]
fn down_without_a_runner_says_so_and_exits_zero() {
    let p = Project::new();
    let out = p.atlas_ok(&["down"]);
    assert!(out.contains("not running"), "{out}");
}

#[test]
fn sessions_are_refused_by_the_runner_with_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        r#"name: sess
services:
  api:
    command: "sh -c 'while true; do sleep 1; done'"
actions:
  - { key: x, label: Shell, command: "sh", session: true }
"#,
    )
    .unwrap();
    let p = Project {
        dir,
        token: String::new(),
    };
    p.atlas_ok(&["dev", "--detach"]);
    let r = call(
        &p.sock(),
        "run_action",
        json!({"action": "x", "confirmed": true}),
    );
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("owner-only"), "{r}");
    p.atlas_ok(&["down"]);
}

#[test]
fn runner_survives_its_starting_shell_and_down_is_graceful() {
    // The command that started the runner exits right away; the runner is not its child
    // any more (own session), so it must still be there afterwards.
    let p = Project::new();
    p.atlas_ok(&["dev", "--detach"]);
    let pid = p.runner_pid().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(alive(pid));
    // setsid: the runner has no controlling terminal (`ps` shows `??` on macOS, `?` on Linux).
    let tty = Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let tty = String::from_utf8_lossy(&tty.stdout).trim().to_string();
    assert!(
        tty == "??" || tty == "?",
        "runner has no controlling tty, got {tty:?}"
    );
    p.atlas_ok(&["down"]);
}

#[test]
fn foreground_headless_is_attachable_and_down_stops_it() {
    // `--headless` is the same engine as the runner: same socket, same stream,
    // so MCP and `atlas dev` attach keep working against it.
    let p = Project::new();
    let mut child = Command::new(BIN)
        .args(["dev", "--headless", "--dir", p.root().to_str().unwrap()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("control socket", Duration::from_secs(20), || {
        p.sock().exists().then_some(())
    });
    let info = wait_for("info", Duration::from_secs(10), || {
        let r = std::panic::catch_unwind(|| call(&p.sock(), "info", json!({})));
        r.ok().filter(|v| v["ok"] == true)
    });
    assert_eq!(info["data"]["mode"], "runner");
    let sub = Subscriber::connect(&p.sock());
    assert_eq!(sub.snapshot["project"], "runner-test");
    drop(sub);

    // The headless process is *our* child: until it is reaped it stays a zombie, which
    // `kill(pid, 0)` still sees as alive on Linux. Reap it while `down` waits.
    let reaper = std::thread::spawn(move || child.wait().unwrap());
    let out = p.atlas_ok(&["down"]);
    assert!(out.contains("atlas stopped"), "{out}");
    reaper.join().unwrap();
}
