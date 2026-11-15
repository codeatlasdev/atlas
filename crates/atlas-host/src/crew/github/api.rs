//! Every call the crew makes to GitHub after login, behind one trait: the `gh` CLI as the crew user
//! on a real server ([`GhCli`]), a fake in tests (no network). Triggers read through it (events,
//! the real `author_association` of an issue or comment, a failed job's log) and the runner writes
//! through it (the progress comment, the PR of a branch).

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

pub(crate) type Call<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// A pull request found for a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PullRequest {
    pub number: u64,
    pub url: String,
}

pub(crate) trait GithubApi: Send + Sync {
    /// The login the crew acts as (`gh api user`).
    fn login(&self) -> Call<'_, String>;
    /// The repo's recent events, newest first (`repos/<repo>/events`).
    fn events(&self, repo: &str) -> Call<'_, Vec<Value>>;
    /// The REST object of an issue or PR (`repos/<repo>/issues/<n>`): unlike the events feed it has
    /// `author_association`.
    fn issue(&self, repo: &str, number: u64) -> Call<'_, Value>;
    /// The REST object of an issue comment (`repos/<repo>/issues/comments/<id>`), with its
    /// `author_association`.
    fn issue_comment(&self, repo: &str, id: u64) -> Call<'_, Value>;
    /// The REST object of a PR review comment (`repos/<repo>/pulls/comments/<id>`).
    fn review_comment(&self, repo: &str, id: u64) -> Call<'_, Value>;
    /// The output of `gh run view <run> --log-failed`, whole (the caller caps it).
    fn failed_log(&self, repo: &str, run_id: u64) -> Call<'_, String>;
    /// The open PR whose head is `branch`, if any (`gh pr list --head`).
    fn pr_for_branch(&self, repo: &str, branch: &str) -> Call<'_, Option<PullRequest>>;
    /// Post a comment on an issue or PR; the new comment's id.
    fn create_comment(&self, repo: &str, number: u64, body: &str) -> Call<'_, u64>;
    /// Replace the body of one of our comments.
    fn edit_comment(&self, repo: &str, comment_id: u64, body: &str) -> Call<'_, ()>;
}

/// The `gh` CLI, signed in as the crew user.
pub(crate) struct GhCli;

async fn gh(args: &[&str]) -> Result<String, String> {
    let out = crate::crew::proc::gh_output(args, "gh").await?;
    if !out.status.success() {
        return Err(format!(
            "gh {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn gh_json(args: &[&str]) -> Result<Value, String> {
    let text = gh(args).await?;
    serde_json::from_str(&text).map_err(|e| format!("gh {}: not JSON: {e}", args.join(" ")))
}

impl GithubApi for GhCli {
    fn login(&self) -> Call<'_, String> {
        Box::pin(async {
            let v = gh(&["api", "user", "--jq", ".login"]).await?;
            Ok(v.trim().to_string())
        })
    }

    fn events(&self, repo: &str) -> Call<'_, Vec<Value>> {
        let path = format!("repos/{repo}/events");
        Box::pin(async move {
            // `--jq '.[]'` prints one JSON object per line.
            let text = gh(&["api", &path, "--jq", ".[]"]).await?;
            Ok(text
                .lines()
                .filter_map(|l| serde_json::from_str(l.trim()).ok())
                .collect())
        })
    }

    fn issue(&self, repo: &str, number: u64) -> Call<'_, Value> {
        let path = format!("repos/{repo}/issues/{number}");
        Box::pin(async move { gh_json(&["api", &path]).await })
    }

    fn issue_comment(&self, repo: &str, id: u64) -> Call<'_, Value> {
        let path = format!("repos/{repo}/issues/comments/{id}");
        Box::pin(async move { gh_json(&["api", &path]).await })
    }

    fn review_comment(&self, repo: &str, id: u64) -> Call<'_, Value> {
        let path = format!("repos/{repo}/pulls/comments/{id}");
        Box::pin(async move { gh_json(&["api", &path]).await })
    }

    fn failed_log(&self, repo: &str, run_id: u64) -> Call<'_, String> {
        let (repo, run) = (repo.to_string(), run_id.to_string());
        Box::pin(async move { gh(&["run", "view", &run, "--log-failed", "-R", &repo]).await })
    }

    fn pr_for_branch(&self, repo: &str, branch: &str) -> Call<'_, Option<PullRequest>> {
        let (repo, branch) = (repo.to_string(), branch.to_string());
        Box::pin(async move {
            let v = gh_json(&[
                "pr",
                "list",
                "-R",
                &repo,
                "--head",
                &branch,
                "--state",
                "open",
                "--json",
                "number,url",
                "--limit",
                "1",
            ])
            .await?;
            Ok(v.as_array().and_then(|a| a.first()).and_then(|pr| {
                Some(PullRequest {
                    number: pr["number"].as_u64()?,
                    url: pr["url"].as_str()?.to_string(),
                })
            }))
        })
    }

    fn create_comment(&self, repo: &str, number: u64, body: &str) -> Call<'_, u64> {
        let path = format!("repos/{repo}/issues/{number}/comments");
        let body = format!("body={body}");
        Box::pin(async move {
            let v = gh_json(&["api", "-X", "POST", &path, "-f", &body]).await?;
            v["id"]
                .as_u64()
                .ok_or_else(|| "gh api: comment reply has no id".to_string())
        })
    }

    fn edit_comment(&self, repo: &str, comment_id: u64, body: &str) -> Call<'_, ()> {
        let path = format!("repos/{repo}/issues/comments/{comment_id}");
        let body = format!("body={body}");
        Box::pin(async move {
            gh(&["api", "-X", "PATCH", &path, "-f", &body])
                .await
                .map(|_| ())
        })
    }
}
