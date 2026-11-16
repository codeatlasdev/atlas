//! The helper's one privileged act: start an allowed binary as a crewmate's user and give the
//! caller the pty master (or one end of a socketpair). It needs `setuid` and the controlling-
//! terminal ioctls, so it only does anything useful as root inside the helper; as any other user
//! it can start a process as that same user (what the tests do, on Linux and on a developer's Mac).
//!
//! Order matters and is the point of this file: the slave pty is `fchown`ed to the crewmate (a
//! terminal the caller does not own cannot become its controlling terminal without
//! `CAP_SYS_ADMIN`), then the child does `setsid`, `TIOCSCTTY`, drops groups, gid and uid (in that
//! order: `setuid` first would lose the right to `setgroups`), sets `no_new_privs`, `chdir`s as the
//! crewmate (never before: `Command::current_dir` would enter the directory as root and follow any link
//! in it) and execs. `std::process::Command` does the fork/exec; the hooks only call async-signal-safe
//! functions. [`spawn_becoming`] is the one place that turns a forked child into a crewmate: the agent
//! spawn and the helper's git request both go through it, so they cannot drift apart.

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
pub(super) enum Identity {
    /// Already the right user (a helper under test, or `ids` is the caller).
    Nothing,
    /// Root: drop groups, gid, uid, in that order.
    To(Ids),
}

/// The identity change a spawn as `want` needs when the process runs as `have`. Root becomes
/// anyone but root; anyone else can only be itself, and being asked for another user is refused
/// here, with a message, instead of failing with a bare `EPERM` in the child after the fork.
pub(super) fn drop_for(have: Ids, want: Ids) -> io::Result<Identity> {
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

/// [`spawn_as`] with explicit directories and no memory cap.
pub fn spawn_in(w: &Where<'_>, req: &SpawnReq, ids: Ids, home: &str) -> io::Result<Started> {
    spawn_capped(w, req, ids, home, None)
}

/// [`spawn_in`] under a memory cap. `leaf` is the fd of the crewmate's leaf `cgroup.procs`
/// (see `cgroup::Leaf::procs_fd`): the child writes itself into it in `pre_exec`, before `setuid`, so
/// it never runs outside its memory cap. The fd stays open in the parent (the leaf owns it); the child
/// only writes to it.
pub fn spawn_capped(
    w: &Where<'_>,
    req: &SpawnReq,
    ids: Ids,
    home: &str,
    leaf: Option<RawFd>,
) -> io::Result<Started> {
    let dirs = w.engines;
    check_spawn_in(w.cm_root, req)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let program = resolve(dirs, &req.argv[0])?;
    let mut cmd = Command::new(&program);
    cmd.arg0(&req.argv[0]).args(&req.argv[1..]).env_clear();
    cmd.env("PATH", pinned_path(dirs))
        .env("HOME", home)
        .env("USER", &req.user)
        .env("LOGNAME", &req.user)
        .env("TERM", "xterm-256color");
    // A `/tmp` of its own: the helper's `PrivateTmp=` is one for every crewmate, so a fixed name there is another
    // crewmate's to take and a default-umask file is another crewmate's to read.
    if let Some(tmp) = private_tmp(home, ids) {
        cmd.env("TMPDIR", tmp);
    }
    cmd.envs(&req.env);

    let (fd, child_end) = match req.mode {
        Mode::Pty => open_pty(req, ids)?,
        Mode::Pipe => open_pipe()?,
    };
    // The child's copies become its 0/1/2 (dup2 clears close-on-exec there only); `child_end`
    // closes in this process once the child is started.
    let dup = |fd: &OwnedFd| fd.try_clone().map(Stdio::from);
    cmd.stdin(dup(&child_end)?)
        .stdout(dup(&child_end)?)
        .stderr(Stdio::from(child_end));
    let child = spawn_becoming(
        &mut cmd,
        Becoming {
            identity: identity_for(ids)?,
            leaf,
            tty: matches!(req.mode, Mode::Pty),
            cwd: &req.cwd,
            max_file: None,
        },
    )?;
    Ok(Started { child, fd })
}

/// `<crewmate dir>/tmp`, made `0700` for the crewmate if it is not there (the directory itself is root's, so nothing
/// in it can have been planted), or `None` when it cannot be (a helper under test with an arbitrary `home`).
fn private_tmp(home: &str, ids: Ids) -> Option<String> {
    let tmp = std::path::Path::new(home).parent()?.join("tmp");
    let c = CString::new(tmp.to_str()?).ok()?;
    // SAFETY: a NUL-terminated path; mkdir and lchown (which does not follow a link) are plain syscalls.
    unsafe {
        if libc::mkdir(c.as_ptr(), 0o700) == 0 {
            // Only root can give it away; anyone else made it their own, which is who they will be.
            if libc::geteuid() == 0 && libc::lchown(c.as_ptr(), ids.uid, ids.gid) != 0 {
                return None;
            }
        } else if io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return None;
        }
    }
    Some(tmp.to_string_lossy().into_owned())
}

