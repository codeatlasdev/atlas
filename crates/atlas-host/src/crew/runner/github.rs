//! GitHub for a task: whether a project needs a login the crew server does not have yet, and waiting
//! for the user to connect it (the device flow lives in the command handlers).

use std::sync::Arc;
use std::time::Duration;

use super::TASK_TIMEOUT;
use super::outcome::was_cancelled;
#[cfg(test)]
use super::testing::{TEST_FORCE_GITHUB_AUTH, TEST_GITHUB_LOGGED_IN};
use crate::crew::store::{Crewmate, ProjectRef};
use crate::server::Hub;

pub(super) enum WaitResult {
    Connected,
    Cancelled,
    Timeout,
}

/// Returns Some(reason) if crewmate has a github.com Git project AND gh is not logged in.
pub(super) async fn needs_github_auth(crewmate: &Crewmate) -> Option<String> {
    #[cfg(test)]
    if let Some(r) = TEST_FORCE_GITHUB_AUTH.lock().clone() {
        return Some(r);
    }
    let url = match crewmate.project.as_ref()? {
        ProjectRef::Git { url, .. } => url.clone(),
        ProjectRef::Path { .. } => return None,
    };
    if !url.contains("github.com") {
        return None;
    }
    if is_github_logged_in().await {
        return None;
    }
    let parts: Vec<&str> = url
        .trim_end_matches(".git")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let slug = if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        url.clone()
    };
    Some(format!("to clone {slug}"))
}

/// The program that asks GitHub for the login: `gh`, or a fake in a test.
fn gh_program() -> String {
    #[cfg(test)]
    if let Some(p) = super::testing::TEST_GH_PROGRAM.lock().clone() {
        return p;
    }
    "gh".to_string()
}

/// Whether `gh` is logged in to github.com (`pub(super)` for the test that gives it a `gh` that hangs).
pub(super) async fn is_github_logged_in() -> bool {
    #[cfg(test)]
    if let Some(v) = *TEST_GITHUB_LOGGED_IN.lock() {
        return v;
    }
    // A deadline and a tree kill like every git call of a checkout: this runs before any task prepares, so a
    // `gh` that never answers would hold the whole queue (#296). Not logged in is the safe reading of a timeout.
    // tree-access: `gh` with hostd's own arguments, no tree cwd
    let mut cmd = tokio::process::Command::new(gh_program());
    cmd.args(["auth", "status", "--hostname", "github.com"]);
    crate::crew::proc::run_bounded(&mut cmd, "gh", crate::crew::proc::call_deadline())
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(super) async fn wait_for_github_connect(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    task_id: &str,
) -> WaitResult {
    if is_github_logged_in().await {
        return WaitResult::Connected;
    }
    let deadline = tokio::time::Instant::now() + TASK_TIMEOUT;
    loop {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        hub.crew.github_connect_waiters.lock().push(tx);
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return WaitResult::Timeout;
        }
        let _ = tokio::time::timeout(remaining.min(Duration::from_secs(30)), rx).await;
        if was_cancelled(hub, crewmate_id, task_id) {
            return WaitResult::Cancelled;
        }
        if is_github_logged_in().await {
            return WaitResult::Connected;
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    use super::super::testing::TEST_GH_PROGRAM;
    use super::*;
    use crate::crew::proc::TEST_CALL_DEADLINE;

    // #296: `gh auth status` runs before every task prepares; a `gh` that never answers held the whole queue. It is
    // killed at the deadline and reads as "not logged in" (the task then waits for a GitHub connect, which a cancel
    // can end), instead of hanging.
    #[tokio::test]
    async fn a_gh_that_never_answers_reads_as_not_logged_in_at_the_deadline() {
        // The overrides are process-wide: serialised with the tests that set the agent binary.
        let _bin_lock = super::super::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let gh = tmp.path().join("gh");
        // Sleeps ten times the hang ceiling: only the deadline's kill ends it before that.
        std::fs::write(&gh, "#!/bin/sh\nsleep 600\n").unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        *TEST_GH_PROGRAM.lock() = Some(gh.to_string_lossy().into_owned());
        *TEST_CALL_DEADLINE.lock() = Some(Duration::from_secs(1));
        let start = Instant::now();
        let checked = tokio::time::timeout(atlas_core::scaling::HANG, is_github_logged_in()).await;
        *TEST_GH_PROGRAM.lock() = None;
        *TEST_CALL_DEADLINE.lock() = None;
        let logged_in =
            checked.expect("the check outlived its deadline: it waited for the hung gh");
        assert!(!logged_in);
        // The deadline was waited for, not given up at once (a lower bound: load only makes this longer).
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }
}
