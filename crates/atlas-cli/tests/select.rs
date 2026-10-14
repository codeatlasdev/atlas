//! `atlas dev [SERVICE...] / --profile / --no-deps` against the real binary.
//! Services are `sleep` loops, so no network or python is needed.

use std::path::Path;
use std::process::{Command, Output, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// db <- api <- web, plus a standalone worker. `SLEEP` is replaced by a long-running command of the
/// platform shell.
const PROJECT: &str = "name: sel
services:
  db:
    command: SLEEP
  api:
    command: SLEEP
    depends_on: [db]
  web:
    command: SLEEP
    depends_on: [api]
  worker:
    command: SLEEP
profiles:
  x: [api]
  y: [worker]
";

/// Five minutes in the shell services run under (`sh -c`, `cmd /c` on Windows).
const SLEEP: &str = if cfg!(windows) {
    "ping -n 300 127.0.0.1 >NUL"
} else {
    "sleep 300"
};

fn project(extra_local: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        PROJECT.replace("SLEEP", SLEEP),
    )
    .unwrap();
    if let Some(local) = extra_local {
        std::fs::write(dir.path().join("atlas.local.yaml"), local).unwrap();
    }
    dir
}

fn dev(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("dev")
        .args(args)
        .args(["--dir", dir.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// `--ci` JSON → `(name, state, off)` rows.
fn ci_rows(out: &Output) -> Vec<(String, String, bool)> {
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    v["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["state"].as_str().unwrap().to_string(),
                s["off"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

fn running(rows: &[(String, String, bool)]) -> Vec<&str> {
    let mut r: Vec<&str> = rows
        .iter()
        .filter(|(_, s, _)| s == "running")
        .map(|(n, _, _)| n.as_str())
        .collect();
    r.sort();
    r
}

fn off(rows: &[(String, String, bool)]) -> Vec<&str> {
    let mut r: Vec<&str> = rows
        .iter()
        .filter(|(_, s, o)| *o && s == "disabled")
        .map(|(n, _, _)| n.as_str())
        .collect();
    r.sort();
    r
}

#[test]
fn ci_with_service_starts_only_its_dependency_closure() {
    let dir = project(None);
    let out = dev(dir.path(), &["--ci", "--timeout", "20", "web"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = ci_rows(&out);
    assert_eq!(running(&rows), ["api", "db", "web"]);
    assert_eq!(off(&rows), ["worker"]);
}

#[test]
fn ci_profiles_union_with_no_deps() {
    let dir = project(None);
    let out = dev(
        dir.path(),
        &[
            "--ci",
            "--timeout",
            "20",
            "--profile",
            "x",
            "--profile",
            "y",
            "--no-deps",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = ci_rows(&out);
    assert_eq!(running(&rows), ["api", "worker"]);
    assert_eq!(off(&rows), ["db", "web"]);
}

#[test]
fn local_yaml_can_override_a_profile() {
    let dir = project(Some("profiles:\n  x: [worker]\n"));
    let out = dev(dir.path(), &["--ci", "--timeout", "20", "--profile", "x"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(running(&ci_rows(&out)), ["worker"]);
}

#[test]
fn unknown_names_fail_fast_with_a_hint_and_start_nothing() {
    let dir = project(None);
    let out = dev(dir.path(), &["--ci", "wev"]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unknown service 'wev' (did you mean 'web'?)"),
        "{err}"
    );

    let out = dev(dir.path(), &["--ci", "--profile", "xx"]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unknown profile 'xx' (did you mean 'x'?)"),
        "{err}"
    );

    let out = dev(dir.path(), &["--ci", "--no-deps"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        !dir.path().join(".logs/.pids").exists(),
        "nothing was started"
    );
}

#[test]
fn config_check_rejects_a_profile_with_an_unknown_service() {
    let dir = project(Some("profiles:\n  bad: [wrker]\n"));
    let out = Command::new(BIN)
        .args(["config", "check", "--dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("profiles.bad[0]") && err.contains("did you mean 'worker'?"),
        "{err}"
    );
}

/// Headless: `--status` marks deselected services `off`; a control-socket
/// start brings one up and clears the mark.
// Unix socket client and `kill`.
#[cfg(unix)]
#[test]
fn headless_marks_off_and_starts_by_hand() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            // SIGTERM → graceful stop of the services; SIGKILL if it hangs.
            let _ = Command::new("kill").arg(self.0.id().to_string()).status();
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() > deadline {
                    let _ = self.0.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = self.0.wait();
        }
    }

    let dir = project(None);
    let _h = Kill(
        Command::new(BIN)
            .args(["dev", "--headless", "--profile", "x", "--dir"])
            .arg(dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let status = |root: &Path| {
        let o = dev(root, &["--status"]);
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    let row = |text: &str, name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .map(|l| l.split_whitespace().nth(1).unwrap_or("").to_string())
    };
    let wait = |what: &str, mut f: Box<dyn FnMut() -> bool>| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !f() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let root = dir.path().to_path_buf();
    wait("api+db running, worker off", {
        let root = root.clone();
        Box::new(move || {
            let t = status(&root);
            row(&t, "api").as_deref() == Some("running")
                && row(&t, "db").as_deref() == Some("running")
                && row(&t, "worker").as_deref() == Some("off")
                && row(&t, "web").as_deref() == Some("off")
        })
    });

    let mut s = UnixStream::connect(dir.path().join(".logs/atlas.sock")).unwrap();
    s.write_all(b"{\"cmd\":\"start_service\",\"args\":{\"service\":\"worker\"}}\n")
        .unwrap();
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("\"ok\":true"));
    wait("worker running", {
        let root = root.clone();
        Box::new(move || row(&status(&root), "worker").as_deref() == Some("running"))
    });
}
