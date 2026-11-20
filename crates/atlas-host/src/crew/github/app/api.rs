//! [`GithubApi`] through the GitHub App: the second implementation, next to the `gh` CLI ([`GhCli`]).
//!
//! [`Routed`] is the one place that chooses, per repository: an app installation that covers the repo answers with
//! its installation token (so what the crew writes comes from `<slug>[bot]`); any other repo goes to `gh`, and says
//! so once in the audit. A failure to ask GitHub is an error, never a quiet switch of identity to the `gh` login.

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;
use reqwest::Method;
use serde_json::{Value, json};

use super::http::{Auth, Http};
use super::service::{AppService, TokenError, valid_repo};
use crate::crew::audit::Audit;
use crate::crew::github::{Call, GhCli, GithubApi, PullRequest};

pub(crate) struct Routed {
    app: Arc<AppService>,
    http: Http,
    audit: Arc<Audit>,
    /// What answers outside the app: the `gh` CLI (a fake under test).
    cli: Arc<dyn GithubApi>,
    /// Repos already reported as "not covered": one audit line each, not one per poll.
    said: Mutex<HashSet<String>>,
}

/// Who answers a call for one repo.
enum Via {
    App(String),
    Cli,
}

impl Routed {
    pub(crate) fn new(app: Arc<AppService>, audit: Arc<Audit>) -> Self {
        Self::with(app, audit, Http::default(), Arc::new(GhCli))
    }

    pub(crate) fn with(
        app: Arc<AppService>,
        audit: Arc<Audit>,
        http: Http,
        cli: Arc<dyn GithubApi>,
    ) -> Self {
        Self {
            app,
            http,
            audit,
            cli,
            said: Mutex::new(HashSet::new()),
        }
    }

    async fn via(&self, repo: &str) -> Result<Via, String> {
        match self.app.token_for(repo).await {
            Ok(t) => Ok(Via::App(t.value)),
            // No app: `gh`, as before the app existed.
            Err(TokenError::NoApp) => Ok(Via::Cli),
            Err(TokenError::NotCovered(r)) => {
                if self.said.lock().insert(r.to_ascii_lowercase()) {
                    self.audit
                        .record(None, "github_app_fallback", json!({"repo": r, "via": "gh"}));
                }
                Ok(Via::Cli)
            }
            Err(e @ TokenError::Failed(_)) => Err(e.to_string()),
        }
    }

    async fn get(&self, repo: &str, path: &str) -> Result<Option<Value>, String> {
        match self.via(repo).await? {
            Via::App(t) => self
                .http
                .json(Method::GET, path, Auth::Token(&t), None)
                .await
                .map(Some)
                .map_err(|e| e.to_string()),
            Via::Cli => Ok(None),
        }
    }
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

impl GithubApi for Routed {
    /// Who the crew writes as: the bot when an app is set up (its comments are ours), else the `gh` login.
    fn login(&self) -> Call<'_, String> {
        Box::pin(async move {
            match self.app.record()? {
                Some(rec) => Ok(rec.identity().name),
                None => self.cli.login().await,
            }
        })
    }

