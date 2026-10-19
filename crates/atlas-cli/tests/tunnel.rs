//! End-to-end tests for the SSH tunnel against a FAKE ssh (`ATLAS_SSH_BIN`).
//! The fake parses `-L local:host:port`, listens on `local` and appends each
//! invocation to a file. No real host is ever contacted.

// The fixtures are sh scripts and Unix sockets.
#![cfg(unix)]

use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

mod common;
use common::{free_port, libc_kill, wait_for};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// Fake ssh. `FAKE_SSH_FAIL=<msg>` makes it print `<msg>` to stderr and exit 255.
const FAKE_SSH: &str = r#"#!/usr/bin/env python3
import os, socket, sys, time
args = sys.argv[1:]
with open(os.environ["FAKE_SSH_LOG"], "a") as f:
    f.write(" ".join(args) + "\n")
fail = os.environ.get("FAKE_SSH_FAIL_FILE")
if fail and os.path.exists(fail):
    sys.stderr.write(open(fail).read().strip() + "\n")
    sys.stderr.flush()
    sys.exit(255)
spec = args[args.index("-L") + 1]
local = int(spec.split(":")[0])
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", local))
s.listen(8)
sys.stderr.write("fake ssh: forwarding %s\n" % spec)
sys.stderr.flush()
while True:
    c, _ = s.accept()
    c.close()
"#;

fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(200),
    )
    .is_ok()
}

struct Fixture {
    dir: tempfile::TempDir,
    local_port: u16,
    fake: PathBuf,
    ssh_log: PathBuf,
    fail_file: PathBuf,
}

