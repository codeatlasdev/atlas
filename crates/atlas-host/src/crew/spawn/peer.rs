//! Who is on the other end of the helper's socket. The socket's file mode (`0660 root:atlas-crew`)
//! already keeps crewmate users out; this is the second lock: the helper reads the connecting
//! process's uid from the kernel (`SO_PEERCRED`) and serves only the crew user (or root, for the
//! helper's own tooling). A crewmate that somehow got a connected fd still gets nothing.

use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

/// The kernel's account of the connecting process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer {
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
}

/// Read the peer credentials of a connected unix socket.
#[cfg(target_os = "linux")]
pub fn peer_of(sock: &UnixStream) -> io::Result<Peer> {
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
    Ok(Peer {
        uid: c.uid,
        gid: c.gid,
        pid: c.pid as u32,
    })
}

/// Read the peer credentials of a connected unix socket (`getpeereid`; macOS has no peer pid here).
#[cfg(not(target_os = "linux"))]
pub fn peer_of(sock: &UnixStream) -> io::Result<Peer> {
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: both out-pointers are live.
    if unsafe { libc::getpeereid(sock.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Peer { uid, gid, pid: 0 })
}

/// Is `peer` allowed to talk to the helper? Only `crew_uid` (the uid of `atlas-crew`) and root.
/// A pure function so the rule is tested without being another user.
pub fn allowed(peer: Peer, crew_uid: u32) -> bool {
    peer.uid == crew_uid || peer.uid == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(uid: u32) -> Peer {
        Peer {
            uid,
            gid: uid,
            pid: 1,
        }
    }

    #[test]
    fn only_the_crew_user_and_root_may_connect() {
        assert!(allowed(p(990), 990));
        assert!(allowed(p(0), 990));
        for uid in [991, 1000, 65534, 989, u32::MAX] {
            assert!(!allowed(p(uid), 990), "{uid}");
        }
        // A crew uid of 0 would make the check meaningless; nobody else may slip in through it.
        assert!(!allowed(p(1000), 0));
    }

    #[test]
    fn the_kernel_reports_our_own_uid_for_a_socketpair_peer() {
        let (a, _b) = UnixStream::pair().unwrap();
        let me = unsafe { libc::geteuid() };
        let peer = peer_of(&a).unwrap();
        assert_eq!(peer.uid, me);
        assert!(allowed(peer, me));
        assert!(!allowed(peer, me.wrapping_add(1)) || me == 0);
    }
}
