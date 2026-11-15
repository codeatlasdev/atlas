//! Every `atlas crew …` command reaches the server through the one ssh helper: the real `atlas`
//! binary runs against a fake `ssh` first on PATH that records its argv. Issue #355 (a server on
//! another port was reachable by install and update only) and the login `PATH` gap.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Output};

/// A fake `ssh`: appends one line per call (`argv` joined by `\x1f`) to `calls`, answers the
/// scripts the commands ask (`WHICH_USER`, status) with plausible output, exits 0.
fn fake_ssh(dir: &Path) {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let p = bin.join("ssh");
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\037' \"$a\"; done >> '{calls}'\necho >> '{calls}'\n\
             case \"$*\" in *'getent passwd'*|*atlas-crew*) echo atlas-crew;; esac\nexit 0\n",
            calls = dir.join("calls").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn atlas(dir: &Path, args: &[&str]) -> Output {
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .env("PATH", path)
        .env("HOME", dir)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn calls(dir: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            l.split('\x1f')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .collect()
}

fn has_port(call: &[String], port: &str) -> bool {
    let t = call.iter().position(|a| a == "atlas-crew@srv");
    call.windows(2)
        .position(|w| w[0] == "-p" && w[1] == port)
        .zip(t)
        .is_some_and(|(p, t)| p < t)
}

// Issue #355: before the fix `--port` was an unknown argument of these commands and, with the
// port in the target, ssh was never given `-p`.
#[test]
fn every_crew_command_passes_the_port_to_every_ssh_call() {
    for args in [
        &["crew", "status", "atlas-crew@srv", "--port", "2222"][..],
        &["crew", "pause", "atlas-crew@srv", "--port", "2222"],
        &["crew", "resume", "atlas-crew@srv", "--port", "2222"],
        &["crew", "logs", "atlas-crew@srv", "--port", "2222"],
        &[
            "crew",
            "uninstall",
            "atlas-crew@srv",
            "--yes",
            "--port",
            "2222",
        ],
        &[
            "crew",
            "login",
            "atlas-crew@srv",
            "claude",
            "--port",
            "2222",
        ],
    ] {
        let dir = tempfile::tempdir().unwrap();
        fake_ssh(dir.path());
        let out = atlas(dir.path(), args);
        let seen = calls(dir.path());
        assert!(
            !seen.is_empty(),
            "{args:?} never ran ssh: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        for c in &seen {
            assert!(
                has_port(c, "2222"),
                "{args:?}: ssh call without -p 2222: {c:?}"
            );
        }
    }
}

// Audit #5: `ssh host claude` runs in a non-interactive shell whose PATH lacks ~/.local/bin; the
// sign-in session must export the agents' dirs like the desktop's check does.
#[test]
fn login_runs_the_agent_with_the_agents_path() {
    let dir = tempfile::tempdir().unwrap();
    fake_ssh(dir.path());
    atlas(dir.path(), &["crew", "login", "atlas-crew@srv", "claude"]);
    let login = calls(dir.path())
        .into_iter()
        .find(|c| c.iter().any(|a| a == "-t"))
        .expect("a terminal ssh call");
    let cmd = login.last().unwrap();
    assert!(
        cmd.contains("$HOME/.local/bin") && cmd.contains("$HOME/.npm-global/bin"),
        "{cmd}"
    );
    assert!(cmd.contains("claude"), "{cmd}");
}

// Port 22 is the default: no `-p` (a port in ~/.ssh/config still applies).
#[test]
fn no_port_means_no_dash_p() {
    let dir = tempfile::tempdir().unwrap();
    fake_ssh(dir.path());
    atlas(dir.path(), &["crew", "pause", "atlas-crew@srv"]);
    let seen = calls(dir.path());
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|c| !c.iter().any(|a| a == "-p")),
        "{seen:?}"
    );
}