impl Fixture {
    /// `tunnel_enabled`: value of `tunnel.enabled`; `action_open_tunnel`: adds an
    /// `open_tunnel: true` action `t` writing `ran.txt`.
    fn new(tunnel_enabled: bool) -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let local_port = free_port();
        let fake = dir.path().join("fake-ssh");
        std::fs::write(&fake, FAKE_SSH).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let yaml = format!(
            r#"name: tunnel-fixture
tunnel:
  enabled: {tunnel_enabled}
  local_port: {local_port}
  remote_host: db.internal
  remote_port: 5432
  ssh_host: fake-host
  keepalive_interval: 7
  keepalive_max: 2
  reconnect_cooldown: 5
services:
  ticker:
    command: "sh -c 'while true; do sleep 1; done'"
actions:
  - key: o
    label: Needs tunnel
    command: echo ran > ran.txt
    open_tunnel: true
  - key: x
    label: No tunnel
    command: echo ran > ran-notunnel.txt
"#
        );
        std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
        let ssh_log = dir.path().join("ssh-invocations.log");
        let fail_file = dir.path().join("fail-msg");
        Self {
            dir,
            local_port,
            fake,
            ssh_log,
            fail_file,
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn logs(&self) -> PathBuf {
        self.root().join(".logs")
    }

    fn invocations(&self) -> Vec<String> {
        std::fs::read_to_string(&self.ssh_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn spawn_headless(&self) -> Headless {
        let out_path = self.root().join("headless-stdout.txt");
        let err_path = self.root().join("headless-stderr.txt");
        let child = Command::new(BIN)
            .args(["dev", "--headless", "--dir", self.root().to_str().unwrap()])
            .env("ATLAS_SSH_BIN", &self.fake)
            .env("FAKE_SSH_LOG", &self.ssh_log)
            .env("FAKE_SSH_FAIL_FILE", &self.fail_file)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&out_path).unwrap())
            .stderr(std::fs::File::create(&err_path).unwrap())
            .spawn()
            .expect("spawn atlas dev --headless");
        Headless {
            child: Some(child),
            out_path,
            err_path,
        }
    }

    /// Parsed `.tui-status.json` (Null until written).
    fn status(&self) -> Value {
        std::fs::read_to_string(self.logs().join(".tui-status.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null)
    }

    fn tunnel_state(&self) -> Option<String> {
        self.status()["tunnel"]["state"]
            .as_str()
            .map(str::to_string)
    }

    fn wait_tunnel(&self, want: &str, timeout: Duration) {
        wait_for(&format!("tunnel {want}"), timeout, || {
            (self.tunnel_state().as_deref() == Some(want)).then_some(())
        });
    }
}

struct Headless {
    child: Option<Child>,
    out_path: PathBuf,
    err_path: PathBuf,
}

impl Headless {
    fn pid(&self) -> i32 {
        self.child.as_ref().unwrap().id() as i32
    }
    fn stdout(&self) -> String {
        std::fs::read_to_string(&self.out_path).unwrap_or_default()
    }
    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.err_path).unwrap_or_default()
    }
    /// SIGTERM and wait for a clean exit.
    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none() {
            unsafe { libc_kill(child.id() as i32, 15) };
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Pid of the fake ssh listening for this fixture: the single process whose
/// command line holds our `-L <port>:` spec (found by scanning ps, never by port).
fn fake_ssh_pids(f: &Fixture) -> Vec<i32> {
    let needle = format!("-L {}:db.internal:5432", f.local_port);
    let out = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains(&needle) && l.contains("fake-ssh"))
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

fn kill_all_fake(f: &Fixture) {
    for pid in fake_ssh_pids(f) {
        unsafe { libc_kill(pid, 9) };
    }
}

// ───────────────────────── tests ─────────────────────────

#[test]
fn enabled_tunnel_starts_at_boot_with_expected_ssh_args_and_stops_on_exit() {
    require_python!();
    let f = Fixture::new(true);
    let mut h = f.spawn_headless();
    f.wait_tunnel("running", Duration::from_secs(30));

    // Status file carries the tunnel object.
    let st = f.status();
    assert_eq!(st["tunnel"]["local_port"], f.local_port);
    assert_eq!(st["tunnel"]["ssh_host"], "fake-host");
    assert_eq!(st["tunnel"]["remote"], "db.internal:5432");
    assert!(port_open(f.local_port));

    // Exact ssh invocation (BatchMode, keepalive from config, -N, -L).
    let inv = f.invocations();
    assert_eq!(inv.len(), 1, "{inv:?}");
    assert_eq!(
        inv[0],
        format!(
            "-N -o ExitOnForwardFailure=yes -o ServerAliveInterval=7 -o ServerAliveCountMax=2 \
             -o BatchMode=yes -L {}:db.internal:5432 fake-host",
            f.local_port
        )
    );
    // ssh stderr is captured into .logs/tunnel.log.
    wait_for("tunnel.log", Duration::from_secs(5), || {
        std::fs::read_to_string(f.logs().join("tunnel.log"))
            .ok()
            .filter(|t| t.contains("fake ssh: forwarding"))
    });

    // Headless always kills the tunnel on exit.
    h.stop();
    wait_for("ssh gone", Duration::from_secs(10), || {
        fake_ssh_pids(&f).is_empty().then_some(())
    });
    assert!(!port_open(f.local_port));
    assert!(!f.logs().join(".tunnel.pid").exists());
}

#[test]
fn disabled_tunnel_is_not_started_and_status_is_null() {
    require_python!();
    let f = Fixture::new(false);
    let mut h = f.spawn_headless();
    wait_for("status file", Duration::from_secs(20), || {
        (f.status()["services"]["ticker"]["state"] == "running").then_some(())
    });
    assert!(f.status()["tunnel"].is_null(), "{}", f.status());
    assert!(f.invocations().is_empty());
    assert!(!port_open(f.local_port));
    h.stop();
}

#[test]
fn killed_ssh_is_reconnected() {
    require_python!();
    let f = Fixture::new(true);
    let _h = f.spawn_headless();
    f.wait_tunnel("running", Duration::from_secs(30));
    let first = fake_ssh_pids(&f);
    assert_eq!(first.len(), 1, "{first:?}");

    kill_all_fake(&f);
    wait_for("fake ssh respawned", Duration::from_secs(40), || {
        let now = fake_ssh_pids(&f);
        (now.len() == 1 && now[0] != first[0] && port_open(f.local_port)).then_some(())
    });
    f.wait_tunnel("running", Duration::from_secs(20));
    assert!(f.invocations().len() >= 2, "{:?}", f.invocations());
}

#[test]
fn action_with_open_tunnel_starts_a_disabled_tunnel_runs_then_closes_it() {
    require_python!();
    let f = Fixture::new(false);
    let _h = f.spawn_headless();
    wait_for("services up", Duration::from_secs(20), || {
        (f.status()["services"]["ticker"]["state"] == "running").then_some(())
    });
    assert!(!port_open(f.local_port));

    let resp = call_control(f.root(), "run_action", serde_json::json!({"action": "o"}));
    assert_eq!(resp["ok"], true, "{resp}");

    wait_for("action ran", Duration::from_secs(30), || {
        f.root().join("ran.txt").exists().then_some(())
    });
    // Exactly one ssh was started for the action...
    assert_eq!(f.invocations().len(), 1);
    // ...and the on-demand tunnel is closed once the action finished: ssh gone,
    // port free, no `.tunnel.pid` left to be reused, status back to null.
    wait_for("ssh gone", Duration::from_secs(15), || {
        fake_ssh_pids(&f).is_empty().then_some(())
    });
    wait_for("port closed", Duration::from_secs(10), || {
        (!port_open(f.local_port)).then_some(())
    });
    assert!(!f.logs().join(".tunnel.pid").exists());
    wait_for("status null", Duration::from_secs(10), || {
        f.tunnel_state().is_none().then_some(())
    });

    // Running it again re-opens (a fresh ssh) and closes again.
    std::fs::remove_file(f.root().join("ran.txt")).unwrap();
    let resp = call_control(f.root(), "run_action", serde_json::json!({"action": "o"}));
    assert_eq!(resp["ok"], true, "{resp}");
    wait_for("action ran again", Duration::from_secs(30), || {
        f.root().join("ran.txt").exists().then_some(())
    });
    wait_for("second ssh gone", Duration::from_secs(15), || {
        (f.invocations().len() == 2 && fake_ssh_pids(&f).is_empty()).then_some(())
    });
}

#[test]
fn boot_enabled_tunnel_survives_an_open_tunnel_action() {
    require_python!();
    let f = Fixture::new(true);
    let _h = f.spawn_headless();
    f.wait_tunnel("running", Duration::from_secs(30));
    let resp = call_control(f.root(), "run_action", serde_json::json!({"action": "o"}));
    assert_eq!(resp["ok"], true, "{resp}");
    wait_for("action ran", Duration::from_secs(30), || {
        f.root().join("ran.txt").exists().then_some(())
    });
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        port_open(f.local_port),
        "boot tunnel is not closed by an action"
    );
    f.wait_tunnel("running", Duration::from_secs(5));
    assert_eq!(f.invocations().len(), 1);
}

#[test]
fn action_without_open_tunnel_never_touches_the_tunnel() {
    require_python!();
    let f = Fixture::new(false);
    let _h = f.spawn_headless();
    wait_for("services up", Duration::from_secs(20), || {
        (f.status()["services"]["ticker"]["state"] == "running").then_some(())
    });
    let resp = call_control(f.root(), "run_action", serde_json::json!({"action": "x"}));
    assert_eq!(resp["ok"], true, "{resp}");
    wait_for("action ran", Duration::from_secs(20), || {
        f.root().join("ran-notunnel.txt").exists().then_some(())
    });
    assert!(f.invocations().is_empty());
}

#[test]
fn open_tunnel_action_fails_without_running_when_tunnel_cannot_come_up() {
    require_python!();
    let f = Fixture::new(false);
    std::fs::write(&f.fail_file, "Permission denied (publickey).").unwrap();
    let h = f.spawn_headless();
    wait_for("services up", Duration::from_secs(20), || {
        (f.status()["services"]["ticker"]["state"] == "running").then_some(())
    });
    let resp = call_control(f.root(), "run_action", serde_json::json!({"action": "o"}));
    assert_eq!(resp["ok"], true, "{resp}");

    wait_for("action failure reported", Duration::from_secs(30), || {
        h.stderr()
            .contains("action Needs tunnel: failed")
            .then_some(())
    });
    assert!(
        !f.root().join("ran.txt").exists(),
        "command must not run when the tunnel is down"
    );
    let out = h.stdout();
    assert!(
        out.contains("tunnel unavailable: Permission denied (publickey)."),
        "{out}"
    );
    assert!(
        h.stderr()
            .contains("[!] SSH tunnel to fake-host failed: Permission denied"),
        "critical alert with ssh stderr: {}",
        h.stderr()
    );
    // A failed on-demand tunnel is closed with the action too (no retry loop
    // left running in the background): status goes back to null.
    wait_for("tunnel closed", Duration::from_secs(10), || {
        f.tunnel_state().is_none().then_some(())
    });
}

#[test]
fn tunnel_left_running_is_reused_only_when_verified() {
    require_python!();
    // Run 1: enabled tunnel. We cannot flip kill_on_exit from headless, so
    // simulate "left running" by recording the fake ssh ourselves.
    let f = Fixture::new(true);
    let mut h1 = f.spawn_headless();
    f.wait_tunnel("running", Duration::from_secs(30));
    let pid = fake_ssh_pids(&f)[0];
    // Detach the ssh from run 1 so it survives atlas's exit, and record it.
    // (Setsid'd child: killing atlas does not signal it; atlas SIGKILLed → no cleanup.)
    unsafe { libc_kill(h1.pid(), 9) };
    let _ = h1.child.as_mut().unwrap().wait();
    h1.child = None;
    assert!(port_open(f.local_port), "ssh must outlive a killed atlas");
    std::fs::create_dir_all(f.logs()).unwrap();
    std::fs::write(
        f.logs().join(".tunnel.pid"),
        format!("{pid}\nfake-host\n{}\n", f.local_port),
    )
    .unwrap();
    // Stale singleton lock left by the killed run is fine: the lock is pid-checked.

    // Run 2: reuses the verified tunnel — no second ssh invocation.
    let before = f.invocations().len();
    let mut h2 = f.spawn_headless();
    wait_for(
        "run 2 to report the tunnel",
        Duration::from_secs(30),
        || h2.stdout().contains("reusing tunnel").then_some(()),
    );
    f.wait_tunnel("running", Duration::from_secs(30));
    assert_eq!(f.invocations().len(), before, "must reuse, not respawn");
    assert!(h2.stderr().is_empty() || !h2.stderr().contains("failed"));
    // Our fake-ssh name matches ATLAS_SSH_BIN's basename, so it's adopted and
    // killed on exit (headless always kills).
    let (so, se) = (h2.stdout(), h2.stderr());
    h2.stop();
    wait_for(
        &format!("adopted ssh killed\nstdout:\n{so}\nstderr:\n{se}"),
        Duration::from_secs(10),
        || fake_ssh_pids(&f).is_empty().then_some(()),
    );
}

#[test]
fn pid_record_pointing_at_a_non_ssh_process_is_ignored_and_never_killed() {
    require_python!();
    let f = Fixture::new(true);
    // A live, innocent process recorded as if it were the tunnel.
    let mut bystander = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::create_dir_all(f.logs()).unwrap();
    std::fs::write(
        f.logs().join(".tunnel.pid"),
        format!("{}\nfake-host\n{}\n", bystander.id(), f.local_port),
    )
    .unwrap();
    let mut h = f.spawn_headless();
    f.wait_tunnel("running", Duration::from_secs(30));
    // A fresh ssh was started (record rejected), and the bystander is untouched.
    assert_eq!(f.invocations().len(), 1);
    assert!(bystander.try_wait().unwrap().is_none(), "bystander killed!");
    h.stop();
    assert!(bystander.try_wait().unwrap().is_none(), "bystander killed!");
    let _ = bystander.kill();
    let _ = bystander.wait();
}

/// One request to the headless control socket (line JSON).
fn call_control(root: &Path, cmd: &str, args: Value) -> Value {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let sock = root.join(".logs/atlas.sock");
    wait_for("control socket", Duration::from_secs(20), || {
        sock.exists().then_some(())
    });
    let mut s = UnixStream::connect(&sock).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let mut line = serde_json::json!({"cmd": cmd, "args": args}).to_string();
    line.push('\n');
    s.write_all(line.as_bytes()).unwrap();
    let mut resp = String::new();
    BufReader::new(s).read_line(&mut resp).unwrap();
    serde_json::from_str(&resp).unwrap()
}
