//! The mirror and the rule behind it: the crew user's GitHub login never reaches a process that runs as a
//! crewmate (security review of the crew isolation, N2). "GitHub" here is a real smart-HTTP server over
//! `git http-backend` that answers `401` until it sees the token, so a private repository really needs it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;

use super::*;
use crate::crew::gitrun::Git;
use crate::crew::project::{fetch, prepare};
use crate::crew::spawn::testing::{Rig, rig};
use crate::crew::store::ProjectRef;

const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
const LOGIN_TOKEN: &str = "ghp_crew_user_login_token_0123456789";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(src: &Path, msg: &str) {
    git(src, &["commit", "-q", "--allow-empty", "-m", msg]);
}

/// A repository with one commit on `main`.
fn source(root: &Path) -> PathBuf {
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    commit(&src, "one");
    src
}

/// A private GitHub: `http://127.0.0.1:<port>/priv.git` serves a bare copy of `src` to a client that sends
/// `Authorization: Basic x-access-token:<token>` and answers `401` to any other.
struct PrivateRemote {
    url: String,
    bare: PathBuf,
    stop: Arc<AtomicBool>,
    _root: tempfile::TempDir,
}

impl Drop for PrivateRemote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl PrivateRemote {
    fn serve(src: &Path, token: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bare = root.path().join("priv.git");
        git(
            root.path(),
            &["clone", "-q", "--bare", &src.to_string_lossy(), "priv.git"],
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/priv.git",
            listener.local_addr().unwrap().port()
        );
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, base, token) = (stop.clone(), root.path().to_path_buf(), token.to_string());
        std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((conn, _)) => {
                        let (base, token) = (base.clone(), token.clone());
                        std::thread::spawn(move || {
                            let _ = answer(conn, &base, &token);
                        });
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        Self {
            url,
            bare,
            stop,
            _root: root,
        }
    }

    /// What a push to the remote would do: its `main` becomes `src`'s.
    fn take(&self, src: &Path) {
        git(
            &self.bare,
            &[
                "fetch",
                "-q",
                &src.to_string_lossy(),
                "+refs/heads/*:refs/heads/*",
            ],
        );
    }
}