    fn events(&self, repo: &str) -> Call<'_, Vec<Value>> {
        let repo = repo.to_string();
        Box::pin(async move {
            match self.get(&repo, &format!("repos/{repo}/events")).await? {
                Some(v) => Ok(v.as_array().cloned().unwrap_or_default()),
                None => self.cli.events(&repo).await,
            }
        })
    }

    fn issue(&self, repo: &str, number: u64) -> Call<'_, Value> {
        let repo = repo.to_string();
        Box::pin(async move {
            match self
                .get(&repo, &format!("repos/{repo}/issues/{number}"))
                .await?
            {
                Some(v) => Ok(v),
                None => self.cli.issue(&repo, number).await,
            }
        })
    }

    fn issue_comment(&self, repo: &str, id: u64) -> Call<'_, Value> {
        let repo = repo.to_string();
        Box::pin(async move {
            match self
                .get(&repo, &format!("repos/{repo}/issues/comments/{id}"))
                .await?
            {
                Some(v) => Ok(v),
                None => self.cli.issue_comment(&repo, id).await,
            }
        })
    }

    fn review_comment(&self, repo: &str, id: u64) -> Call<'_, Value> {
        let repo = repo.to_string();
        Box::pin(async move {
            match self
                .get(&repo, &format!("repos/{repo}/pulls/comments/{id}"))
                .await?
            {
                Some(v) => Ok(v),
                None => self.cli.review_comment(&repo, id).await,
            }
        })
    }

    /// What `gh run view --log-failed` prints, from the REST API: the log of each failed job of the run.
    fn failed_log(&self, repo: &str, run_id: u64) -> Call<'_, String> {
        let repo = repo.to_string();
        Box::pin(async move {
            let Via::App(t) = self.via(&repo).await? else {
                return self.cli.failed_log(&repo, run_id).await;
            };
            let jobs = self
                .http
                .json(
                    Method::GET,
                    &format!("repos/{repo}/actions/runs/{run_id}/jobs?filter=latest&per_page=100"),
                    Auth::Token(&t),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            let mut out = String::new();
            for job in jobs["jobs"].as_array().into_iter().flatten() {
                if job["conclusion"] != "failure" {
                    continue;
                }
                let Some(id) = job["id"].as_u64() else {
                    continue;
                };
                let log = self
                    .http
                    .text(
                        &format!("repos/{repo}/actions/jobs/{id}/logs"),
                        Auth::Token(&t),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                out.push_str(&format!(
                    "== {} ==\n{log}\n",
                    job["name"].as_str().unwrap_or("job")
                ));
            }
            Ok(out)
        })
    }

    fn pr_for_branch(&self, repo: &str, branch: &str) -> Call<'_, Option<PullRequest>> {
        let (repo, branch) = (repo.to_string(), branch.to_string());
        Box::pin(async move {
            let Some((owner, _)) = valid_repo(&repo) else {
                return self.cli.pr_for_branch(&repo, &branch).await;
            };
            let head = enc(&format!("{owner}:{branch}"));
            let path = format!("repos/{repo}/pulls?head={head}&state=open&per_page=1");
            match self.get(&repo, &path).await? {
                Some(v) => Ok(v.as_array().and_then(|a| a.first()).and_then(|pr| {
                    Some(PullRequest {
                        number: pr["number"].as_u64()?,
                        url: pr["html_url"].as_str()?.to_string(),
                    })
                })),
                None => self.cli.pr_for_branch(&repo, &branch).await,
            }
        })
    }

    fn create_comment(&self, repo: &str, number: u64, body: &str) -> Call<'_, u64> {
        let (repo, body) = (repo.to_string(), body.to_string());
        Box::pin(async move {
            let Via::App(t) = self.via(&repo).await? else {
                return self.cli.create_comment(&repo, number, &body).await;
            };
            let v = self
                .http
                .json(
                    Method::POST,
                    &format!("repos/{repo}/issues/{number}/comments"),
                    Auth::Token(&t),
                    Some(&json!({"body": body})),
                )
                .await
                .map_err(|e| e.to_string())?;
            v["id"]
                .as_u64()
                .ok_or_else(|| "GitHub: comment reply has no id".to_string())
        })
    }

    fn edit_comment(&self, repo: &str, comment_id: u64, body: &str) -> Call<'_, ()> {
        let (repo, body) = (repo.to_string(), body.to_string());
        Box::pin(async move {
            let Via::App(t) = self.via(&repo).await? else {
                return self.cli.edit_comment(&repo, comment_id, &body).await;
            };
            self.http
                .json(
                    Method::PATCH,
                    &format!("repos/{repo}/issues/comments/{comment_id}"),
                    Auth::Token(&t),
                    Some(&json!({"body": body})),
                )
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }
}