/// The identity change a process of this helper needs to become `want`.
pub(super) fn identity_for(want: Ids) -> io::Result<Identity> {
    // SAFETY: plain getters.
    let (euid, egid) = unsafe { (libc::geteuid(), libc::getegid()) };
    drop_for(
        Ids {
            uid: euid,
            gid: egid,
        },
        want,
    )
}

/// What a forked child does to become a crewmate before it execs: the one description both of the helper's
/// ways of running a crewmate's code (an agent, and `git` with the hooks of the crewmate's own config) share.
pub(super) struct Becoming<'a> {
    pub identity: Identity,
    /// The fd of the crewmate's leaf `cgroup.procs` (see `cgroup::Leaf::procs_fd`), `None` without a cap.
    pub leaf: Option<RawFd>,
    /// Take stdin (a pty slave) as the controlling terminal.
    pub tty: bool,
    /// Entered after the uid changed, never before: `Command::current_dir` would enter it as root and follow
    /// any link in it, so a link the crewmate planted could start the process inside another user's tree.
    pub cwd: &'a str,
    /// `RLIMIT_FSIZE`: the most any file the process writes may hold.
    pub max_file: Option<u64>,
}

/// Run `cmd` as a crewmate: in the order the module documents, with every step named in the error if it fails.
/// Join the leaf (root only), `setsid`, take the terminal, `RLIMIT_FSIZE`, groups, gid, uid, `no_new_privs`,
/// `chdir`. The helper's unit cannot have `NoNewPrivileges=` (it takes the helper's own `CAP_SETUID` away on
/// systemd 255), so what that option gave every crewmate process is set here, as the crewmate.
pub(super) fn spawn_becoming(cmd: &mut Command, b: Becoming<'_>) -> io::Result<Child> {
    let Becoming {
        identity,
        leaf,
        tty,
        cwd,
        max_file,
    } = b;
    // Prepared before the fork: the hook may not allocate.
    let cwd = CString::new(cwd)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in the working directory"))?;
    let limit = max_file.map(|n| libc::rlimit {
        rlim_cur: n as libc::rlim_t,
        rlim_max: n as libc::rlim_t,
    });
    // A failing step says which one it was through a close-on-exec pipe (one byte, `write(2)` only: the hook
    // may not allocate), so the error is "setuid: Operation not permitted" and not a bare errno.
    let (step_rd, step_wr) = step_pipe()?;
    let step_fd = step_wr.as_raw_fd();
    // SAFETY: the hook runs between fork and exec and calls only async-signal-safe libc functions
    // (write, setsid, ioctl, setrlimit, setgroups, setgid, setuid, prctl, chdir); it allocates nothing.
    unsafe {
        cmd.pre_exec(move || {
            let failed = |step: u8| {
                let e = io::Error::last_os_error();
                libc::write(step_fd, (&raw const step).cast(), 1);
                e
            };
            // First thing, as root: the cgroup file is only writable before the uid changes.
            #[cfg(target_os = "linux")]
            if let Some(fd) = leaf
                && super::cgroup::join(fd).is_err()
            {
                return Err(failed(STEP_CGROUP));
            }
            #[cfg(not(target_os = "linux"))]
            let _ = leaf;
            if libc::setsid() < 0 {
                return Err(failed(STEP_SETSID));
            }
            if tty && libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(failed(STEP_TTY));
            }
            if let Some(l) = &limit
                && libc::setrlimit(libc::RLIMIT_FSIZE, l) != 0
            {
                return Err(failed(STEP_FSIZE));
            }
            if let Identity::To(ids) = identity {
                if libc::setgroups(0, std::ptr::null()) != 0 {
                    return Err(failed(STEP_SETGROUPS));
                }
                if libc::setgid(ids.gid) != 0 {
                    return Err(failed(STEP_SETGID));
                }
                if libc::setuid(ids.uid) != 0 {
                    return Err(failed(STEP_SETUID));
                }
            }
            #[cfg(target_os = "linux")]
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(failed(STEP_NO_NEW_PRIVS));
            }
            if libc::chdir(cwd.as_ptr()) != 0 {
                return Err(failed(STEP_CHDIR));
            }
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| match step_failed(&step_rd) {
        Some(step) => io::Error::new(e.kind(), format!("{step}: {e}")),
        None => e,
    })
}

