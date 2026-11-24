//! Who is on the other end of the helper's socket. The socket's file mode (`0660 root:atlas-crew`)
//! already keeps crewmate users out; this is the second lock: the helper reads the connecting
//! process's uid from the kernel (`SO_PEERCRED`) and serves only the crew user (or root, for the
//! helper's own tooling). A crewmate that somehow got a connected fd still gets nothing.

/// The kernel's account of the connecting process (`atlas_core::sys::peer_cred`).
pub use atlas_core::sys::PeerCred as Peer;

/// Is `peer` allowed to talk to the helper? Only `crew_uid` (the uid of `atlas-crew`) and root.
/// A pure function so the rule is tested without being another user.
pub fn allowed(peer: Peer, crew_uid: u32) -> bool {
    peer.uid == crew_uid || peer.uid == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

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
        let me = atlas_core::sys::euid();
        let peer = atlas_core::sys::peer_cred(&a).unwrap();
        assert_eq!(peer.uid, me);
        assert!(allowed(peer, me));
        assert!(!allowed(peer, me.wrapping_add(1)) || me == 0);
    }
}