/// One request: authenticate, hand it to `git http-backend` as CGI, send its answer back.
fn answer(conn: TcpStream, root: &Path, token: &str) -> std::io::Result<()> {
    conn.set_nonblocking(false)?;
    let mut rd = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    rd.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut headers = std::collections::BTreeMap::new();
    loop {
        let mut h = String::new();
        rd.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let mut body = Vec::new();
    if headers
        .get("transfer-encoding")
        .is_some_and(|v| v.contains("chunked"))
    {
        loop {
            let mut size = String::new();
            rd.read_line(&mut size)?;
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            let mut chunk = vec![0; n];
            rd.read_exact(&mut chunk)?;
            body.extend(chunk);
            let mut crlf = String::new();
            rd.read_line(&mut crlf)?;
            if n == 0 {
                break;
            }
        }
    } else if let Some(n) = headers.get("content-length").and_then(|v| v.parse().ok()) {
        body = vec![0; n];
        rd.read_exact(&mut body)?;
    }
    let want = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"))
    );
    let mut out = conn;
    if headers.get("authorization") != Some(&want) {
        return out.write_all(
            b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"private\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    let mut cgi = std::process::Command::new("git");
    cgi.arg("http-backend")
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("REQUEST_METHOD", method)
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("CONTENT_LENGTH", body.len().to_string())
        .env("REMOTE_USER", "x-access-token")
        .env("REMOTE_ADDR", "127.0.0.1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    for (k, env) in [
        ("content-type", "CONTENT_TYPE"),
        ("content-encoding", "HTTP_CONTENT_ENCODING"),
        ("git-protocol", "GIT_PROTOCOL"),
    ] {
        if let Some(v) = headers.get(k) {
            cgi.env(env, v);
        }
    }
    let mut child = cgi.spawn()?;
    child.stdin.take().unwrap().write_all(&body)?;
    let mut reply = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut reply)?;
    child.wait()?;
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(reply.len(), |i| i + 4);
    let head = String::from_utf8_lossy(&reply[..split]).into_owned();
    let status = head
        .lines()
        .find_map(|l| l.strip_prefix("Status:"))
        .map_or("200 OK".to_string(), |s| s.trim().to_string());
    let rest: String = head
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("Status:"))
        .map(|l| format!("{l}\r\n"))
        .collect();
    let payload = &reply[split.min(reply.len())..];
    write!(
        out,
        "HTTP/1.1 {status}\r\n{rest}Content-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )?;
    out.write_all(payload)
}

/// A `gh` that answers `auth token` with [`LOGIN_TOKEN`], installed for the one test that holds the lock.
fn login_with_token(dir: &Path) {
    let gh = dir.join("gh");
    std::fs::write(
        &gh,
        format!("#!/bin/sh\n[ \"$1 $2\" = \"auth token\" ] && echo {LOGIN_TOKEN}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    *crate::crew::proc::TEST_GH_PROGRAM.lock() = Some(gh.display().to_string());
}

struct Crewmate {
    rig: Rig,
    git: Git,
    iso: Isolated,
}

fn crewmate() -> Crewmate {
    let rig = rig();
    let setup = rig.setup();
    let iso = setup.of(ID).unwrap();
    let git = Git::crewmate(&setup.socket, iso.user.clone());
    Crewmate { rig, git, iso }
}

// The reason for the whole design, on a repository that really is private: hostd (the crew user, with its
// `gh` login) makes the mirror, the crewmate clones it with no credential at all, and no process of the crewmate
// ever has the token in its environment (hooks in its own `~/.gitconfig` record it). Without hostd's token the
// remote refuses, so the clone working proves the token went through the mirror.
#[tokio::test]
async fn a_private_repository_reaches_a_crewmate_through_hostds_mirror_and_the_token_stays_with_hostd()
 {
    let _bin = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let remote = PrivateRemote::serve(&src, LOGIN_TOKEN);
    let cm = crewmate();
    let log = cm.rig.env_recording_hooks();
    let project = ProjectRef::Git {
        url: remote.url.clone(),
        branch: None,
    };
    let mirror = Mirror::of(&remote.url, &cm.iso);

    // The control: no login, no repository.
    let err = mirror.refresh_with(None).await.unwrap_err();
    assert!(
        err.contains("Authentication failed")
            || err.contains("401")
            || err.contains("terminal prompts disabled"),
        "{err}"
    );
    assert!(!mirror.dir().exists() && !mirror.dir().with_extension("git.partial").exists());

    login_with_token(tmp.path());
    let work = prepare(
        &project,
        ID,
        &tmp.path().join("hostd"),
        Some(cm.iso.clone()),
        &cm.git,
    )
    .await
    .unwrap();
    // The mirror is the crew user's, in `crew-runtime/`, and the crewmate's origin reads it and pushes to GitHub.
    assert_eq!(mirror.dir().parent(), Some(cm.iso.runtime().as_path()));
    assert!(mirror.dir().join("HEAD").is_file());
    assert_eq!(
        git(&work, &["config", "remote.origin.url"]),
        mirror.local_url()
    );
    assert_eq!(git(&work, &["config", "remote.origin.pushurl"]), remote.url);
    assert_eq!(git(&work, &["rev-list", "--count", "HEAD"]), "1");

    // A commit on GitHub reaches the checkout through hostd's refresh, again with nothing for the crewmate.
    commit(&src, "two");
    remote.take(&src);
    let m = mirror_for(&project, Some(&cm.iso), &cm.git);
    assert!(m.is_some());
    fetch(&work, &cm.git, m.as_ref()).await.unwrap();
    *crate::crew::proc::TEST_GH_PROGRAM.lock() = None;
    assert_eq!(git(&work, &["rev-list", "--count", "HEAD"]), "2");

    let seen = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        seen.contains("HOME="),
        "the recording hooks never ran, so this proved nothing: {seen:?}"
    );
    assert!(
        !seen.contains(LOGIN_TOKEN) && !seen.contains("ATLAS_GIT_TOKEN"),
        "the crew user's token is in a crewmate's git process:\n{seen}"
    );
    // git < 2.45 refuses to serve a repository another uid owns (the mirror is the crew user's): every read of it names
    // an upload-pack that trusts that one directory (the real-systemd E2E, Ubuntu 24.04's git 2.43, runs it).
    assert_eq!(
        mirror.upload_pack(),
        format!(
            "git -c safe.directory={} upload-pack",
            mirror.dir().display()
        )
    );
}

// A crewmate that has a GitHub token of its own uses it, and only it: the real URL, its token in its own
// git process, no mirror, and never the crew user's login.
#[tokio::test]
async fn a_crewmate_with_a_token_of_its_own_clones_the_real_url_with_it_and_hostd_makes_no_mirror()
{
    let _bin = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    const OWN: &str = "ghp_the_crewmates_own_token_9876543210";
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    login_with_token(tmp.path());
    let cm = crewmate();
    let git_own = cm.git.clone().with_token(Some(OWN.into()));
    let log = cm.rig.env_recording_hooks();
    let url = format!("file://{}", src.display());
    let project = ProjectRef::Git {
        url: url.clone(),
        branch: None,
    };
    assert!(mirror_for(&project, Some(&cm.iso), &git_own).is_none());
    let work = prepare(
        &project,
        ID,
        &tmp.path().join("hostd"),
        Some(cm.iso.clone()),
        &git_own,
    )
    .await
    .unwrap();
    *crate::crew::proc::TEST_GH_PROGRAM.lock() = None;
    assert_eq!(git(&work, &["config", "remote.origin.url"]), url);
    let seen = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(seen.contains(&format!("ATLAS_GIT_TOKEN={OWN}")), "{seen}");
    assert!(!seen.contains(LOGIN_TOKEN), "{seen}");
    assert!(
        std::fs::read_dir(cm.iso.runtime())
            .map(|d| d
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".git")))
            .unwrap_or(true),
        "hostd made a mirror for a crewmate with a token of its own"
    );
}

