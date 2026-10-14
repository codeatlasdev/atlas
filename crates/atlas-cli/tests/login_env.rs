//! Incident: Atlas Desktop launched from the desktop/menu on Linux (no terminal, so no shell rc was ever
//! read) failed the stack setup with `exit 127: sh: 1: bun: not found`, because `bun` lives in
//! `~/.bun/bin`, added to PATH only by the user's rc file. Services, `pre_command` and actions all
//! inherited that bare PATH. These tests run the real binary the way a GUI launch does
//! (`PATH=/usr/bin:/bin`, a fake HOME whose rc files add a dir with a fake `bun`) and assert every spawn
//! path finds `bun`. They run on macOS and Linux CI.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

struct Gui {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    proj: PathBuf,
}

impl Gui {
    /// `with_bun` = the rc files put `~/.bun/bin` on PATH; otherwise the shell cannot find `bun`.
    fn new(yaml: &str, with_bun: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = tmp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let bin = home.join(".bun/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let bun = bin.join("bun");
        std::fs::write(&bun, "#!/bin/sh\necho bun-ran \"$@\"\n").unwrap();
        std::fs::set_permissions(&bun, std::fs::Permissions::from_mode(0o755)).unwrap();
        if with_bun {
            let rc = "echo 'welcome banner from rc'\nexport PATH=\"$HOME/.bun/bin:$PATH\"\n";
            for f in [".bashrc", ".bash_profile", ".profile"] {
                std::fs::write(home.join(f), rc).unwrap();
            }
        }
        std::fs::write(proj.join("atlas.yaml"), yaml).unwrap();
        Self {
            _tmp: tmp,
            home,
            proj,
        }
    }

    /// `atlas dev --ci` with the environment of a desktop-entry launch.
    fn ci(&self) -> (Output, Value) {
        let o = Command::new(BIN)
            .args(["dev", "--ci", "--timeout", "20", "--dir"])
            .arg(&self.proj)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("SHELL", "/bin/bash")
            .stdin(Stdio::null())
            .output()
            .expect("run atlas");
        let v = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
        (o, v)
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn have_bash() -> bool {
    Path::new("/bin/bash").exists() || Path::new("/usr/bin/bash").exists()
}

#[test]
fn pre_command_finds_bun_from_the_login_shell_path() {
    if !have_bash() {
        return;
    }
    let g = Gui::new(
        "name: t\npre_command: bun install\nservices:\n  s:\n    command: \"sh -c 'sleep 30'\"\n",
        true,
    );
    let (o, v) = g.ci();
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(v["ok"], true, "{}", text(&o));
    let log = std::fs::read_to_string(g.proj.join(".logs/setup.log")).unwrap();
    assert!(log.contains("bun-ran install"), "{log}");
}

#[test]
fn a_service_finds_bun_from_the_login_shell_path() {
    if !have_bash() {
        return;
    }
    let g = Gui::new(
        "name: t\nservices:\n  web:\n    command: \"bun dev && sleep 30\"\n",
        true,
    );
    let (o, _) = g.ci();
    let log = std::fs::read_to_string(g.proj.join(".logs/web.log")).unwrap_or_default();
    assert!(log.contains("bun-ran dev"), "{log}\n{}", text(&o));
    assert!(!log.contains("not found"), "{log}");
}

/// The 127 case must say which binary was missing, the PATH Atlas used, and what to do.
#[test]
fn a_missing_binary_is_reported_with_its_name_the_path_and_a_hint() {
    if !have_bash() {
        return;
    }
    // A binary no rc file or fallback directory provides.
    let g = Gui::new(
        "name: t\npre_command: atlas-no-such-tool install\nservices:\n  s:\n    command: \"sh -c 'sleep 30'\"\n",
        true,
    );
    let (o, v) = g.ci();
    assert!(!o.status.success());
    let reason = v["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("exit 127"), "{reason}");
    assert!(
        reason.contains("`atlas-no-such-tool`"),
        "binary named: {reason}"
    );
    assert!(
        reason.contains("PATH used by Atlas:"),
        "PATH shown: {reason}"
    );
    assert!(reason.contains("/usr/bin"), "{reason}");
    assert!(
        reason.contains("~/.bashrc") || reason.contains("shell"),
        "hint: {reason}"
    );
    let status: Value =
        serde_json::from_str(&std::fs::read_to_string(g.proj.join(".logs/setup.json")).unwrap())
            .unwrap();
    assert!(
        status["reason"]
            .as_str()
            .unwrap()
            .contains("`atlas-no-such-tool`"),
        "{status}"
    );
}