const STEP_CGROUP: u8 = 1;
const STEP_SETSID: u8 = 2;
const STEP_TTY: u8 = 3;
const STEP_SETGROUPS: u8 = 4;
const STEP_SETGID: u8 = 5;
const STEP_SETUID: u8 = 6;
#[cfg(target_os = "linux")]
const STEP_NO_NEW_PRIVS: u8 = 7;
const STEP_CHDIR: u8 = 8;
const STEP_FSIZE: u8 = 9;

/// A close-on-exec pipe the forked child reports its failing step on; the read end is non-blocking.
fn step_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: `fds` has room for the two descriptors.
    #[cfg(target_os = "linux")]
    let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) };
    #[cfg(not(target_os = "linux"))]
    let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pipe returned two fresh descriptors we now own.
    let (r, w) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    set_cloexec(r.as_raw_fd());
    set_cloexec(w.as_raw_fd());
    // SAFETY: plain fcntl on a descriptor we own (the pipe2 flag above is Linux only).
    unsafe { libc::fcntl(r.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };
    Ok((r, w))
}

/// The step a failed child reported, in words.
fn step_failed(rd: &OwnedFd) -> Option<&'static str> {
    let mut b = 0u8;
    // SAFETY: reads one byte into a live local from a descriptor we own.
    let n = unsafe { libc::read(rd.as_raw_fd(), (&raw mut b).cast(), 1) };
    if n != 1 {
        return None;
    }
    Some(match b {
        STEP_CGROUP => "joining the memory cgroup",
        STEP_SETSID => "setsid",
        STEP_TTY => "taking the terminal",
        STEP_SETGROUPS => "setgroups",
        STEP_SETGID => "setgid",
        STEP_SETUID => "setuid",
        STEP_CHDIR => "entering the working directory",
        STEP_FSIZE => "setting the file size limit",
        #[cfg(target_os = "linux")]
        STEP_NO_NEW_PRIVS => "no_new_privs",
        _ => "an unknown step",
    })
}

