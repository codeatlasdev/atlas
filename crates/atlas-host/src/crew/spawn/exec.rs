//! The helper's one privileged act: start an allowed binary as a crewmate's user and give the
//! caller the pty master (or one end of a socketpair). It needs `setuid` and the controlling-
//! terminal ioctls, so it only does anything useful as root inside the helper; as any other user
//! it can start a process as that same user (what the tests do, on Linux and on a developer's Mac).
//!
//! Order matters and is the point of this file: the slave pty is `fchown`ed to the crewmate (a
//! terminal the caller does not own cannot become its controlling terminal without
//! `CAP_SYS_ADMIN`), then the child does `setsid`, `TIOCSCTTY`, drops groups, gid and uid (in that
//! order: `setuid` first would lose the right to `setgroups`), `chdir`s as the crewmate and execs.
//! `std::process::Command` does the fork/exec; the hooks only call async-signal-safe functions.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use super::protocol::{Mode, SpawnReq};
use super::validate::{CM_ROOT, check_spawn_in};

/// A started crewmate process and the fd to hand to hostd.
pub struct Started {
    pub child: Child,
    /// pty master (`Mode::Pty`) or hostd's end of the socketpair (`Mode::Pipe`).
    pub fd: OwnedFd,
}

/// What the helper looked up for the crewmate's user (`getpwnam`), outside this module so the
/// function stays testable with the current user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    pub uid: u32,
    pub gid: u32,
}

/// Directories searched for the engine binary, in order. Pinned by the helper, never taken from
/// the request or from the crewmate's `PATH`.
pub const ENGINE_DIRS: [&str; 3] = [
    "/usr/local/lib/atlas-crew/engines",
    "/usr/local/bin",
    "/usr/bin",
];

fn pinned_path(dirs: &[&str]) -> String {
    dirs.join(":")
}

/// `argv[0]` found in `dirs` (an allowed bare name, checked by `validate`).
fn resolve(dirs: &[&str], bin: &str) -> io::Result<String> {
    dirs.iter()
        .map(|d| format!("{d}/{bin}"))
        .find(|p| std::path::Path::new(p).is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{bin} is not installed")))
}

/// Start `req` as `ids` with engines looked up in [`ENGINE_DIRS`]. The request is checked again
/// here: the validator is the last gate before `setuid`, not only the first.
pub fn spawn_as(req: &SpawnReq, ids: Ids, home: &str) -> io::Result<Started> {
    spawn_in(
        &Where {
            engines: &ENGINE_DIRS,
            cm_root: CM_ROOT,
        },
        req,
        ids,
        home,
    )
}

/// The two directories a spawn depends on; tests point both at temp dirs.
pub struct Where<'a> {
    pub engines: &'a [&'a str],
    pub cm_root: &'a str,
}

/// What the child must do about its identity before `exec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Identity {
    /// Already the right user (a helper under test, or `ids` is the caller).
    Nothing,
    /// Root: drop groups, gid, uid, in that order.
    To(Ids),
}

/// The identity change a spawn as `want` needs when the process runs as `have`. Root becomes
/// anyone but root; anyone else can only be itself, and being asked for another user is refused
/// here, with a message, instead of failing with a bare `EPERM` in the child after the fork.
fn drop_for(have: Ids, want: Ids) -> io::Result<Identity> {
    if want.uid == 0 && have.uid == 0 {
        // A crewmate user whose account resolves to uid 0 (a hand-edited passwd) must never run.
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a crewmate cannot run as uid 0",
        ));
    }
    if have.uid == 0 {
        return Ok(Identity::To(want));
    }
    if have == want {
        return Ok(Identity::Nothing);
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "cannot start a process as uid {} from uid {}: the spawn helper must run as root",
            want.uid, have.uid
        ),
    ))
}

/// [`spawn_as`] with explicit directories.
pub fn spawn_in(w: &Where<'_>, req: &SpawnReq, ids: Ids, home: &str) -> io::Result<Started> {
    let dirs = w.engines;
    check_spawn_in(w.cm_root, req)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let program = resolve(dirs, &req.argv[0])?;
    let mut cmd = Command::new(&program);
    cmd.arg0(&req.argv[0])
        .args(&req.argv[1..])
        .env_clear()
        .current_dir(&req.cwd);
    cmd.env("PATH", pinned_path(dirs))
        .env("HOME", home)
        .env("USER", &req.user)
        .env("LOGNAME", &req.user)
        .env("TERM", "xterm-256color");
    cmd.envs(&req.env);

    let (fd, stdio) = match req.mode {
        Mode::Pty => open_pty(req, ids)?,
        Mode::Pipe => open_pipe()?,
    };
    let [i, o, e] = stdio;
    cmd.stdin(i).stdout(o).stderr(e);
    let tty = matches!(req.mode, Mode::Pty);
    // SAFETY: plain getters.
    let (euid, egid) = unsafe { (libc::geteuid(), libc::getegid()) };
    let identity = drop_for(
        Ids {
            uid: euid,
            gid: egid,
        },
        ids,
    )?;
    // SAFETY: the hook runs between fork and exec and calls only async-signal-safe libc functions
    // (setsid, ioctl, setgroups, setgid, setuid); it allocates nothing.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if tty && libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
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
    let child = cmd.spawn()?;
    Ok(Started { child, fd })
}

