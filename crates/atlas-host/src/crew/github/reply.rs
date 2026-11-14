//! The crewmate answering on GitHub: one comment on the issue or PR it was asked on, posted when the
//! task starts, edited as steps arrive (at most once per [`PROGRESS_INTERVAL`]) and edited a last
//! time with the result and the PR link, or the error. Best effort: GitHub being down never fails
//! or stalls the task, it only costs the comment.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use super::GithubApi;
use crate::crew::store::{GithubOrigin, Step};

/// Hidden in every comment we write (an HTML comment, invisible on GitHub). Comments carrying it are
/// ours, whoever's login they show, and never start a task.
pub(crate) const REPLY_MARKER: &str = "<!-- atlas-crew -->";

/// The least time between two progress edits of the same comment.
pub(crate) const PROGRESS_INTERVAL: Duration = Duration::from_secs(30);

/// GitHub refuses a comment over 65 536 characters; the result is cut well under it.
const MAX_RESULT_CHARS: usize = 20_000;
/// The checklist shows the latest steps only.
const MAX_STEPS: usize = 15;
const MAX_STEP_CHARS: usize = 100;

pub(crate) struct Reporter {
    api: Arc<dyn GithubApi>,
    origin: GithubOrigin,
    comment_id: Option<u64>,
    last_edit: Option<Instant>,
}

impl Reporter {
    pub(crate) fn new(api: Arc<dyn GithubApi>, origin: GithubOrigin) -> Self {
        Self {
            comment_id: origin.reply_comment_id,
            api,
            origin,
            last_edit: None,
        }
    }

    /// The branch the agent worked on, to find its PR at the end. The branch the origin already
    /// names (a PR's own head, a failed run's branch) wins: it is where the work was asked for.
    pub(crate) fn set_branch(&mut self, branch: Option<String>) {
        if self.origin.branch.is_none() {
            self.origin.branch = branch;
        }
    }

    /// Post "Working on it" (or take over the comment a previous run of this task already posted).
    /// `None` when there is nowhere to write or GitHub refused.
    pub(crate) async fn start(&mut self) -> Option<u64> {
        self.write(&render("Working on it.", &[], None)).await;
        self.last_edit = Some(Instant::now());
        self.comment_id
    }

    /// The steps so far. Edited at most once per [`PROGRESS_INTERVAL`]; a skipped call loses
    /// nothing, the next edit that goes out lists every step.
    pub(crate) async fn progress(&mut self, steps: &[Step]) {
        if self
            .last_edit
            .is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }
        self.last_edit = Some(Instant::now());
        self.write(&render("Working on it.", steps, None)).await;
    }

    /// The task is done: the result and, when the work is on a branch with a PR, its link.
    pub(crate) async fn finish_ok(&mut self, result: &str, steps: &[Step]) {
        let pr = match self.origin.branch.as_deref() {
            Some(b) => self
                .api
                .pr_for_branch(&self.origin.repo, b)
                .await
                .inspect_err(|e| eprintln!("atlas crew: github: PR lookup for {b}: {e}"))
                .ok()
                .flatten(),
            None => None,
        };
        let head = format!("Done.\n\n{}", clip(result.trim(), MAX_RESULT_CHARS));
        self.write(&render(&head, steps, pr.as_ref().map(|p| p.url.as_str())))
            .await;
    }

    /// The task failed.
    pub(crate) async fn finish_err(&mut self, error: &str) {
        let head = format!("Failed: {}", clip(error.trim(), MAX_RESULT_CHARS));
        self.write(&render(&head, &[], None)).await;
    }

    /// Edit our comment, or post it when there is none yet (the first post failed, or this is the
    /// end of a task that never got to start one).
    async fn write(&mut self, body: &str) {
        let (repo, api) = (self.origin.repo.clone(), self.api.clone());
        match self.comment_id {
            Some(id) => {
                if let Err(e) = api.edit_comment(&repo, id, body).await {
                    eprintln!("atlas crew: github: cannot edit comment {id} in {repo}: {e}");
                }
            }
            None => {
                let Some(number) = self.origin.number else {
                    return;
                };
                match api.create_comment(&repo, number, body).await {
                    Ok(id) => self.comment_id = Some(id),
                    Err(e) => {
                        eprintln!("atlas crew: github: cannot comment on {repo}#{number}: {e}")
                    }
                }
            }
        }
    }
}

fn render(head: &str, steps: &[Step], pr_url: Option<&str>) -> String {
    let mut out = format!("{REPLY_MARKER}\n{head}\n");
    if let Some(url) = pr_url {
        out.push_str(&format!("\nPull request: {url}\n"));
    }
    if !steps.is_empty() {
        out.push('\n');
        let skipped = steps.len().saturating_sub(MAX_STEPS);
        for (i, s) in steps.iter().enumerate().skip(skipped) {
            // The last step of a running task is the one in progress.
            let done = head.starts_with("Done") || i + 1 < steps.len();
            out.push_str(&format!(
                "- [{}] {}\n",
                if done { "x" } else { " " },
                plain(&s.text)
            ));
        }
    }
    out
}

/// A step as inert text: no link, image, autolink or mention, whatever the agent's tool printed.
fn plain(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        let w = word.trim_matches(|c: char| "[]()!<>".contains(c));
        let w = w.split("](").next().unwrap_or(w);
        if w.contains("://") || w.starts_with("www.") || w.starts_with('@') || w.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        // `#12` would autolink to an issue.
        out.push_str(&w.replace('#', "\u{200b}#"));
    }
    clip(&out, MAX_STEP_CHARS)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}
