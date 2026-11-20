//! GitHub for a crew server: is `gh` there and signed in, the device-flow login the apps drive
//! (start, poll, logout) and waking tasks that wait for the connection. `cli` has the `gh` helpers; `app` is the
//! GitHub App of the server (manifest, key, installation tokens, the git credential helper).

mod api;
pub mod app;
mod cli;
pub(crate) mod reply;
pub(crate) use api::{Call, GhCli, GithubApi, PullRequest};
pub(crate) use reply::REPLY_MARKER;

/// Test-only: the API every `CrewHub::github()` returns. Process-wide, so a test that sets it holds
/// the agent-bin lock (`tests::fixture::lock_agent_bin`) for its whole run.
#[cfg(test)]
pub(crate) static TEST_GITHUB_API: parking_lot::Mutex<Option<Arc<dyn GithubApi>>> =
    parking_lot::Mutex::new(None);

use std::sync::Arc;

use serde_json::{Value, json};

use crate::server::Hub;
use cli::{ensure_gh_installed, gh_auth_setup_git, parse_gh_login, wait_for_gh_code};

/// In-progress `gh auth login` flow.
#[allow(dead_code)] // fields read via json! macro in handlers
pub(crate) struct GithubLoginHandle {
    pub user_code: String,
    pub verification_uri: String,
    /// Shared state updated by the background waiter.
    pub state: Arc<parking_lot::Mutex<GithubLoginState>>,
}

#[derive(Debug, Clone)]
pub(crate) enum GithubLoginState {
    Pending,
    Done,
    Error(String),
}

/// The text of `gh auth status` for github.com (it prints to stderr and stdout), `None` when gh
/// says it is not logged in.
async fn auth_status_text() -> Result<Option<String>, String> {
    let out = crate::crew::proc::gh_output(
        &["auth", "status", "--hostname", "github.com"],
        "gh auth status",
    )
    .await?;
    let text =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    Ok(out.status.success().then_some(text))
}

/// `CrewGithubStatus`
pub(super) async fn status() -> Result<Value, String> {
    let gh_ok = crate::crew::proc::gh_output(&["--version"], "gh --version")
        .await
        .is_ok_and(|o| o.status.success());
    if !gh_ok {
        return Ok(json!({"installed": false, "login": null}));
    }
    let Some(text) = auth_status_text().await? else {
        return Ok(json!({"installed": true, "login": null}));
    };
    let login = parse_gh_login(&text);
    Ok(json!({"installed": true, "login": login}))
}

/// `CrewGithubLoginStart`
pub(super) async fn login_start(hub: &Hub) -> Result<Value, String> {
    let home = dirs::home_dir().ok_or("no home dir")?;
    ensure_gh_installed(&home).await?;
    {
        let mut g = hub.crew.github_login.lock().await;
        *g = None;
    }
    // Interactive and long by design (the user completes it in a browser): not bounded, but started
    // through the one `gh` constructor.
    let mut child = crate::crew::proc::gh_command()
        .args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--git-protocol",
            "https",
            "--web",
        ])
        .env("BROWSER", "false")
        .env("GH_BROWSER", "false")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn gh: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (user_code, verification_uri) =
        wait_for_gh_code(stdout, stderr, std::time::Duration::from_secs(30))
            .await
            .ok_or("gh auth login did not produce a device code within 30s")?;
    let state = Arc::new(parking_lot::Mutex::new(GithubLoginState::Pending));
    let state2 = Arc::clone(&state);
    tokio::spawn(async move {
        let result = child.wait().await;
        let mut s = state2.lock();
        if matches!(*s, GithubLoginState::Pending) {
            match result {
                Ok(status) if status.success() => *s = GithubLoginState::Done,
                Ok(status) => {
                    *s = GithubLoginState::Error(format!("gh auth login exited with {status}"))
                }
                Err(e) => *s = GithubLoginState::Error(format!("wait: {e}")),
            }
        }
    });
    let mut g = hub.crew.github_login.lock().await;
    *g = Some(GithubLoginHandle {
        user_code: user_code.clone(),
        verification_uri: verification_uri.clone(),
        state,
    });
    Ok(json!({"user_code": user_code, "verification_uri": verification_uri}))
}

/// `CrewGithubLoginPoll`
pub(super) async fn login_poll(hub: &Hub) -> Result<Value, String> {
    let g = hub.crew.github_login.lock().await;
    let Some(handle) = g.as_ref() else {
        return Ok(json!({"state": "error", "error": "no login in progress"}));
    };
    let current = handle.state.lock().clone();
    match current {
        GithubLoginState::Pending => Ok(json!({"state": "pending"})),
        GithubLoginState::Error(e) => Ok(json!({"state": "error", "error": e})),
        GithubLoginState::Done => {
            drop(g);
            gh_auth_setup_git().await?;
            let _ = tokio::process::Command::new("git")
                .args([
                    "config",
                    "--global",
                    "url.https://github.com/.insteadOf",
                    "git@github.com:",
                ])
                .status()
                .await;
            let text = auth_status_text().await?.unwrap_or_default();
            let login = parse_gh_login(&text);
            let mut g2 = hub.crew.github_login.lock().await;
            *g2 = None;
            hub.crew.notify_github_connected();
            Ok(json!({"state": "done", "login": login}))
        }
    }
}

/// `CrewGithubLogout`
pub(super) async fn logout(hub: &Hub) -> Result<Value, String> {
    {
        let mut g = hub.crew.github_login.lock().await;
        *g = None;
    }
    let _ = crate::crew::proc::gh_output(
        &["auth", "logout", "--hostname", "github.com"],
        "gh auth logout",
    )
    .await;
    let _ = tokio::process::Command::new("git")
        .args([
            "config",
            "--global",
            "--unset",
            "credential.https://github.com.helper",
        ])
        .status()
        .await;
    let _ = tokio::process::Command::new("git")
        .args([
            "config",
            "--global",
            "--unset",
            "url.https://github.com/.insteadOf",
        ])
        .status()
        .await;
    Ok(json!({}))
}

/// `CrewGithubConnectResume`
pub(super) fn connect_resume(hub: &Hub) -> Result<Value, String> {
    // Always fire the notification; the waiting runner calls is_github_logged_in()
    // after waking and returns Connected only if truly logged in. This also lets
    // tests drive the check via TEST_GITHUB_LOGGED_IN without spawning gh.
    hub.crew.notify_github_connected();
    Ok(json!({}))
}