/// pty with the slave owned by the crewmate; the child's stdio is the slave. Returns the master.
fn open_pty(req: &SpawnReq, ids: Ids) -> io::Result<(OwnedFd, [Stdio; 3])> {
    let (mut m, mut s): (RawFd, RawFd) = (-1, -1);
    let ws = libc::winsize {
        ws_row: req.rows,
        ws_col: req.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: out-pointers are live; null name/termios are allowed. macOS declares termios and winsize
    // `*mut` where Linux has `*const`; a cast to the type the platform wants is a no-op on Linux and
    // the only way to a single call on both (openpty never writes through them).
    if unsafe {
        libc::openpty(
            &mut m,
            &mut s,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::from_ref(&ws).cast_mut().cast(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty returned two fresh fds we now own.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
    // SAFETY: valid fd; mode 0620 like a login tty.
    if unsafe { libc::fchown(slave.as_raw_fd(), ids.uid, ids.gid) } != 0
        || unsafe { libc::fchmod(slave.as_raw_fd(), 0o620) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    set_cloexec(master.as_raw_fd());
    let dup = |fd: &OwnedFd| fd.try_clone().map(Stdio::from);
    Ok((master, [dup(&slave)?, dup(&slave)?, Stdio::from(slave)]))
}

/// A socketpair: the child gets one end as stdin and stdout (stderr merged), hostd the other.
fn open_pipe() -> io::Result<(OwnedFd, [Stdio; 3])> {
    let mut sv = [0 as RawFd; 2];
    // SAFETY: `sv` has room for the two fds. `SOCK_CLOEXEC` is Linux only, so the flag is set after
    // creation (the helper is single-threaded around a spawn's fork, so there is no window that matters).
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, sv.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socketpair returned two fresh fds we now own.
    let (ours, theirs) = unsafe { (OwnedFd::from_raw_fd(sv[0]), OwnedFd::from_raw_fd(sv[1])) };
    set_cloexec(ours.as_raw_fd());
    let dup = |fd: &OwnedFd| fd.try_clone().map(Stdio::from);
    Ok((ours, [dup(&theirs)?, dup(&theirs)?, Stdio::from(theirs)]))
}

fn set_cloexec(fd: RawFd) {
    // SAFETY: plain fcntl on an fd we own.
    unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
}

/// uid and gid of a system user, by name (`getpwnam_r` through libc).
pub fn lookup(user: &str) -> io::Result<Ids> {
    let name = CString::new(user)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in user"))?;
    let mut buf = vec![0u8; 4096];
    // SAFETY: zeroed passwd is a valid out-parameter for getpwnam_r.
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: all pointers are live for the call; buffer length is passed.
    let rc = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            &mut pw,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut out,
        )
    };
    if rc != 0 {
        return Err(io::Error::from_raw_os_error(rc));
    }
    if out.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such user: {user}"),
        ));
    }
    Ok(Ids {
        uid: pw.pw_uid,
        gid: pw.pw_gid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;

    const USER: &str = "atlas-cm-3f2a9c1e5b7d";

    struct Fixture {
        _t: tempfile::TempDir,
        engines: String,
        root: String,
        work: String,
    }

    /// A fake `claude` that prints what the child looks like: uid, tty, env, cwd, argv.
    fn fixture() -> Fixture {
        let t = tempfile::tempdir().unwrap();
        let engines = t.path().join("engines");
        let work = t.path().join("cm/3f2a9c1e5b7d/work");
        std::fs::create_dir_all(&engines).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let bin = engines.join("claude");
        std::fs::write(
            &bin,
            "#!/bin/sh\necho \"uid=$(/usr/bin/id -u) cwd=$(pwd) home=$HOME user=$USER path=$PATH secret=$SECRET ld=${LD_PRELOAD:-none} args=$*\"\n[ -t 0 ] && echo tty=yes || echo tty=no\nread line; echo got=$line\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Fixture {
            engines: engines.to_string_lossy().into(),
            root: t.path().join("cm").to_string_lossy().into(),
            work: work.to_string_lossy().into(),
            _t: t,
        }
    }

    fn me() -> Ids {
        // SAFETY: plain getters.
        Ids {
            uid: unsafe { libc::geteuid() },
            gid: unsafe { libc::getegid() },
        }
    }

    fn req(f: &Fixture, mode: Mode) -> SpawnReq {
        SpawnReq {
            user: USER.into(),
            argv: vec!["claude".into(), "-p".into(), "a b; echo pwned".into()],
            env: [("SECRET".to_string(), "s3".to_string())].into(),
            cwd: f.work.clone(),
            mode,
            cols: 100,
            rows: 30,
        }
    }

    fn run(f: &Fixture, mode: Mode) -> String {
        let w = Where {
            engines: &[f.engines.as_str()],
            cm_root: &f.root,
        };
        let mut s = spawn_in(&w, &req(f, mode), me(), "/home/x").unwrap();
        let mut io = std::fs::File::from(s.fd.try_clone().unwrap());
        io.write_all(b"hello\n").unwrap();
        let mut out = String::new();
        let mut buf = [0u8; 4096];
        // A pty master reads EIO at hang-up, a socketpair reads 0: both end the loop.
        while let Ok(n) = io.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.push_str(&String::from_utf8_lossy(&buf[..n]));
            if out.contains("got=") {
                break;
            }
        }
        assert!(s.child.wait().unwrap().success(), "{out}");
        out
    }

    #[test]
    fn a_pty_spawn_gives_the_child_a_controlling_terminal_and_the_master_to_the_caller() {
        let out = run(&fixture(), Mode::Pty);
        assert!(out.contains("tty=yes"), "{out}");
        assert!(out.contains("got=hello"), "{out}");
    }

    #[test]
    fn a_pipe_spawn_is_a_socketpair_without_a_tty() {
        let out = run(&fixture(), Mode::Pipe);
        assert!(out.contains("tty=no"), "{out}");
        assert!(out.contains("got=hello"), "{out}");
    }

    // argv is data: the `;` stays inside one argument. The environment is cleared, PATH/HOME/USER
    // are the helper's, the request's variables are added, nothing else leaks in.
    #[test]
    fn argv_is_data_and_the_environment_is_rebuilt_from_scratch() {
        let f = fixture();
        // Something hostd-like in our own environment must not reach the child.
        // SAFETY: test-only, single assertion reads it back through the child.
        unsafe { std::env::set_var("ATLAS_TEST_LEAK", "leaked") };
        let out = run(&f, Mode::Pipe);
        assert!(out.contains("args=-p a b; echo pwned"), "{out}");
        assert!(out.contains("secret=s3"), "{out}");
        // `pwd` reports the resolved path (macOS temp dirs sit behind the /var -> /private/var link).
        let real = std::fs::canonicalize(&f.work).unwrap();
        assert!(out.contains(&format!("cwd={}", real.display())), "{out}");
        assert!(
            out.contains("home=/home/x") && out.contains(&format!("user={USER}")),
            "{out}"
        );
        assert!(out.contains(&format!("path={}", f.engines)), "{out}");
        assert!(out.contains("ld=none"), "{out}");
        assert!(!out.contains("leaked"), "{out}");
    }

    #[test]
    fn a_refused_request_never_reaches_fork() {
        let f = fixture();
        let w = Where {
            engines: &[f.engines.as_str()],
            cm_root: &f.root,
        };
        for tweak in [
            (|r: &mut SpawnReq| r.argv[0] = "sh".into()) as fn(&mut SpawnReq),
            |r| r.cwd = "/tmp".into(),
            |r| r.user = "root".into(),
            |r| {
                r.env.insert("LD_PRELOAD".into(), "/x.so".into());
            },
        ] {
            let mut r = req(&f, Mode::Pipe);
            tweak(&mut r);
            let e = spawn_in(&w, &r, me(), "/h").err().unwrap();
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        }
    }

    #[test]
    fn a_missing_engine_is_not_found_and_leaves_no_child() {
        let f = fixture();
        let w = Where {
            engines: &["/nonexistent"],
            cm_root: &f.root,
        };
        let e = spawn_in(&w, &req(&f, Mode::Pipe), me(), "/h")
            .err()
            .unwrap();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    fn ids(uid: u32, gid: u32) -> Ids {
        Ids { uid, gid }
    }

    // Incident: stage 1 always called setgroups(0), so every spawn by a non-root caller failed
    // with EPERM (CI on Linux, 4 exec tests). Root drops to the crewmate; anyone else is itself.
    #[test]
    fn root_drops_to_the_crewmate_and_others_may_only_be_themselves() {
        assert_eq!(
            drop_for(ids(0, 0), ids(990, 990)).unwrap(),
            Identity::To(ids(990, 990))
        );
        assert_eq!(
            drop_for(ids(1000, 1000), ids(1000, 1000)).unwrap(),
            Identity::Nothing
        );
        for want in [ids(990, 990), ids(1000, 990), ids(990, 1000), ids(0, 0)] {
            let e = drop_for(ids(1000, 1000), want).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{want:?}");
        }
    }

    // A passwd entry that resolves a crewmate to uid 0 must never run, even for root.
    #[test]
    fn a_crewmate_that_resolves_to_uid_zero_is_refused_even_for_root() {
        let e = drop_for(ids(0, 0), ids(0, 0)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("uid 0"), "{e}");
        // Root-owned group with a non-root uid is fine: only the uid matters here.
        assert!(drop_for(ids(0, 0), ids(990, 0)).is_ok());
    }

    #[test]
    fn lookup_finds_root_and_reports_a_missing_user() {
        assert_eq!(lookup("root").unwrap().uid, 0);
        assert_eq!(
            lookup("atlas-cm-000000000000").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            lookup("a\0b").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    // Needs root: a real cross-uid spawn. Run by a root Linux CI job (docs/contributing.md,
    // "Crew isolation tests"): `sudo -E cargo test -p atlas-host crew::spawn -- --ignored`.
    // It creates and deletes a throwaway user, so it never runs in the normal gate.
    #[test]
    #[ignore = "needs root on Linux: creates a system user and changes uid (see docs/contributing.md)"]
    fn a_real_spawn_runs_as_the_crewmates_user_and_hostd_cannot_touch_it() {
        use std::process::Command as C;
        // SAFETY: plain getter.
        assert_eq!(unsafe { libc::geteuid() }, 0, "this test must run as root");
        let f = fixture();
        let user = "atlas-cm-ffffffffffff";
        let sh = |cmd: &str| C::new("sh").args(["-c", cmd]).status().unwrap().success();
        assert!(sh(&format!(
            "useradd --system --no-create-home --shell /usr/sbin/nologin {user}"
        )));
        let guard = Cleanup(user);
        let ids = lookup(user).unwrap();
        assert_ne!(ids.uid, 0);
        // The temp tree is root's 0700 by default: open it to the new user like provisioning would.
        assert!(sh(&format!(
            "chmod -R a+rX {t} && chmod -R a+rwx {w}",
            t = f.root,
            w = f.work
        )));

        let mut r = req(&f, Mode::Pipe);
        r.user = user.into();
        r.cwd = f.work.clone();
        // Slug of the throwaway user is `ffffffffffff`: its directory is `<root>/ffffffffffff`.
        let dir = format!("{}/ffffffffffff", f.root);
        std::fs::create_dir_all(format!("{dir}/work")).unwrap();
        assert!(sh(&format!("chown -R {user} {dir} && chmod 700 {dir}")));
        r.cwd = format!("{dir}/work");
        let w = Where {
            engines: &[f.engines.as_str()],
            cm_root: &f.root,
        };
        let mut s = spawn_in(&w, &r, ids, &format!("{dir}/home")).unwrap();
        let mut io = std::fs::File::from(s.fd.try_clone().unwrap());
        io.write_all(b"x\n").unwrap();
        let mut out = String::new();
        io.read_to_string(&mut out).ok();
        assert!(out.contains(&format!("uid={}", ids.uid)), "{out}");

        // A process of another uid cannot be signalled by an unprivileged caller, and cannot
        // read the crewmate's directory: the isolation #200 asks for.
        let nobody = lookup("nobody").unwrap();
        let mut probe = C::new("sh");
        probe.args(["-c", &format!("ls {dir} >/dev/null 2>&1; echo $?")]);
        // SAFETY: the closure only calls setgid/setuid between fork and exec.
        unsafe {
            probe.pre_exec(move || {
                if libc::setgid(nobody.gid) != 0 || libc::setuid(nobody.uid) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let denied = String::from_utf8(probe.output().unwrap().stdout).unwrap();
        assert_ne!(
            denied.trim(),
            "0",
            "another user could read the crewmate's dir"
        );
        s.child.wait().unwrap();
        drop(guard);
    }

    struct Cleanup(&'static str);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::process::Command::new("userdel").arg(self.0).status();
        }
    }
}
