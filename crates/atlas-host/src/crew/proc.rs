//! Running `git` and `gh` for the crew with a deadline: a call that never returns (a stalled clone,
//! a credential prompt, a dead network) is killed with everything it started, instead of holding a
//! task, and with it a checkout lock, forever (#296).

use std::time::Duration;

/// How long one `git` or `gh` call may run before it is killed.
const CALL_DEADLINE: Duration = Duration::from_secs(120);

/// Test-only: a shortened [`CALL_DEADLINE`].
#[cfg(test)]
pub(crate) static TEST_CALL_DEADLINE: parking_lot::Mutex<Option<Duration>> =
    parking_lot::Mutex::new(None);

pub(crate) fn call_deadline() -> Duration {
    #[cfg(test)]
    if let Some(d) = *TEST_CALL_DEADLINE.lock() {
        return d;
    }
    CALL_DEADLINE
}

/// Run `cmd` to the end and return its output, or an error naming `label` when it could not start
/// or ran past `deadline`. The child leads its own process group (Unix) and the whole tree is killed
/// on the deadline and when this future is dropped (a cancelled task), so nothing it started
/// survives. Stdin is closed: a prompt for a password fails at once instead of waiting.
pub(crate) async fn run_bounded(
    cmd: &mut tokio::process::Command,
    label: &str,
    deadline: Duration,
) -> Result<std::process::Output, String> {
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| format!("{label}: {e}"))?;
    let mut tree = Tree {
        pid: child.id(),
        done: false,
    };
    let result = tokio::time::timeout(deadline, child.wait_with_output()).await;
    // The call is over (or given up on): a helper it left behind must not outlive it.
    tree.kill();
    match result {
        Ok(out) => out.map_err(|e| format!("{label}: {e}")),
        Err(_) => Err(format!("{label}: timed out after {}s", deadline.as_secs())),
    }
}

/// The process tree of a call, killed when it goes out of scope (a dropped future included).
struct Tree {
    pid: Option<u32>,
    done: bool,
}

impl Tree {
    fn kill(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        let Some(pid) = self.pid else { return };
        #[cfg(unix)]
        {
            // SAFETY: `process_group(0)` made the child's pid its group id, so `-pid` is exactly this
            // call's group. `pid` came from our own spawn: positive and in i32 range.
            if pid <= i32::MAX as u32 {
                unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
            }
        }
        #[cfg(windows)]
        atlas_core::sys::kill_tree(pid);
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        self.kill();
    }
}
