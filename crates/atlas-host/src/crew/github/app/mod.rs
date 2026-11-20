//! The crew server's GitHub App (#643, #644): created from GitHub's manifest flow, its key kept by hostd alone, and
//! used to reach GitHub as `<slug>[bot]` with installation tokens instead of the `gh` user's login.
//!
//! | file       | what it owns                                                                          |
//! |------------|---------------------------------------------------------------------------------------|
//! | `manifest` | the manifest the desktop posts to GitHub: minimal permissions, `public: false`        |
//! | `record`   | `<crew dir>/github-app.json` (0600): the app's id, key and webhook secret             |
//! | `jwt`      | RS256 app JWT with `ring`                                                             |
//! | `tokens`   | per-repo installation token cache, refreshed five minutes before expiry (fake clock)  |
//! | `service`  | conversion, status, removal, installation lookup, token minting, audit                |
//! | `api`      | [`Routed`]: the `GithubApi` that picks the app or `gh` per repository                 |
//! | `helper`   | `atlas crew git-credential`: asks hostd for a token per git operation                 |
//! | `env`      | the git environment of a crewmate: helper, bot identity (no secret)                   |
//! | `http`     | the one HTTP door to api.github.com (a fake on loopback under test)                   |
//! | `webhook`  | `POST /github/webhook`'s parts: signature, replayed ids, hook state, hook config, redelivery |
//!
//! Wire contract: `docs/crew.md` ("GitHub App").

mod api;
mod env;
#[cfg(test)]
pub(crate) mod fake;
pub mod gh_shim;
pub mod helper;
#[cfg(unix)]
mod hooks;
mod http;
mod jwt;
mod manifest;
mod record;
mod service;
#[cfg(test)]
mod tests;
mod tokens;
pub(crate) mod webhook;

use std::sync::Arc;

use serde_json::{Value, json};

use crate::crew::pane;
use crate::protocol::{Failure, HostCmd};
use crate::server::Hub;

pub(crate) use api::Routed;
// Only the Unix-only `crew::tests::github_app` reaches it from outside; on Windows it would be an unused import.
#[cfg(all(test, unix))]
pub(crate) use http::TEST_API_BASE;
pub(crate) use service::{
    AppService, CODE_EXPIRED, CONVERSION_FAILED, EXISTS, FORBIDDEN, NONE, TokenError,
};
// The samples of `protocol::tests::reply_errors_json_contract`: the functions hostd itself fails with.
#[cfg(test)]
pub(crate) use service::{code_expired, conversion_failed, exists, forbidden, none};

/// The username git sends with an installation token as its password.
const USERNAME: &str = "x-access-token";

/// `crew_github_app_*`. `origin` is who is on the connection: a phone never sets the app up (its scopes already
/// refuse these commands; this is the second lock).
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd, origin: &str) -> Result<Value, Failure> {
    if !hub.crew_host {
        return Err("this computer is not a crew server (atlas crew install)".into());
    }
    if origin.starts_with("phone:") {
        return Err(service::forbidden().into());
    }
    let app = &hub.crew.github_app;
    // The public URL setting: the webhook the app is made with, and the one its status reports.
    let url = hub.crew.store.load_crew_settings().public_url;
    let out = match cmd {
        HostCmd::CrewGithubAppManifest {
            owner_kind,
            org,
            workflows,
            server_name,
        } => {
            let server = server_name
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(crate::server::hostname);
            app.manifest(
                &owner_kind,
                org.as_deref(),
                workflows,
                &server,
                url.as_deref(),
            )
        }
        HostCmd::CrewGithubAppConvert { code } => app.convert(&code, url.as_deref()).await,
        HostCmd::CrewGithubAppStatus {} => app.status(url.as_deref()).await,
        HostCmd::CrewGithubAppRemove {} => app.remove().await,
        _ => return Err("not a GitHub App command".into()),
    };
    out.map_err(Failure::from)
}

