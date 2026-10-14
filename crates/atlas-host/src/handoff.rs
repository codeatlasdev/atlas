//! Live upgrade: a new hostd takes over every running pty from the old one.
//!
//! Terminals must survive app updates, but a new binary cannot inherit the old
//! process's ptys the normal way. So the old daemon (`upgrade {exe}`) starts
//! `exe hostd --takeover <sock>`, freezes each session (reader stopped, in-flight
//! bytes flushed into the ring) and sends one [`Bundle`] over a unix socket: the
//! session state as JSON plus every pty master fd as `SCM_RIGHTS` ancillary data.
//! The children are not touched: they keep their session (`setsid`) and slave side,
//! only the master moves to the new process, which resumes reading and appending to
//! the same scrollback files. Then the old daemon exits.
//!
//! Adopted children are not children of the new daemon, so `waitpid` is unavailable:
//! exit is detected by pty EOF, then [`wait_exit`] (kqueue `NOTE_EXIT` on macOS,
//! `kill(pid, 0)` polling elsewhere). The exit *code* of an adopted child is
//! therefore usually unknown (`null` in `pty_exit`); the exit itself is reliable.

use std::io;
#[cfg(target_os = "macos")]
use std::os::fd::AsRawFd;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::Scope;

/// Session state that moves with the master fd. The ring is not carried: it is the
/// tail of the scrollback file (`<id>.bin`), which both daemons share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub cwd: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    pub created_at: u64,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: Option<u32>,
    /// Length of `<id>.bin` at freeze time (the new side verifies it).
    pub file_len: u64,
    /// The session already exited (no fd travels with it). Separate from `exit_code`:
    /// "exited, code unknown" must not read as "alive" (`Some(None)` serializes to null).
    pub exited: bool,
    pub exit_code: Option<i32>,
    /// Index into the fd list of the message, when a master travels.
    pub fd: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// Version of the old daemon, for the log line.
    pub from_version: String,
    pub sessions: Vec<SessionState>,
    /// Terminator only: its single fd is the locked pid file. `flock` is per open file
    /// description, so the successor holds the singleton lock from the first instant.
    #[serde(default)]
    pub lock: bool,
}

/// Where the old daemon listens for its successor.
pub fn handoff_socket(dir: &std::path::Path, pid: u32) -> PathBuf {
    dir.join(format!("hostd.handoff.{pid}.sock"))
}

/// Largest fd batch per `sendmsg`; more sessions are split over several messages.
const MAX_FDS: usize = 32;

