//! `Request::Git`: run `git` as a crewmate, in that crewmate's tree, for hostd.
//!
//! Why the helper does this and not hostd: the tree is the crewmate's, so its `.git/config` is too, and
//! git runs what that config names: `core.fsmonitor`, clean/smudge/textconv filters, `core.sshCommand`,
//! credential helpers, `protocol.ext.allow`. hostd (the crew user, who owns `hostd.token` and reads every
//! crewmate's `secrets.json`) running `git diff` there would run the crewmate's command as itself. Here
//! the same command runs as the crewmate, whose code it already is. Security review of #448, B2.
//!
//! The helper stays small: a subcommand from [`super::validate::GIT_SUBCOMMANDS`] first (so no `-c`),
//! a cwd inside the crewmate's directory, the same identity switch as a spawn, `git` looked up in
//! pinned directories, a fixed environment (the request cannot add to it), a deadline that kills the
//! process group, and output in anonymous files with a size limit so a runaway diff cannot fill memory.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::exec::{Identity, Ids, drop_for};
use super::protocol::{GitReq, Response};
use super::server::Config;
use super::validate::dir_in;

/// Where `git` is looked up, in order. Pinned, never taken from the request or the crewmate's `PATH`.
pub const GIT_DIRS: [&str; 3] = ["/usr/bin", "/usr/local/bin", "/bin"];
/// `PATH` of the git process: `git` finds `ssh` and its own helpers here.
const GIT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
/// Most that stdout may hold (`RLIMIT_FSIZE`): a diff of a huge tree ends the process instead of memory.
pub const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
/// How much of stderr the reply keeps (the end of it, where git says why it failed).
const MAX_STDERR: u64 = 16 * 1024;

/// Run one request. The reply carries stdout as an fd.
pub fn run(cfg: &Config, req: &GitReq) -> (Response, Option<OwnedFd>) {
    match run_inner(cfg, req) {
        Ok((resp, fd)) => (resp, Some(fd)),
        Err(e) => (
            Response::Error {
                reason: format!("git: {e}"),
            },
            None,
        ),
    }
}

fn run_inner(cfg: &Config, req: &GitReq) -> io::Result<(Response, OwnedFd)> {
    let ids = (cfg.resolve)(&req.user)?;
    let dir = dir_in(&cfg.cm_root, &req.user)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    // SAFETY: plain getters.
    let (euid, egid) = unsafe { (libc::geteuid(), libc::getegid()) };
    let identity = drop_for(
        Ids {
            uid: euid,
            gid: egid,
        },
        ids,
    )?;
    let git = GIT_DIRS
        .iter()
        .map(|d| format!("{d}/git"))
        .find(|p| std::path::Path::new(p).is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "git is not installed"))?;

    let mut out = scratch()?;
    let mut err = scratch()?;
    let mut cmd = Command::new(git);
    cmd.arg0("git");
    if req.token.is_some() {
        // Reset the helper list, then answer github.com from the environment: the token is never in argv.
        cmd.args([
            "-c",
            "credential.helper=",
            "-c",
            "credential.https://github.com.helper=!f() { echo username=x-access-token; echo \"password=$ATLAS_GIT_TOKEN\"; }; f",
        ]);
    }
    cmd.args(&req.args)
        .env_clear()
        .current_dir(&req.cwd)
        .env("PATH", GIT_PATH)
        .env("HOME", format!("{dir}/home"))
        .env("USER", &req.user)
        .env("LOGNAME", &req.user)
        // Never ask: a prompt would wait on a terminal nobody has.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        // A read must not rewrite the index under an agent that is running git itself.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::from(out.try_clone()?))
        .stderr(Stdio::from(err.try_clone()?));
    if let Some(t) = &req.token {
        cmd.env("ATLAS_GIT_TOKEN", t);
    }
    // SAFETY: the hook runs between fork and exec and calls only async-signal-safe libc functions
    // (setsid, setrlimit, setgroups, setgid, setuid); it allocates nothing.
    unsafe {
        cmd.pre_exec(move || {
            // Its own process group, so the deadline ends the whole tree (a hook, ssh, a helper).
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            let limit = libc::rlimit {
                rlim_cur: MAX_OUTPUT as libc::rlim_t,
                rlim_max: MAX_OUTPUT as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(io::Error::last_os_error());
            }
            if let Identity::To(ids) = identity
                && (libc::setgroups(0, std::ptr::null()) != 0
                    || libc::setgid(ids.gid) != 0
                    || libc::setuid(ids.uid) != 0)
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    // The parent's copies of the stdio fds close with the `Command`; only the files remain.
    drop(cmd);

    let deadline = Instant::now() + Duration::from_secs(req.timeout_secs);
    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            // SAFETY: kill(2) on the process group of a child we own and have not reaped.
            unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // The group may hold something that outlived git (a daemonized hook): end it too.
    // SAFETY: as above; the group id is the child's pid, and a no-such-group answer is fine.
    unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };

    let code = status.code();
    let stderr = tail(&mut err, MAX_STDERR)?;
    out.seek(SeekFrom::Start(0))?;
    Ok((
        Response::Git {
            code,
            stderr,
            timed_out,
        },
        OwnedFd::from(out),
    ))
}

/// The last `max` bytes of `f`, as text.
fn tail(f: &mut File, max: u64) -> io::Result<String> {
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max)))?;
    let mut b = Vec::new();
    f.take(max).read_to_end(&mut b)?;
    Ok(String::from_utf8_lossy(&b).trim().to_string())
}

