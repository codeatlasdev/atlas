//! The Unix-only half of `sys`: who this process is, signals to a process group, the umask and who is on the other
//! end of a Unix socket. The unix-only crew modules (`atlas-host`'s `crew`) need these; the callers that also build on
//! Windows go through `sys::kill_tree`, which means the same thing there.

use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

pub const SIGKILL: i32 = libc::SIGKILL;
pub const SIGTERM: i32 = libc::SIGTERM;
pub const SIGSTOP: i32 = libc::SIGSTOP;

/// The effective user id (root is 0).
pub fn euid() -> u32 {
    // SAFETY: plain getter, cannot fail.
    unsafe { libc::geteuid() }
}

/// The effective group id.
pub fn egid() -> u32 {
    // SAFETY: plain getter, cannot fail.
    unsafe { libc::getegid() }
}

/// The real user id (differs from [`euid`] only in a set-uid process).
pub fn uid() -> u32 {
    // SAFETY: plain getter, cannot fail.
    unsafe { libc::getuid() }
}

/// Send `sig` to the process group that `leader` leads (`kill(-leader)`): the process and everything it started
/// that did not leave the group. The caller started `leader` with its own group (`process_group(0)` or `setsid`).
/// `leader` 0 and 1 are refused: `kill(-0)` is our own group and `kill(-1)` every process we may signal. So is a
/// number that is not a `pid_t`. Otherwise the OS's answer (`ESRCH` when the group is gone).
pub fn kill_group(leader: u32, sig: i32) -> io::Result<()> {
    if !(2..=i32::MAX as u32).contains(&leader) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{leader} is not a process group leader"),
        ));
    }
    // SAFETY: kill(2) with an exact negative pid has no memory preconditions.
    if unsafe { libc::kill(-(leader as libc::pid_t), sig) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// End `root` and everything in the group it leads, now. Nothing to report: a group that is already gone is the goal.
pub fn kill_tree(root: u32) {
    let _ = kill_group(root, SIGKILL);
}

/// Set the file creation mask and return the previous one. Async-signal-safe: usable in a `pre_exec` hook.
pub fn set_umask(mask: u32) -> u32 {
    // SAFETY: umask(2) cannot fail.
    unsafe { libc::umask(mask as libc::mode_t) as u32 }
}

/// The kernel's account of the process on the other end of a connected Unix socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCred {
    pub uid: u32,
    pub gid: u32,
    /// 0 where the OS does not say (macOS's `getpeereid`).
    pub pid: u32,
}

/// Read the peer credentials of a connected Unix socket (`SO_PEERCRED` on Linux, `getpeereid` elsewhere).
#[cfg(target_os = "linux")]
pub fn peer_cred(sock: &UnixStream) -> io::Result<PeerCred> {
    let mut c = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `c` and `len` are live and sized for SO_PEERCRED's `ucred`.
    let rc = unsafe {
        libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut c as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(PeerCred {
        uid: c.uid,
        gid: c.gid,
        pid: c.pid as u32,
    })
}

/// Read the peer credentials of a connected Unix socket (`getpeereid`; macOS has no peer pid here).
#[cfg(not(target_os = "linux"))]
pub fn peer_cred(sock: &UnixStream) -> io::Result<PeerCred> {
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: both out-pointers are live.
    if unsafe { libc::getpeereid(sock.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(PeerCred { uid, gid, pid: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    fn id(flag: &str) -> u32 {
        let out = Command::new("id").arg(flag).output().unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    // Contract the crew helper relies on to decide "am I root" and "who am I": the numbers are the kernel's, the
    // same ones `id` reports for a child of ours.
    #[test]
    fn the_ids_are_the_ones_id_reports_for_this_user() {
        assert_eq!(euid(), id("-u"));
        assert_eq!(egid(), id("-g"));
        assert_eq!(uid(), id("-ru"));
    }

    #[test]
    fn a_group_signal_reaches_what_the_leader_started() {
        // `sleep` holds the stdout pipe; when the group is killed every holder is gone and the read ends.
        let mut leader = Command::new("sh")
            .args(["-c", "sleep 300 & wait"])
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = leader.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = tx.send(out.read_to_string(&mut s).is_ok());
        });
        // Give the shell time to start `sleep`: the group exists from the spawn, so this only orders the story.
        std::thread::sleep(std::time::Duration::from_millis(100));
        kill_group(leader.id(), SIGKILL).unwrap();
        leader.wait().unwrap();
        assert_eq!(
            rx.recv_timeout(crate::scaling::HANG),
            Ok(true),
            "the grandchild outlived its group"
        );
    }

    // `kill(-0)` is our own group and `kill(-1)` is every process we may signal: a pid that reaches either is a
    // bug in the caller that must never become a mass kill.
    #[test]
    fn a_group_signal_never_addresses_our_own_group_or_every_process() {
        for pid in [0, 1, u32::MAX, i32::MAX as u32 + 1] {
            let e = kill_group(pid, SIGKILL).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{pid}");
        }
    }

    #[test]
    fn a_group_that_is_gone_reports_the_os_error() {
        let mut gone = Command::new("true").process_group(0).spawn().unwrap();
        let pid = gone.id();
        gone.wait().unwrap();
        let e = kill_group(pid, SIGKILL).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn kill_tree_ends_the_whole_group_and_says_nothing_about_a_group_that_is_gone() {
        let mut leader = Command::new("sleep")
            .arg("300")
            .process_group(0)
            .spawn()
            .unwrap();
        kill_tree(leader.id());
        let st = leader.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&st),
            Some(SIGKILL)
        );
        // Already reaped: nothing to do, nothing to panic about.
        kill_tree(leader.id());
    }

    #[test]
    fn the_umask_applies_in_the_child_and_the_call_returns_the_previous_mask() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "umask"]);
        // SAFETY: only `umask(2)`, which is async-signal-safe, runs between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                set_umask(0o077);
                if set_umask(0o027) != 0o077 {
                    return Err(io::Error::other(
                        "set_umask did not return the previous mask",
                    ));
                }
                Ok(())
            });
        }
        let out = cmd.output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0027");
    }

    #[test]
    fn the_kernel_names_our_own_ids_for_the_other_end_of_a_socketpair() {
        let (a, _b) = UnixStream::pair().unwrap();
        let p = peer_cred(&a).unwrap();
        assert_eq!((p.uid, p.gid), (euid(), egid()));
        // Linux reports the peer's pid; macOS's `getpeereid` has none.
        #[cfg(target_os = "linux")]
        assert_eq!(p.pid, std::process::id());
        #[cfg(not(target_os = "linux"))]
        assert_eq!(p.pid, 0);
    }
}
