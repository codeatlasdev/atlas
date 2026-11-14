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

async fn is_github_logged_in() -> bool {
    #[cfg(test)]
    if let Some(v) = *TEST_GITHUB_LOGGED_IN.lock() {
        return v;
    }
    tokio::process::Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
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