/// `POST /crew/<id>/git_credential` of the hook server: the installation token git needs for one operation of
/// crewmate `crewmate_id`'s task. The route is open to the crewmate's own hook token only (`hook_auth`), and the
/// pane key must name one of its tasks.
pub(crate) async fn credential_reply(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    v: &Value,
) -> (&'static str, String) {
    let refuse = |status: &'static str, why: String| (status, json!({ "error": why }).to_string());
    let in_task = v["pane_key"]
        .as_str()
        .and_then(|p| pane::task_id(p, crewmate_id))
        .is_some_and(|t| hub.crew.store.task(crewmate_id, t).is_some());
    if !in_task {
        return refuse(
            "422 Unprocessable Entity",
            "git_credential: only a crewmate's task can ask for a token (no task pane)".into(),
        );
    }
    if !v["host"]
        .as_str()
        .is_some_and(|h| h.eq_ignore_ascii_case("github.com"))
    {
        return refuse(
            "422 Unprocessable Entity",
            "git_credential: only github.com".into(),
        );
    }
    // The repo git is talking to; without a path (an old git, a URL with none) the crewmate's own project.
    let repo = v["path"]
        .as_str()
        .and_then(helper::repo_of_path)
        .or_else(|| project_repo(hub, crewmate_id));
    let Some(repo) = repo else {
        return refuse(
            "422 Unprocessable Entity",
            "git_credential: no repository to make a token for".into(),
        );
    };
    match hub.crew.github_app.token_for(&repo).await {
        Ok(t) => (
            "200 OK",
            json!({"username": USERNAME, "password": t.value, "expires_at": t.expires_at})
                .to_string(),
        ),
        Err(e @ (TokenError::NoApp | TokenError::NotCovered(_))) => {
            refuse("404 Not Found", e.to_string())
        }
        Err(e @ TokenError::Failed(_)) => refuse("502 Bad Gateway", e.to_string()),
    }
}

/// `owner/repo` of the crewmate's Git project.
pub(crate) fn project_repo(hub: &Hub, crewmate_id: &str) -> Option<String> {
    let mate = hub.crew.store.load_crewmate(crewmate_id).ok()?;
    match mate.project? {
        crate::crew::store::ProjectRef::Git { url, .. } => {
            crate::crew::project::parse_github_repo(&url)
        }
        crate::crew::store::ProjectRef::Path { .. } => None,
    }
}

/// Whether crewmate `id` works through the app: one is set up, and the crewmate has no `GITHUB_TOKEN` of its own.
pub(crate) fn uses_app(hub: &Hub, id: &str) -> bool {
    matches!(hub.crew.github_app.record(), Ok(Some(_)))
        && !hub.crew.store.secret_env(id).contains_key("GITHUB_TOKEN")
}

/// The environment of crewmate `id`'s agent for git and `gh` through the app: empty when no app is set up, and for
/// a crewmate that carries a `GITHUB_TOKEN` secret of its own (its own credential stays its own). Never a token:
/// the git credential helper and the `gh` shim ask hostd per operation.
///
/// - the helper, the bot identity, the hooks directory (`hooks`), and `ATLAS_CO_AUTHOR` for the person who started the
///   task (`pane_key` names it), whom the `commit-msg` hook credits;
/// - `PATH` with the `gh` shim first, for a crewmate that is not its own Linux user. For one that is, the spawn helper
///   pins `PATH` and the installer links the shim into its engine directory.
pub(crate) async fn agent_env(hub: &Hub, id: &str, pane_key: &str) -> Vec<(String, String)> {
    if hub.crew.store.secret_env(id).contains_key("GITHUB_TOKEN") {
        return vec![];
    }
    let Some(who) = hub.crew.github_app.identity().await else {
        return vec![];
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| "atlas".into());
    let isolated = hub.crew.store.isolated(id).is_some();
    // Where the agent reads what hostd writes for it: `crew-runtime/` when isolated.
    #[cfg(unix)]
    let hooks = {
        let runtime = match hub.crew.store.isolated(id) {
            Some(iso) => iso.runtime(),
            None => hub.crew.store.crewmate_dir(id),
        };
        match hooks::ensure(&runtime) {
            Ok(d) => Some(d.to_string_lossy().into_owned()),
            Err(e) => {
                // i18n-ignore: log
                eprintln!("atlas crew: crewmate {id}: cannot write the git hooks: {e}");
                None
            }
        }
    };
    #[cfg(not(unix))]
    let hooks: Option<String> = None;
    let co_author = pane::task_id(pane_key, id)
        .and_then(|t| hub.crew.store.task(id, t))
        .and_then(|t| t.origin)
        .and_then(|o| o.requester)
        .map(|u| env::co_author(&u.login, u.id));
    // A crewmate that is its own Linux user never gets the crew user's `gh` login as a fallback.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut out = env::git_env(
        &exe.to_string_lossy(),
        &who,
        !isolated,
        hooks.as_deref(),
        co_author.as_deref(),
    );
    #[cfg(unix)]
    if !isolated {
        match gh_shim::ensure_dir(&hub.crew.dir, &exe) {
            Ok(dir) => {
                let old = std::env::var_os("PATH").unwrap_or_default();
                let paths = std::iter::once(dir).chain(std::env::split_paths(&old));
                if let Ok(path) = std::env::join_paths(paths) {
                    out.push(("PATH".to_string(), path.to_string_lossy().into_owned()));
                }
            }
            Err(e) => {
                // i18n-ignore: log
                eprintln!("atlas crew: crewmate {id}: cannot make the gh shim: {e}");
            }
        }
    }
    out
}

