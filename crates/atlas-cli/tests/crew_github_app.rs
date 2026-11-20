//! The crewmate side of the crew GitHub App through the real `atlas` binary (#644): git's credential helper
//! (`atlas crew git-credential`, run by a real `git credential fill`) and the `gh` shim (`atlas` run under the name `gh`),
//! against a stand-in for hostd's hook endpoint. The installation token reaches git's and gh's own processes and nothing
//! else: not the shim's stderr, not the environment the agent was started with.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
const TOKEN: &str = "ghs_cli_installation_token_0123456789";

/// A stand-in for hostd's hook server: answers a token for `acme/web` and refuses any other repo; records each request.
struct Hook {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
}

fn hook() -> Hook {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { return };
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            log.lock().unwrap().push(req.clone());
            let (status, body) = if req.contains(r#""path":"acme/web"#) {
                (
                    "200 OK",
                    format!(
                        r#"{{"username":"x-access-token","password":"{TOKEN}","expires_at":1}}"#
                    ),
                )
            } else {
                (
                    "404 Not Found",
                    r#"{"error":"the GitHub App is not installed on that repo"}"#.to_string(),
                )
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Hook { port, seen }
}

/// The environment hostd gives a crewmate's agent for the hook endpoint (no token in it).
fn agent_env(cmd: &mut Command, port: u16) {
    cmd.env("ATLAS_HOOK_PORT", port.to_string())
        .env("ATLAS_HOOK_TOKEN", "crewmate-own-token")
        .env("ATLAS_PANE_KEY", "crew:cm1:t1")
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_REPO")
        .env_remove("GH_HOST");
}

fn text(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

// What git runs when it needs a password for github.com in a crewmate: the helper, asked per operation, for the repo
// git names (`useHttpPath`), answering the way git reads it.
#[test]
fn git_asks_the_helper_per_operation_and_gets_x_access_token_credentials() {
    let hook = hook();
    let mut git = Command::new("git");
    git.args(["credential", "fill"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "credential.helper")
        .env("GIT_CONFIG_VALUE_0", "")
        .env(
            "GIT_CONFIG_KEY_1",
            "credential.https://github.com.useHttpPath",
        )
        .env("GIT_CONFIG_VALUE_1", "true")
        .env("GIT_CONFIG_KEY_2", "credential.https://github.com.helper")
        .env(
            "GIT_CONFIG_VALUE_2",
            format!("!'{BIN}' crew git-credential"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    agent_env(&mut git, hook.port);
    let mut child = git.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol=https\nhost=github.com\npath=acme/web.git\n\n")
        .unwrap();
    let (out, err) = text(&child.wait_with_output().unwrap());

    assert!(out.contains("username=x-access-token\n"), "{out}\n{err}");
    assert!(out.contains(&format!("password={TOKEN}\n")), "{out}\n{err}");
    // The only place the token is printed is the pipe to git: nothing on stderr.
    assert!(!err.contains(TOKEN), "{err}");
    // hostd was asked once, with the crewmate's own token, for the repo git named.
    let seen = hook.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST /crew/cm1/git_credential "),
        "{}",
        seen[0]
    );
    assert!(seen[0].contains("X-Atlas-Hook-Token: crewmate-own-token"));
    assert!(seen[0].contains(r#""path":"acme/web.git""#));
}

/// A `gh` that reports what it was started with: its arguments and the `GH_TOKEN` it got.
fn real_gh(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join("gh");
    std::fs::write(
        &p,
        "#!/bin/sh\nprintf 'args=%s token=%s\\n' \"$*\" \"${GH_TOKEN:-none}\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `atlas` run under the name `gh`, first on PATH, with the real `gh` behind it.
fn run_gh(shims: &Path, real: &Path, port: Option<u16>, args: &[&str]) -> (String, String) {
    let path = std::env::join_paths([shims, real]).unwrap();
    let mut c = Command::new(shims.join("gh"));
    c.args(args).env("PATH", path).current_dir("/");
    match port {
        Some(p) => agent_env(&mut c, p),
        None => {
            c.env_remove("ATLAS_HOOK_PORT")
                .env_remove("ATLAS_HOOK_TOKEN")
                .env_remove("ATLAS_PANE_KEY")
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN");
        }
    }
    text(&c.output().unwrap())
}

#[test]
fn atlas_run_as_gh_gives_the_real_gh_a_token_for_a_covered_repo_and_nothing_otherwise() {
    let hook = hook();
    let tmp = tempfile::tempdir().unwrap();
    let (shims, real): (PathBuf, PathBuf) = (tmp.path().join("shims"), tmp.path().join("real"));
    std::fs::create_dir_all(&shims).unwrap();
    std::os::unix::fs::symlink(BIN, shims.join("gh")).unwrap();
    real_gh(&real);

    // `gh api` on a repo the app covers: the real gh got the token, in its own process.
    let (out, err) = run_gh(
        &shims,
        &real,
        Some(hook.port),
        &["api", "repos/acme/web/issues/5"],
    );
    assert_eq!(
        out,
        format!("args=api repos/acme/web/issues/5 token={TOKEN}\n")
    );
    // The shim itself says nothing: the token is not on its stderr, nor anywhere but gh's environment.
    assert_eq!(err, "");

    // A repo no installation covers: the real gh, untouched (hostd said 404).
    let (out, err) = run_gh(
        &shims,
        &real,
        Some(hook.port),
        &["api", "repos/other/repo/issues/1"],
    );
    assert_eq!(out, "args=api repos/other/repo/issues/1 token=none\n");
    assert_eq!(err, "");

    // A command with no repository never asks hostd.
    let asked = hook.seen.lock().unwrap().len();
    let (out, _) = run_gh(&shims, &real, Some(hook.port), &["auth", "status"]);
    assert_eq!(out, "args=auth status token=none\n");
    assert_eq!(hook.seen.lock().unwrap().len(), asked);

    // Outside a crewmate (no hook endpoint), and with no real gh at all.
    let (out, _) = run_gh(&shims, &real, None, &["api", "repos/acme/web/issues/5"]);
    assert_eq!(out, "args=api repos/acme/web/issues/5 token=none\n");
    let no_real = tmp.path().join("empty");
    std::fs::create_dir_all(&no_real).unwrap();
    let path = std::env::join_paths([shims.as_path(), no_real.as_path()]).unwrap();
    let o = Command::new(shims.join("gh"))
        .args(["pr", "list"])
        .env("PATH", path)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&o.stderr).contains("gh is not installed"));
}

// `atlas` under its own name is still `atlas`: the shim is chosen by the name it was started under, nothing else.
#[test]
fn atlas_under_its_own_name_is_not_the_shim() {
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("Usage"));
}
