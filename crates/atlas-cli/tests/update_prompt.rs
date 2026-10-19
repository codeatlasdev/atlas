//! `atlas dev` pre-flight update prompt against the real binary under a pty.
//! A fake newer version is injected through the update cache in a temp HOME (the real
//! ~/.atlas is never touched) and `curl` is shadowed by a stub that always fails, so there is
//! no network and no download. Needs python3 (stdlib `pty`).

// The fixtures are sh scripts and Unix sockets.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// Runs `argv` in a pty. For each `[expect, send]` step: wait (20s) for `expect` in the output
/// produced since the previous step, then write `send` (if any). Afterwards: SIGTERM if the
/// process is still alive (the TUI quits gracefully on it), reap, and print
/// `{ok, status, out}` as JSON, `out` being the full transcript.
const DRIVER: &str = r#"
import json, os, pty, select, signal, sys, time

steps = json.loads(os.environ["STEPS"])
argv = json.loads(os.environ["ARGV"])
pid, fd = pty.fork()
if pid == 0:
    os.execv(argv[0], argv)

out = b""
cursor = 0

def pump(timeout):
    global out
    r, _, _ = select.select([fd], [], [], timeout)
    if not r:
        return True
    try:
        d = os.read(fd, 65536)
    except OSError:
        return False
    if not d:
        return False
    out += d
    return True

ok = True
for expect, send in steps:
    end = time.time() + 20
    needle = expect.encode()
    while needle not in out[cursor:] and time.time() < end:
        if not pump(0.2):
            break
    if needle not in out[cursor:]:
        ok = False
        break
    cursor += out[cursor:].index(needle) + len(needle)
    if send is not None:
        os.write(fd, send.encode())

status = None
end = time.time() + 8
while time.time() < end and status is None:
    p, st = os.waitpid(pid, os.WNOHANG)
    if p:
        status = os.waitstatus_to_exitcode(st)
        break
    pump(0.2)
if status is None:
    os.kill(pid, signal.SIGTERM)
    end = time.time() + 8
    while time.time() < end and status is None:
        p, st = os.waitpid(pid, os.WNOHANG)
        if p:
            status = os.waitstatus_to_exitcode(st)
            break
        pump(0.2)
if status is None:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
while pump(0.1):
    pass
print(json.dumps({"ok": ok, "status": status, "out": out.decode(errors="replace")}))
"#;

/// Alternate-screen enter: the TUI took the terminal.
const TUI_STARTED: &str = "\x1b[?1049h";

struct Sandbox {
    home: tempfile::TempDir,
    bin_dir: tempfile::TempDir,
    project: tempfile::TempDir,
}

