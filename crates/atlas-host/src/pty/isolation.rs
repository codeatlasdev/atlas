//! A session whose process runs as a crewmate's own Linux user (issue #200). hostd cannot signal,
//! wait for or inspect another uid's process, so everything that touches the process after the spawn
//! goes through the spawn helper: signals, the exit code, the working directory, and the sweep of what
//! the process left behind.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::master::Waiter;
use crate::crew::spawn::client::{Client, is_unknown_session};

/// The part of an isolated session that survives a live upgrade (it travels in `SessionState`): which
/// crewmate user it runs as and which helper session it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsoRef {
    pub user: String,
    pub session: u64,
}

/// An isolated session's handle on the helper.
#[derive(Debug, Clone)]
pub(super) struct Isolation {
    pub client: Client,
    pub iso: IsoRef,
    pub pid: u32,
}

impl Isolation {
    /// Signal the process (`sig` is a libc signal number the helper allows).
    pub(super) fn signal(&self, sig: i32) -> std::io::Result<()> {
        self.client.signal(self.iso.session, sig)
    }

    pub(super) fn cwd(&self) -> std::io::Result<String> {
        self.client.cwd(self.iso.session)
    }

    /// End everything the crewmate's user still runs (what a daemonizing child outlived the shell with).
    pub(super) fn reap(&self) -> std::io::Result<()> {
        self.client.reap(&self.iso.user)
    }

    /// Blocks until the process is gone and yields its code. The helper knows the code; when it no
    /// longer knows the session (it restarted) the pid is watched instead and the code is unknown.
    pub(super) fn waiter(&self) -> Waiter {
        let (client, session, pid) = (self.client.clone(), self.iso.session, self.pid);
        Box::new(move || match client.wait(session) {
            Err(e) if is_unknown_session(&e) => watch_pid(pid),
            _ => client.wait_exit(session).or_else(|| watch_pid(pid)),
        })
    }
}

/// Wait for another user's pid to disappear. `kill(pid, 0)` answers EPERM for a live process of
/// another uid, which counts as alive; only ESRCH ends the wait. The exit code is unknown.
fn watch_pid(pid: u32) -> Option<i32> {
    while atlas_core::sys::pid_alive(pid) {
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident class: a process of another uid answers EPERM to kill(pid, 0). Treating EPERM as
    // "gone" ended the session of a running agent the moment hostd stopped being its owner.
    #[cfg(unix)]
    #[test]
    fn a_live_process_of_another_user_counts_as_alive() {
        // pid 1 belongs to root: EPERM for us (alive), or 0 when the test itself runs as root.
        assert!(atlas_core::sys::pid_alive(1));
    }

    #[test]
    fn a_pid_that_is_gone_ends_the_watch() {
        let mut c = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = c.id();
        c.wait().unwrap();
        assert_eq!(watch_pid(pid), None);
    }

    #[test]
    fn the_handoff_reference_round_trips() {
        let r = IsoRef {
            user: "atlas-cm-3f2a9c1e5b7d".into(),
            session: 9,
        };
        let back: IsoRef = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }
}