// Only a Git project of an isolated crewmate that has no token of its own is mirrored.
#[test]
fn only_an_isolated_crewmate_without_its_own_token_gets_a_mirror() {
    let cm = crewmate();
    let git = ProjectRef::Git {
        url: "git@github.com:o/r.git".into(),
        branch: None,
    };
    let path = ProjectRef::Path { path: "/x".into() };
    let m = mirror_for(&git, Some(&cm.iso), &cm.git).unwrap();
    assert!(m.local_url().starts_with("file://") && m.local_url().ends_with(".git"));
    assert_eq!(
        m.url, "https://github.com/o/r.git",
        "ssh urls are mirrored as https"
    );
    assert!(mirror_for(&path, Some(&cm.iso), &cm.git).is_none());
    assert!(mirror_for(&git, None, &cm.git).is_none());
    assert!(mirror_for(&git, Some(&cm.iso), &Git::local()).is_none());
    assert!(
        mirror_for(
            &git,
            Some(&cm.iso),
            &cm.git.clone().with_token(Some("t".into()))
        )
        .is_none()
    );
}

// Security review of the crew isolation, S1: the login answered any host's password prompt, so a project on another host
// (or a redirect to one) was sent the operator's GitHub token. It is offered to https://github.com only. And two long URLs
// never share a mirror directory (the old name kept the first 48 bytes of the URL).
#[test]
fn the_login_is_only_offered_to_github_and_every_url_has_its_own_mirror() {
    assert_eq!(
        token_scope("https://github.com/o/r.git").as_deref(),
        Some("https://github.com")
    );
    for other in [
        "https://gitlab.com/o/r.git",
        "https://github.com.evil.example/o/r.git",
        "http://github.com/o/r.git",
        "git@github.com:o/r.git",
        "file:///srv/r",
    ] {
        assert_eq!(token_scope(other), None, "{other}");
    }
    let long = format!("https://github.com/{}", "a".repeat(80));
    assert_ne!(
        url_hash(&format!("{long}/one")),
        url_hash(&format!("{long}/two"))
    );
}

// A clone made before the mirror existed (origin = GitHub, no token any more) is moved to it by the next fetch:
// origin reads the mirror and pushes to the project (review 3, nit).
#[tokio::test]
async fn fetch_moves_an_older_clone_to_the_mirror() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let url = format!("file://{}", src.display());
    let cm = crewmate();
    let project = ProjectRef::Git {
        url: url.clone(),
        branch: None,
    };
    let own = cm
        .git
        .clone()
        .with_token(Some("ghp_own_token_for_the_old_clone".into()));
    let work = prepare(
        &project,
        ID,
        &tmp.path().join("hostd"),
        Some(cm.iso.clone()),
        &own,
    )
    .await
    .unwrap();
    assert_eq!(git(&work, &["config", "remote.origin.url"]), url);
    let m = mirror_for(&project, Some(&cm.iso), &cm.git).unwrap();
    fetch(&work, &cm.git, Some(&m)).await.unwrap();
    assert_eq!(git(&work, &["config", "remote.origin.url"]), m.local_url());
    assert_eq!(git(&work, &["config", "remote.origin.pushurl"]), url);
}