impl Sandbox {
    fn new(cache: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".atlas")).unwrap();
        std::fs::write(home.path().join(".atlas/.update_check"), cache).unwrap();
        // `curl` always fails: a live check errs silently and `atlas update` fails fast.
        let bin_dir = tempfile::tempdir().unwrap();
        let curl = bin_dir.path().join("curl");
        std::fs::write(
            &curl,
            "#!/bin/sh\necho 'curl stub: no network' >&2\nexit 1\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("atlas.yaml"),
            "name: up\nservices:\n  w:\n    command: sleep 300\n",
        )
        .unwrap();
        Self {
            home,
            bin_dir,
            project,
        }
    }

    fn cache_path(&self) -> PathBuf {
        self.home.path().join(".atlas/.update_check")
    }

    fn cache(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.cache_path()).unwrap()).unwrap()
    }

    fn run(&self, steps: &[(&str, Option<&str>)], env: &[(&str, &str)]) -> Value {
        let argv = [BIN, "dev", "--dir", self.project.path().to_str().unwrap()];
        let path = format!(
            "{}:{}",
            self.bin_dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new("python3");
        cmd.arg("-c")
            .arg(DRIVER)
            .env("HOME", self.home.path())
            .env("PATH", path)
            .env("TERM", "xterm-256color")
            // Debug test binaries count as "linked" (hint only) unless forced.
            .env("ATLAS_UPDATE_PROMPT", "force")
            .env("ARGV", serde_json::to_string(&argv).unwrap())
            .env("STEPS", serde_json::to_string(steps).unwrap());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("python3 is required for this test");
        assert!(
            out.status.success(),
            "driver failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Fresh cache (no live check) advertising a version far above the running one, computed for
/// the channel the sandbox follows: no `update-channel` file and no env override, so the
/// default (`atlas_core::channel`), which is read from the library rather than spelled here.
fn newer_cache() -> String {
    format!(
        r#"{{"last_checked":{},"available_version":"99.0.0","channel":"{}"}}"#,
        now(),
        atlas_core::channel::Channel::default()
    )
}

fn out(r: &Value) -> &str {
    r["out"].as_str().unwrap()
}

#[test]
fn prompt_shows_versions_and_changelog_and_no_continues_and_remembers() {
    let sb = Sandbox::new(&newer_cache());
    let r = sb.run(
        &[
            ("v99.0.0", None),
            ("releases/tag/v99.0.0", None),
            ("Update now? [Y/n] ", Some("n\n")),
            (TUI_STARTED, None), // `n` continues into the TUI
        ],
        &[],
    );
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert!(
        !out(&r).contains("update failed"),
        "`n` must not update: {r}"
    );

    let d = &sb.cache()["dismissed"];
    assert_eq!(d["version"], "99.0.0", "{r}");
    assert!(d["until"].as_u64().unwrap() >= now() + 23 * 3600);
}

#[test]
fn dismissed_version_is_not_prompted_again() {
    let cache = format!(
        r#"{{"last_checked":{},"available_version":"99.0.0","dismissed":{{"version":"99.0.0","until":{}}}}}"#,
        now(),
        now() + 3600
    );
    let sb = Sandbox::new(&cache);
    let r = sb.run(&[(TUI_STARTED, None)], &[]);
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert!(!out(&r).contains("Update now?"), "{r}");
}

#[test]
fn no_prompt_when_disabled_by_env() {
    let sb = Sandbox::new(&newer_cache());
    let r = sb.run(&[(TUI_STARTED, None)], &[("ATLAS_NO_UPDATE_PROMPT", "1")]);
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert!(!out(&r).contains("Update now?"), "{r}");
}

#[test]
fn linked_or_debug_build_only_prints_a_hint() {
    let sb = Sandbox::new(&newer_cache());
    // No `ATLAS_UPDATE_PROMPT=force`: this debug binary behaves like `atlas link`.
    let r = sb.run(
        &[("v99.0.0 is available", None), (TUI_STARTED, None)],
        &[("ATLAS_UPDATE_PROMPT", "")],
    );
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert!(!out(&r).contains("Update now?"), "{r}");
}

#[test]
fn yes_runs_the_update_and_a_failure_offers_to_continue() {
    // `y` reaches the real update logic; the stubbed `curl` fails, so nothing is downloaded and
    // we land on "Continue with v…?". `n` there exits without starting the TUI.
    let sb = Sandbox::new(&newer_cache());
    let r = sb.run(
        &[
            ("Update now? [Y/n] ", Some("y\n")),
            ("update failed", None),
            ("Continue with v", None),
            ("? [Y/n] ", Some("n\n")),
        ],
        &[],
    );
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert_eq!(r["status"], 0, "{r}");
    assert!(!out(&r).contains(TUI_STARTED), "TUI must not start: {r}");
}

/// Incident guard for flipping the default channel: a cache written without a `channel` field
/// is Stable's (older atlas versions only knew Stable). An install that follows another default
/// must not show Stable's offer as its own, or `99.0.0` would be "an update" it never asked for.
#[test]
fn a_cache_from_another_channel_is_not_the_defaults_offer() {
    if atlas_core::channel::Channel::default() == atlas_core::channel::Channel::Stable {
        return; // the Stable-era cache IS the default's: nothing to guard
    }
    let cache = format!(
        r#"{{"last_checked":{},"available_version":"99.0.0"}}"#,
        now()
    );
    let sb = Sandbox::new(&cache);
    let r = sb.run(&[(TUI_STARTED, None)], &[]);
    assert!(r["ok"].as_bool().unwrap(), "{r}");
    assert!(!out(&r).contains("Update now?"), "{r}");
}

#[test]
fn no_prompt_for_non_interactive_flags() {
    let sb = Sandbox::new(&newer_cache());
    let o = Command::new(BIN)
        .args(["dev", "--status", "--dir"])
        .arg(sb.project.path())
        .env("HOME", sb.home.path())
        .output()
        .unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(!all.contains("Update now?"), "{all}");
}