/// What a task is told about commits when an app is set up: the identity is already in the environment, and the
/// `Co-authored-by:` trailer of the requester is added by the `commit-msg` hook (`hooks`).
pub(crate) fn commit_note() -> String {
    "Commit with the git identity already configured (the GitHub App's bot); never change it. \
     A hook adds the Co-authored-by trailer of the person who asked: do not write it yourself.\n"
        .to_string()
}

/// Whether crewmate `id`'s project is reached through the app, with nothing from the `gh` login: an isolated
/// crewmate (its checkout is made by its own Linux user, with a token for that one operation) whose repo an installation
/// covers. Such a task does not wait for a `gh` login.
pub(crate) async fn reaches_project(hub: &Hub, id: &str) -> bool {
    if hub.crew.store.isolated(id).is_none() {
        return false;
    }
    match project_repo(hub, id) {
        Some(repo) => hub.crew.github_app.covers(&repo).await,
        None => false,
    }
}

/// `git` with an installation token for the crewmate's project repo, for the commands that talk to the network
/// (clone, fetch). The token lives only in that one git process's environment, in the spawn helper's cleared
/// environment, and expires within the hour; the crewmate's agent never receives it. A crewmate with a token of its
/// own, or a repo no installation covers, keeps what it had (its own token, or hostd's mirror).
pub(crate) async fn with_project_token(
    hub: &Hub,
    id: &str,
    git: crate::crew::gitrun::Git,
) -> crate::crew::gitrun::Git {
    if git.has_own_token() || !reaches_project(hub, id).await {
        return git;
    }
    let Some(repo) = project_repo(hub, id) else {
        return git;
    };
    match hub.crew.github_app.token_for(&repo).await {
        Ok(t) => git.with_token(Some(t.value)),
        Err(_) => git,
    }
}

/// A checkout made from hostd's mirror (before the app) reads and pushes through that mirror: with an installation
/// token the crewmate talks to the project itself, so `origin` is put back on the real URL, push URL included.
/// Nothing happens for a crewmate that does not get an app token, or whose checkout is not there yet (the clone
/// already used the real URL).
pub(crate) async fn use_project_origin(
    hub: &Hub,
    id: &str,
    git: &crate::crew::gitrun::Git,
    work_dir: &std::path::Path,
) {
    let own = hub.crew.store.secret_env(id).contains_key("GITHUB_TOKEN");
    if own || !git.has_own_token() {
        return;
    }
    let Some(repo) = project_repo(hub, id) else {
        return;
    };
    let url = format!("https://github.com/{repo}.git");
    for args in [
        vec!["remote", "set-url", "origin", url.as_str()],
        vec!["config", "--unset", "remote.origin.pushurl"],
    ] {
        let _ = git.run(Some(work_dir), &args).await;
    }
}