// A clone that fails halfway is not left where a crewmate would clone it as the mirror.
#[tokio::test]
async fn a_failed_mirror_clone_leaves_nothing_behind() {
    let cm = crewmate();
    let tmp = tempfile::tempdir().unwrap();
    let m = Mirror::of(&format!("file://{}/missing", tmp.path().display()), &cm.iso);
    assert!(m.refresh_with(None).await.is_err());
    assert!(!m.dir().exists());
    assert!(!m.dir().with_extension("git.partial").exists());
}

// Needs root: the mirror is the crew user's and the crewmate another uid. The crewmate's git (the helper's real
// identity switch) must be able to clone it (git's ownership check, file modes), and cannot write in it.
#[test]
#[ignore = "needs root on Linux: creates a system user and changes uid (see docs/contributing.md)"]
fn a_crewmate_uid_clones_the_mirror_of_another_uid_and_cannot_write_it() {
    use crate::crew::spawn::protocol::{GitReq, Response};
    use crate::crew::spawn::server::{Config, Memory};
    use std::process::Command as C;
    // SAFETY: plain getter.
    assert_eq!(unsafe { libc::geteuid() }, 0, "this test must run as root");
    let user = "atlas-cm-fffffffffffd";
    let sh = |cmd: &str| C::new("sh").args(["-c", cmd]).status().unwrap().success();
    assert!(sh(&format!(
        "useradd --system --no-create-home --shell /usr/sbin/nologin {user}"
    )));
    struct Cleanup(&'static str);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = C::new("userdel").arg(self.0).status();
        }
    }
    let _guard = Cleanup("atlas-cm-fffffffffffd");
    let ids = crate::crew::spawn::exec::lookup(user).unwrap();

    let t = tempfile::tempdir().unwrap();
    let src = source(t.path());
    let root = t.path().join("cm");
    let iso = Isolated::under(
        &root.to_string_lossy(),
        "ffffffff-fffd-4000-8000-000000000000",
    )
    .unwrap();
    assert_eq!(iso.user, user);
    std::fs::create_dir_all(iso.work()).unwrap();
    std::fs::create_dir_all(iso.home()).unwrap();
    std::fs::create_dir_all(iso.runtime()).unwrap();
    // Provisioning's layout: the directory root's 0711, the crewmate's `work` and `home`, `crew-runtime/` owned by
    // the crew user (root here, another uid than the crewmate) with the crewmate's group, 0750.
    assert!(sh(&format!(
        "chmod 755 {t} {r} && chmod 711 {d} && chown {user}:{user} {w} {h} && chown root:{user} {rt} && chmod 750 {rt}",
        t = t.path().display(),
        r = root.display(),
        d = iso.dir().display(),
        w = iso.work().display(),
        h = iso.home().display(),
        rt = iso.runtime().display(),
    )));
    let mirror = Mirror::of(&format!("file://{}", src.display()), &iso);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(mirror.refresh_with(None))
        .unwrap();

    let cfg = Config {
        crew_uid: 0,
        cm_root: root.to_string_lossy().into_owned(),
        engines: vec![],
        resolve: crate::crew::spawn::exec::lookup,
        memory: Arc::new(Memory::Off("test".into())),
    };
    let run = |args: &[&str], cwd: &Path| {
        let (resp, fd) = crate::crew::spawn::git::run(
            &cfg,
            &GitReq {
                user: user.into(),
                args: args.iter().map(|s| s.to_string()).collect(),
                cwd: cwd.to_string_lossy().into_owned(),
                token: None,
                timeout_secs: 60,
            },
        );
        drop(fd);
        resp
    };
    let work = iso.work().join("checkout");
    let resp = run(
        &[
            "clone",
            "--filter=blob:none",
            &mirror.local_url(),
            &work.to_string_lossy(),
        ],
        &iso.work(),
    );
    assert!(
        matches!(resp, Response::Git { code: Some(0), .. }),
        "the crewmate could not clone the crew user's mirror: {resp:?}"
    );
    assert!(work.join(".git").is_dir());
    // It cannot change the mirror: not as the crewmate's git, not at all.
    let denied = C::new("su")
        .args(["-s", "/bin/sh", user, "-c"])
        .arg(format!("touch {}/x", mirror.dir().display()))
        .status()
        .unwrap();
    assert!(!denied.success(), "the crewmate can write in the mirror");
    let _ = ids;
}