/// An anonymous file, close-on-exec, that the child writes through its own copy of the fd.
fn scratch() -> io::Result<File> {
    #[cfg(target_os = "linux")]
    let fd = {
        // SAFETY: valid NUL-terminated name.
        unsafe { libc::memfd_create(c"atlas-git".as_ptr(), libc::MFD_CLOEXEC) }
    };
    #[cfg(not(target_os = "linux"))]
    let fd = {
        let mut tpl = std::env::temp_dir()
            .join("atlas-git-XXXXXX")
            .into_os_string()
            .into_encoded_bytes();
        tpl.push(0);
        // SAFETY: `tpl` is a writable NUL-terminated template.
        let fd = unsafe { libc::mkstemp(tpl.as_mut_ptr().cast()) };
        if fd >= 0 {
            // SAFETY: the template now holds the created path; unlinking leaves an anonymous file.
            unsafe {
                libc::unlink(tpl.as_ptr().cast());
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        fd
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a fresh fd we own.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::spawn::testing::{Rig, USER, rig};
    use std::os::unix::fs::PermissionsExt;

    // Every test here runs the real helper loop on a temp socket as the current user (`testing::Rig`), and
    // `git` is the machine's. The identity switch itself needs root: see the ignored test at the end.
    fn req(rig: &Rig, args: &[&str]) -> GitReq {
        GitReq {
            user: USER.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: rig.work().to_string_lossy().into_owned(),
            token: None,
            timeout_secs: 30,
        }
    }

    #[test]
    fn git_runs_in_the_crewmates_tree_and_returns_stdout_the_code_and_the_end_of_stderr() {
        let rig = rig();
        rig.booby_trapped_repo();
        let out = rig
            .client
            .git(&req(&rig, &["rev-parse", "--is-inside-work-tree"]))
            .unwrap();
        assert_eq!((out.code, out.stdout.as_slice()), (Some(0), &b"true\n"[..]));
        assert!(!out.timed_out);
        // A failing command is an answer, with git's own words, not a transport error.
        let out = rig
            .client
            .git(&req(&rig, &["rev-parse", "--verify", "no-such-ref"]))
            .unwrap();
        assert_eq!(out.code, Some(128));
        assert!(out.stderr.starts_with("fatal:"), "{}", out.stderr);
    }

    // The point of the request: what the crewmate's own git config runs, runs with the crewmate's identity
    // (here `USER`, set by the helper) and never hostd's. As non-root the uid is the test's own, so the
    // proof at this level is the environment the helper builds; the real uid switch is the ignored test.
    #[test]
    fn the_crewmates_own_config_runs_inside_the_helpers_git_with_the_crewmates_environment() {
        let rig = rig();
        let marker = rig.booby_trapped_repo();
        let out = rig
            .client
            .git(&req(&rig, &["status", "--porcelain"]))
            .unwrap();
        assert_eq!(out.code, Some(0), "{}", out.stderr);
        let users = Rig::marker_users(&marker);
        assert!(!users.is_empty(), "git status did not run core.fsmonitor");
        assert!(users.iter().all(|u| u == USER), "{users:?}");
    }

    #[test]
    fn only_an_allowed_subcommand_in_the_crewmates_directory_is_run() {
        let rig = rig();
        rig.booby_trapped_repo();
        for bad in [
            vec!["config", "--global", "x.y", "z"],
            vec!["-c", "core.fsmonitor=x", "status"],
            vec!["--exec-path=/tmp", "status"],
            vec!["push"],
            vec![],
        ] {
            let e = rig.client.git(&req(&rig, &bad)).unwrap_err();
            assert!(
                e.to_string().contains("not allowed") || e.to_string().contains("empty"),
                "{bad:?}: {e}"
            );
        }
        let mut r = req(&rig, &["status"]);
        r.cwd = "/tmp".into();
        assert!(rig.client.git(&r).unwrap_err().to_string().contains("cwd"));
        r.cwd = format!("{}/../other", rig.work().display());
        assert!(rig.client.git(&r).is_err());
    }

    // A command that never returns (a hook that sleeps) is ended at the request's deadline with its tree.
    #[test]
    fn a_git_that_runs_past_its_deadline_is_killed_and_reported() {
        let rig = rig();
        rig.booby_trapped_repo();
        let hook = rig.cm.join("slow.sh");
        std::fs::write(&hook, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let set = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(rig.work())
                .status()
                .unwrap()
        };
        assert!(set(&["config", "core.fsmonitor", &hook.to_string_lossy()]).success());
        let mut r = req(&rig, &["status"]);
        r.timeout_secs = 1;
        let t = Instant::now();
        let out = rig.client.git(&r).unwrap();
        assert!(out.timed_out, "{out:?}");
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    }

    // Needs root on Linux: the real identity switch. The planted `core.fsmonitor` of a crewmate's repo must
    // run with that crewmate's uid, not root's and not the crew user's (CI's root job runs `crew::spawn --ignored`).
    #[test]
    #[ignore = "needs root on Linux: creates a system user and changes uid (see docs/contributing.md)"]
    fn a_real_git_request_runs_the_crewmates_config_as_the_crewmates_uid() {
        use std::os::unix::fs::MetadataExt;
        use std::process::Command as C;
        // SAFETY: plain getter.
        assert_eq!(unsafe { libc::geteuid() }, 0, "this test must run as root");
        let user = "atlas-cm-fffffffffffe";
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
        let _guard = Cleanup("atlas-cm-fffffffffffe");
        let ids = exec_lookup(user);
        assert_ne!(ids.uid, 0);

        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("cm");
        let dir = root.join("fffffffffffe");
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(dir.join("home")).unwrap();
        // The crewmate owns its tree and a repo with a planted command; the rest is traversable.
        let marker = work.join("marker");
        let hook = dir.join("hook.sh");
        std::fs::write(
            &hook,
            format!("#!/bin/sh\nid -u > {}\nexit 1\n", marker.display()),
        )
        .unwrap();
        assert!(sh(&format!(
            "chmod 755 {t} {r} {d} {h} && chown -R {user}:{user} {w} {d}/home && chmod 700 {w}",
            t = t.path().display(),
            r = root.display(),
            d = dir.display(),
            h = hook.display(),
            w = work.display(),
        )));
        let as_user = |args: &str| {
            C::new("sh")
                .args([
                    "-c",
                    &format!("cd {} && su -s /bin/sh {user} -c '{args}'", work.display()),
                ])
                .status()
                .unwrap()
                .success()
        };
        assert!(as_user(&format!(
            "git init -q && git config core.fsmonitor {}",
            hook.display()
        )));

        let cfg = Config {
            crew_uid: 0,
            cm_root: root.to_string_lossy().into_owned(),
            engines: vec![],
            resolve: super::super::exec::lookup,
            memory: std::sync::Arc::new(super::super::server::Memory::Off("test".into())),
        };
        let (resp, fd) = run(
            &cfg,
            &GitReq {
                user: user.into(),
                args: vec!["status".into(), "--porcelain".into()],
                cwd: work.to_string_lossy().into_owned(),
                token: None,
                timeout_secs: 30,
            },
        );
        assert!(matches!(resp, Response::Git { .. }), "{resp:?}");
        assert!(fd.is_some());
        // The planted command ran with the crewmate's uid.
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap().trim(),
            ids.uid.to_string()
        );
        assert_eq!(std::fs::metadata(&marker).unwrap().uid(), ids.uid);
    }

    fn exec_lookup(user: &str) -> Ids {
        super::super::exec::lookup(user).unwrap()
    }
}