/// Send `payload` with `fds` attached.
pub fn send_with_fds(sock: RawFd, payload: &[u8], fds: &[RawFd]) -> io::Result<()> {
    assert!(fds.len() <= MAX_FDS);
    let mut iov = libc::iovec {
        iov_base: payload.as_ptr() as *mut _,
        iov_len: payload.len(),
    };
    // SAFETY: CMSG_SPACE is a pure size computation.
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of_val(fds) as u32) } as usize;
    let mut cbuf = vec![0u8; space];
    // SAFETY: zeroed msghdr is valid; pointers below outlive the sendmsg call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    if !fds.is_empty() {
        msg.msg_control = cbuf.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        // SAFETY: the control buffer is sized by CMSG_SPACE for `fds`, so the first
        // header and its data area are in bounds.
        unsafe {
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(fds) as u32) as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr().cast::<u8>(),
                libc::CMSG_DATA(c),
                std::mem::size_of_val(fds),
            );
        }
    }
    loop {
        // SAFETY: `msg` describes live buffers.
        let n = unsafe { libc::sendmsg(sock, &msg, 0) };
        if n >= 0 {
            return if n as usize == payload.len() {
                Ok(())
            } else {
                Err(io::Error::other("short sendmsg"))
            };
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Receive up to `buf.len()` payload bytes and any attached fds.
pub fn recv_with_fds(sock: RawFd, buf: &mut [u8]) -> io::Result<(usize, Vec<OwnedFd>)> {
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    // SAFETY: pure size computation.
    let space =
        unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<RawFd>()) as u32) } as usize;
    let mut cbuf = vec![0u8; space];
    // SAFETY: zeroed msghdr is valid; buffers outlive the call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cbuf.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    let n = loop {
        // SAFETY: `msg` describes live buffers.
        let n = unsafe { libc::recvmsg(sock, &mut msg, 0) };
        if n >= 0 {
            break n as usize;
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    };
    let mut fds = Vec::new();
    // SAFETY: the kernel filled `cbuf`; every header is walked with the libc macros and
    // each SCM_RIGHTS payload is a whole number of fds that we now own exactly once.
    unsafe {
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(c);
                let len = (*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                for i in 0..len / std::mem::size_of::<RawFd>() {
                    let mut fd: RawFd = 0;
                    std::ptr::copy_nonoverlapping(
                        data.add(i * std::mem::size_of::<RawFd>()),
                        (&mut fd as *mut RawFd).cast::<u8>(),
                        std::mem::size_of::<RawFd>(),
                    );
                    // macOS has no MSG_CMSG_CLOEXEC: received fds are inheritable until we say
                    // otherwise. Without this every child (language servers, the next hostd)
                    // kept the pid lock and the pty masters (see the regression test below).
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                    fds.push(OwnedFd::from_raw_fd(fd));
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    Ok((n, fds))
}

/// Chunked wire format on the handoff socket: the bundle is JSON, one `\n`-terminated
/// line per chunk of ≤ [`MAX_FDS`] fd-carrying sessions. `fd` indices are relative to
/// the chunk. The last chunk is an empty-sessions line with `lock: true` carrying the
/// pid-file fd (when given), so the receiver knows it is done.
pub fn encode_chunks(
    bundle: &Bundle,
    fds: &[RawFd],
    lock_fd: Option<RawFd>,
) -> Vec<(Vec<u8>, Vec<RawFd>)> {
    let mut out = Vec::new();
    let mut cur = Bundle {
        from_version: bundle.from_version.clone(),
        sessions: Vec::new(),
        lock: false,
    };
    let mut cur_fds: Vec<RawFd> = Vec::new();
    for s in &bundle.sessions {
        let mut s = s.clone();
        if let Some(i) = s.fd {
            if cur_fds.len() == MAX_FDS {
                out.push((line(&cur), std::mem::take(&mut cur_fds)));
                cur.sessions.clear();
            }
            s.fd = Some(cur_fds.len());
            cur_fds.push(fds[i]);
        }
        cur.sessions.push(s);
    }
    out.push((line(&cur), cur_fds));
    // Terminator: an empty bundle, carrying the lock.
    cur.sessions.clear();
    cur.lock = lock_fd.is_some();
    out.push((line(&cur), lock_fd.into_iter().collect()));
    out
}

fn line(b: &Bundle) -> Vec<u8> {
    let mut v = serde_json::to_vec(b).unwrap_or_default();
    v.push(b'\n');
    v
}

/// Watches a process that is not our child. On macOS it holds a kqueue `NOTE_EXIT`
/// registration made **while the process is alive**, which is the only way to learn its
/// exit status (`NOTE_EXITSTATUS`); a watch opened after it died sees nothing. Elsewhere,
/// or when kqueue refuses, [`ExitWatch::wait`] polls `kill(pid, 0)` and the code is unknown.
pub struct ExitWatch {
    pid: u32,
    #[cfg(target_os = "macos")]
    kq: Option<OwnedFd>,
}

impl ExitWatch {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            #[cfg(target_os = "macos")]
            kq: kqueue_register(pid),
        }
    }

    /// Block until the process is gone; the exit code when the OS reported it.
    pub fn wait(self) -> Option<i32> {
        #[cfg(target_os = "macos")]
        if let Some(kq) = &self.kq {
            return kqueue_wait(kq);
        }
        loop {
            // SAFETY: signal 0 only checks existence/permission.
            if unsafe { libc::kill(self.pid as libc::pid_t, 0) } != 0
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                return None;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(target_os = "macos")]
fn kqueue_register(pid: u32) -> Option<OwnedFd> {
    // SAFETY: kqueue() returns a new fd or -1; wrapped at once so it always closes.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return None;
    }
    // SAFETY: freshly created, owned here.
    let kq = unsafe { OwnedFd::from_raw_fd(kq) };
    // SAFETY: zeroed kevent is a valid "empty" value; fields are set below.
    let mut ev: libc::kevent = unsafe { std::mem::zeroed() };
    ev.ident = pid as usize;
    ev.filter = libc::EVFILT_PROC;
    ev.flags = libc::EV_ADD | libc::EV_ONESHOT;
    ev.fflags = libc::NOTE_EXIT | libc::NOTE_EXITSTATUS;
    // SAFETY: change list of one live kevent, no event list, no timeout.
    let rc = unsafe {
        libc::kevent(
            kq.as_raw_fd(),
            &ev,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    (rc == 0).then_some(kq)
}

/// `Some(code)` when the status is known. Blocks until the registered process exits.
#[cfg(target_os = "macos")]
fn kqueue_wait(kq: &OwnedFd) -> Option<i32> {
    // SAFETY: zeroed kevent as the out buffer.
    let mut out: libc::kevent = unsafe { std::mem::zeroed() };
    // SAFETY: empty change list, one live out kevent, no timeout = block.
    let n = unsafe {
        libc::kevent(
            kq.as_raw_fd(),
            std::ptr::null(),
            0,
            &mut out,
            1,
            std::ptr::null(),
        )
    };
    if n <= 0 || out.flags & libc::EV_ERROR != 0 {
        return None;
    }
    if out.fflags & libc::NOTE_EXITSTATUS != 0 {
        let st = out.data as i32;
        if libc::WIFEXITED(st) {
            return Some(libc::WEXITSTATUS(st));
        }
        if libc::WIFSIGNALED(st) {
            return Some(128 + libc::WTERMSIG(st));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    fn state(id: &str, fd: Option<usize>) -> SessionState {
        SessionState {
            id: id.into(),
            cwd: "/tmp".into(),
            argv: vec!["sh".into(), "-l".into()],
            scope: Scope {
                worktree_id: Some("wt".into()),
                tab_id: None,
                pane_key: Some("p".into()),
            },
            created_at: 7,
            title: "π title".into(),
            cols: 120,
            rows: 40,
            pid: Some(4242),
            file_len: 99,
            exited: false,
            exit_code: None,
            fd,
        }
    }

    #[test]
    fn state_roundtrips_through_json_including_scope_and_unicode() {
        let b = Bundle {
            from_version: "0.5.13".into(),
            lock: false,
            sessions: vec![
                state("a", Some(0)),
                {
                    let mut s = state("b", None);
                    s.exited = true;
                    s.exit_code = Some(3);
                    s
                },
                {
                    let mut s = state("c", None);
                    s.exited = true;
                    s
                },
            ],
        };
        let back: Bundle = serde_json::from_slice(&serde_json::to_vec(&b).unwrap()).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn chunks_split_fds_and_end_with_an_empty_terminator() {
        let n = MAX_FDS + 5;
        let fds: Vec<RawFd> = (0..n as i32).collect();
        let b = Bundle {
            from_version: "x".into(),
            lock: false,
            sessions: (0..n)
                .map(|i| state(&format!("s{i}"), Some(i)))
                .chain([state("dead", None)])
                .collect(),
        };
        let chunks = encode_chunks(&b, &fds, Some(1000));
        assert_eq!(chunks.len(), 3, "two data chunks + terminator");
        assert_eq!(chunks[0].1.len(), MAX_FDS);
        assert_eq!(chunks[1].1.len(), 5);
        let mut seen = Vec::new();
        for (payload, cfds) in &chunks {
            let c: Bundle = serde_json::from_slice(payload.strip_suffix(b"\n").unwrap()).unwrap();
            for s in &c.sessions {
                if let Some(i) = s.fd {
                    // The fd index is chunk-relative and points at that session's fd.
                    let id: usize = s.id[1..].parse().unwrap();
                    assert_eq!(cfds[i], id as RawFd);
                }
                seen.push(s.id.clone());
            }
        }
        assert_eq!(seen.len(), n + 1);
        assert_eq!(
            chunks[2].1,
            vec![1000],
            "the terminator carries the lock fd"
        );
        let t: Bundle = serde_json::from_slice(chunks[2].0.strip_suffix(b"\n").unwrap()).unwrap();
        assert!(t.sessions.is_empty() && t.lock);
    }

    #[test]
    fn fds_cross_a_socketpair_and_stay_usable() {
        let (a, b) = UnixStream::pair().unwrap();
        let (mut pr, pw) = {
            let mut fds = [0; 2];
            // SAFETY: valid 2-element array.
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            // SAFETY: fresh fds owned here.
            unsafe {
                (
                    std::fs::File::from_raw_fd(fds[0]),
                    std::fs::File::from_raw_fd(fds[1]),
                )
            }
        };
        send_with_fds(a.as_raw_fd(), b"hello\n", &[pw.as_raw_fd()]).unwrap();
        drop(pw);
        let mut buf = [0u8; 64];
        let (n, fds) = recv_with_fds(b.as_raw_fd(), &mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello\n");
        assert_eq!(fds.len(), 1);
        // The received fd is the pipe's write end: writing through it reaches `pr`.
        let mut w = std::fs::File::from(fds.into_iter().next().unwrap());
        w.write_all(b"via-fd").unwrap();
        drop(w);
        let mut got = String::new();
        pr.read_to_string(&mut got).unwrap();
        assert_eq!(got, "via-fd");
    }

    /// Regression (1.5.0-alpha.8, hostd.log of a user stuck on "another atlas hostd is running"):
    /// fds received over `SCM_RIGHTS` came without `FD_CLOEXEC` (macOS has no `MSG_CMSG_CLOEXEC`),
    /// so every child spawned after a takeover (pty shells, language servers, the next hostd)
    /// inherited the pid-lock fd and the pty masters. When the daemon died, its shells still held
    /// the `flock`: no hostd could ever start again, and fds piled up across takeovers until
    /// "Too many open files".
    #[test]
    fn received_fds_are_not_inherited_by_children() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hostd.pid");
        let lock = std::fs::File::create(&path).unwrap();
        // SAFETY: valid fd owned by `lock`.
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let (a, b) = UnixStream::pair().unwrap();
        send_with_fds(a.as_raw_fd(), b"lock\n", &[lock.as_raw_fd()]).unwrap();
        drop(lock);
        let mut buf = [0u8; 16];
        let (_, fds) = recv_with_fds(b.as_raw_fd(), &mut buf).unwrap();
        // SAFETY: F_GETFD on an owned, open fd.
        let flags = unsafe { libc::fcntl(fds[0].as_raw_fd(), libc::F_GETFD) };
        assert_ne!(
            flags & libc::FD_CLOEXEC,
            0,
            "received fd must be close-on-exec"
        );
        // A child started while we hold the received lock, like a pty shell after a takeover.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        drop(fds);
        let again = std::fs::File::open(&path).unwrap();
        // SAFETY: valid fd owned by `again`.
        let free = unsafe { libc::flock(again.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            free,
            "a child of the daemon kept the pid lock after the daemon let go"
        );
    }

    #[test]
    fn exit_watch_returns_when_a_non_child_dies_and_reports_its_status() {
        // A grandchild: `sh` forks a subshell and exits, so the subshell is not our child.
        let out = std::process::Command::new("sh")
            .args(["-c", "(sleep 0.4; exit 7) >/dev/null 2>&1 & echo $!"])
            .output()
            .unwrap();
        let pid: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        let t0 = std::time::Instant::now();
        let code = ExitWatch::new(pid).wait();
        assert!(t0.elapsed() < Duration::from_secs(5));
        // SAFETY: existence probe.
        assert_ne!(unsafe { libc::kill(pid as libc::pid_t, 0) }, 0);
        #[cfg(target_os = "macos")]
        assert_eq!(code, Some(7));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(code, None);
    }
}