/// pty with the slave owned by the crewmate. Returns the master and the slave (the child's stdio),
/// both close-on-exec: see [`open_pipe`].
fn open_pty(req: &SpawnReq, ids: Ids) -> io::Result<(OwnedFd, OwnedFd)> {
    let ws = libc::winsize {
        ws_row: req.rows,
        ws_col: req.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let (master, slave) = pty_pair(&ws)?;
    // SAFETY: valid fd; mode 0620 like a login tty.
    if unsafe { libc::fchown(slave.as_raw_fd(), ids.uid, ids.gid) } != 0
        || unsafe { libc::fchmod(slave.as_raw_fd(), 0o620) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((master, slave))
}

/// A pty whose two ends are close-on-exec **from the instant they exist**. glibc's `openpty` makes both without
/// it, and the helper serves each connection on its own thread: a fork on another thread inside that window
/// would carry this crewmate's terminal into another crewmate's process across `exec` (a read of its output, input
/// into its agent). Security review of the crew isolation, B2. `posix_openpt(O_CLOEXEC)` and `TIOCGPTPEER` (the
/// slave opened from the master's fd, no path) leave no window. Elsewhere (a developer's Mac) `openpty` and an
/// immediate `fcntl`.
#[cfg(target_os = "linux")]
fn pty_pair(ws: &libc::winsize) -> io::Result<(OwnedFd, OwnedFd)> {
    let flags = libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC;
    // SAFETY: plain syscalls; the descriptors are wrapped as soon as they exist.
    unsafe {
        let m = libc::posix_openpt(flags);
        if m < 0 {
            return Err(io::Error::last_os_error());
        }
        let master = OwnedFd::from_raw_fd(m);
        if libc::grantpt(m) != 0 || libc::unlockpt(m) != 0 {
            return Err(io::Error::last_os_error());
        }
        let s = libc::ioctl(m, libc::TIOCGPTPEER as _, flags);
        if s < 0 {
            return Err(io::Error::last_os_error());
        }
        let slave = OwnedFd::from_raw_fd(s);
        if libc::ioctl(m, libc::TIOCSWINSZ as _, std::ptr::from_ref(ws)) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((master, slave))
    }
}

#[cfg(not(target_os = "linux"))]
fn pty_pair(ws: &libc::winsize) -> io::Result<(OwnedFd, OwnedFd)> {
    let (mut m, mut s): (RawFd, RawFd) = (-1, -1);
    // SAFETY: out-pointers are live; macOS declares termios and winsize `*mut` (openpty never writes through them).
    if unsafe {
        libc::openpty(
            &mut m,
            &mut s,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::from_ref(ws).cast_mut().cast(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty returned two fresh fds we now own.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
    set_cloexec(master.as_raw_fd());
    set_cloexec(slave.as_raw_fd());
    Ok((master, slave))
}

/// A socketpair: the child gets one end as stdin and stdout (stderr merged), hostd the other.
/// Both ends are close-on-exec: a process another thread forks meanwhile (the helper serves
/// connections in parallel; a test binary runs tests in parallel) must not inherit the child's
/// end, or hostd never sees EOF until that unrelated process exits (an engine call hung to its
/// timeout in the full suite).
fn open_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut sv = [0 as RawFd; 2];
    // Atomic where the platform has it; elsewhere (macOS, dev only) set right after creation.
    #[cfg(target_os = "linux")]
    let kind = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let kind = libc::SOCK_STREAM;
    // SAFETY: `sv` has room for the two fds.
    if unsafe { libc::socketpair(libc::AF_UNIX, kind, 0, sv.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socketpair returned two fresh fds we now own.
    let (ours, theirs) = unsafe { (OwnedFd::from_raw_fd(sv[0]), OwnedFd::from_raw_fd(sv[1])) };
    set_cloexec(ours.as_raw_fd());
    set_cloexec(theirs.as_raw_fd());
    Ok((ours, theirs))
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
            "#!/bin/sh\necho \"uid=$(/usr/bin/id -u) cwd=$(pwd) home=$HOME user=$USER path=$PATH secret=$SECRET ld=${LD_PRELOAD:-none} args=$*\"\nwhile read k v; do [ \"$k\" = NoNewPrivs: ] && echo nnp=$v; done < /proc/self/status\n[ -t 0 ] && echo tty=yes || echo tty=no\nread line; echo got=$line\n",
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

    // Incident (real-systemd end-to-end test): the helper's unit had `NoNewPrivileges=yes`, which on systemd 255
    // took `CAP_SETUID` out of the helper's own permitted set (every spawn then failed at `setuid` with EPERM; the
    // unit works with the option off, checked by leaving each option out in turn). What it was for is that no
    // process a crewmate runs gains privileges through a setuid binary, so the child sets it on itself.
    // no_new_privs and /proc are Linux's.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_child_cannot_gain_privileges_through_a_setuid_binary() {
        let f = fixture();
        let out = run(&f, Mode::Pipe);
        assert!(
            out.contains("nnp=1"),
            "the child must set no_new_privs on itself: {out}"
        );
    }

    // Incident (real-systemd end-to-end test): a spawn failed with a bare "Operation not permitted" and nothing
    // said which of cgroup join, setsid, setgroups, setgid or setuid after the fork had been refused. The
    // step is now in the message (and a failure of chdir or exec, which std reports itself, has none).
    // The cgroup join exists on Linux only.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_step_that_fails_after_the_fork_is_named_in_the_error() {
        use std::os::fd::AsRawFd;
        let f = fixture();
        // A read-only descriptor: writing `0` to it, as the cgroup join does, fails in the child.
        let ro = std::fs::File::open("/dev/null").unwrap();
        ro.as_raw_fd();
        let err = spawn_capped(
            &Where {
                engines: &[f.engines.as_str()],
                cm_root: &f.root,
            },
            &req(&f, Mode::Pipe),
            me(),
            "/tmp",
            Some(ro.as_raw_fd()),
        )
        .err()
        .expect("the cgroup join fails");
        assert!(
            err.to_string().contains("joining the memory cgroup"),
            "the failing step is named: {err}"
        );
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

    // Regression: an engine call through the helper hung to its 20 s timeout in the full suite.
    // The child's end of the socketpair (and the pty slave) was not close-on-exec, so a process
    // another thread forked meanwhile inherited it and hostd never saw EOF.
    #[test]
    fn no_fd_a_spawn_creates_leaks_into_other_forks() {
        let cloexec = |fd: &OwnedFd| {
            // SAFETY: F_GETFD on an fd we own.
            let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
            flags >= 0 && flags & libc::FD_CLOEXEC != 0
        };
        let (ours, theirs) = open_pipe().unwrap();
        assert!(cloexec(&ours), "hostd's end of the socketpair");
        assert!(cloexec(&theirs), "the child's end of the socketpair");
        // Straight from creation, before anything else runs: a fork on another thread has no window.
        let ws = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (m, s) = pty_pair(&ws).unwrap();
        assert!(cloexec(&m) && cloexec(&s), "a fresh pty is close-on-exec");
        let (master, slave) = open_pty(&req(&fixture(), Mode::Pty), me()).unwrap();
        assert!(cloexec(&master), "the pty master");
        assert!(cloexec(&slave), "the pty slave");
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
        // The helper's unit has no NoNewPrivileges=: what it gave is set on the child itself, as the crewmate.
        assert!(
            out.contains("nnp=1"),
            "the crewmate's process has no no_new_privs: {out}"
        );

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

    // Needs root, like the test above. Incidents (security review of the crew isolation): N1, the helper's
    // listening socket (fd 3 from systemd, close-on-exec cleared) was inherited by every agent; N4,
    // `Command::current_dir` entered the working directory as root, before `setuid`, following any link in it,
    // so a crewmate that swapped a directory of its own for a link got a process started inside another user's
    // private tree. Now the child enters it as the crewmate: a link into a directory it may not enter fails
    // the spawn (the child never execs), and the child holds nothing but stdio.
    #[test]
    #[ignore = "needs root on Linux: creates a system user and changes uid (see docs/contributing.md)"]
    fn a_spawned_child_holds_only_stdio_and_cannot_be_started_inside_another_users_directory_through_a_link()
     {
        use crate::crew::spawn::testing::{PROBE, probe_fds, systemd_listener};
        use std::process::Command as C;
        // SAFETY: plain getter.
        assert_eq!(unsafe { libc::geteuid() }, 0, "this test must run as root");
        let f = fixture();
        let user = "atlas-cm-cccccccccccc";
        let sh = |cmd: &str| C::new("sh").args(["-c", cmd]).status().unwrap().success();
        assert!(sh(&format!(
            "useradd --system --no-create-home --shell /usr/sbin/nologin {user}"
        )));
        let _guard = Cleanup(user);
        let ids = lookup(user).unwrap();
        let other = lookup("nobody").unwrap();
        assert_ne!(ids.uid, other.uid);

        let t = std::path::Path::new(&f.root)
            .parent()
            .unwrap()
            .to_path_buf();
        let dir = format!("{}/cccccccccccc", f.root);
        let secret = t.join("secret");
        std::fs::create_dir_all(format!("{dir}/work")).unwrap();
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::write(secret.join("flag"), "other user's file\n").unwrap();
        std::fs::write(
            format!("{}/claude", f.engines),
            format!("#!/bin/bash\nexec sh -c '{PROBE}'\n"),
        )
        .unwrap();
        assert!(sh(&format!(
            "chmod 755 {t} {r} {d} {e} {e}/claude && chown {user}:{user} {d}/work && chmod 700 {d}/work && \
             chown {o}:{og} {s} && chmod 700 {s}",
            t = t.display(),
            r = f.root,
            d = dir,
            e = f.engines,
            o = other.uid,
            og = other.gid,
            s = secret.display(),
        )));
        let w = Where {
            engines: &[f.engines.as_str(), "/usr/bin", "/bin"],
            cm_root: &f.root,
        };
        let mut r = req(&f, Mode::Pipe);
        r.user = user.into();
        r.argv = vec!["claude".into()];
        r.cwd = format!("{dir}/work");

        // N1 and nothing else open, with the listener left the way systemd leaves it.
        let (_listener, _socket) = systemd_listener(&t);
        let s = spawn_in(&w, &r, ids, &format!("{dir}/home")).unwrap();
        let mut seen = String::new();
        std::fs::File::from(s.fd.try_clone().unwrap())
            .read_to_string(&mut seen)
            .ok();
        assert_eq!(probe_fds(&seen), [0, 1, 2], "{seen}");
        assert!(seen.contains("NoNewPrivs:\t1"), "{seen}");

        // N4: the link is the crewmate's own doing (it could not make it in root's directory, but `work/` is its).
        std::os::unix::fs::symlink(&secret, format!("{dir}/work/link")).unwrap();
        r.cwd = format!("{dir}/work/link");
        match spawn_in(&w, &r, ids, &format!("{dir}/home")) {
            Ok(mut s) => {
                let mut seen = String::new();
                std::fs::File::from(s.fd.try_clone().unwrap())
                    .read_to_string(&mut seen)
                    .ok();
                s.child.wait().ok();
                panic!("a process started inside another user's private directory: {seen}");
            }
            Err(e) => assert!(
                e.to_string().contains("entering the working directory"),
                "the spawn failed, but not at the chdir as the crewmate: {e}"
            ),
        }
    }

    struct Cleanup(&'static str);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::process::Command::new("userdel").arg(self.0).status();
        }
    }
}
