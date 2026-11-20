//! A `GithubApi` that never touches the network: canned REST objects in, every write recorded.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde_json::Value;

use crate::crew::github::{Call, GithubApi, PullRequest};

/// One write the crew made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Write {
    Create { number: u64, body: String },
    Edit { comment_id: u64, body: String },
}

#[derive(Default)]
pub(crate) struct FakeGithub {
    pub login: Mutex<String>,
    pub events: Mutex<Vec<Value>>,
    /// The repo of every `events` call, in order: what the poll asked for.
    pub event_reads: Mutex<Vec<String>>,
    /// `issues/<n>` by number.
    pub issues: Mutex<HashMap<u64, Value>>,
    /// `issues/comments/<id>` by id.
    pub comments: Mutex<HashMap<u64, Value>>,
    /// `pulls/comments/<id>` by id.
    pub review_comments: Mutex<HashMap<u64, Value>>,
    pub logs: Mutex<HashMap<u64, String>>,
    pub prs: Mutex<HashMap<String, PullRequest>>,
    pub writes: Mutex<Vec<Write>>,
    /// Fail every create/edit with this message.
    pub write_error: Mutex<Option<String>>,
    next_comment: Mutex<u64>,
}

impl FakeGithub {
    pub(crate) fn new(login: &str) -> Self {
        let f = Self::default();
        *f.login.lock() = login.to_string();
        *f.next_comment.lock() = 9000;
        f
    }

    pub(crate) fn writes(&self) -> Vec<Write> {
        self.writes.lock().clone()
    }

    /// How many times the poll read `repo`'s events. Only the Unix-only `github_webhook` tests ask.
    #[cfg(unix)]
    pub(crate) fn reads_of(&self, repo: &str) -> usize {
        self.event_reads
            .lock()
            .iter()
            .filter(|r| *r == repo)
            .count()
    }
}

fn found(v: Option<Value>, what: String) -> Result<Value, String> {
    v.ok_or(format!("404 {what}"))
}

impl GithubApi for FakeGithub {
    fn login(&self) -> Call<'_, String> {
        let l = self.login.lock().clone();
        Box::pin(async move { Ok(l) })
    }
    fn events(&self, repo: &str) -> Call<'_, Vec<Value>> {
        self.event_reads.lock().push(repo.to_string());
        let e = self.events.lock().clone();
        Box::pin(async move { Ok(e) })
    }
    fn issue(&self, _repo: &str, number: u64) -> Call<'_, Value> {
        let v = found(
            self.issues.lock().get(&number).cloned(),
            format!("issue {number}"),
        );
        Box::pin(async move { v })
    }
    fn issue_comment(&self, _repo: &str, id: u64) -> Call<'_, Value> {
        let v = found(
            self.comments.lock().get(&id).cloned(),
            format!("comment {id}"),
        );
        Box::pin(async move { v })
    }
    fn review_comment(&self, _repo: &str, id: u64) -> Call<'_, Value> {
        let v = found(
            self.review_comments.lock().get(&id).cloned(),
            format!("review comment {id}"),
        );
        Box::pin(async move { v })
    }
    fn failed_log(&self, _repo: &str, run_id: u64) -> Call<'_, String> {
        let v = self
            .logs
            .lock()
            .get(&run_id)
            .cloned()
            .ok_or(format!("no log {run_id}"));
        Box::pin(async move { v })
    }
    fn pr_for_branch(&self, _repo: &str, branch: &str) -> Call<'_, Option<PullRequest>> {
        let v = self.prs.lock().get(branch).cloned();
        Box::pin(async move { Ok(v) })
    }
    fn create_comment(&self, _repo: &str, number: u64, body: &str) -> Call<'_, u64> {
        let r = match self.write_error.lock().clone() {
            Some(e) => Err(e),
            None => {
                let mut n = self.next_comment.lock();
                *n += 1;
                self.writes.lock().push(Write::Create {
                    number,
                    body: body.to_string(),
                });
                Ok(*n)
            }
        };
        Box::pin(async move { r })
    }
    fn edit_comment(&self, _repo: &str, comment_id: u64, body: &str) -> Call<'_, ()> {
        let r = match self.write_error.lock().clone() {
            Some(e) => Err(e),
            None => {
                self.writes.lock().push(Write::Edit {
                    comment_id,
                    body: body.to_string(),
                });
                Ok(())
            }
        };
        Box::pin(async move { r })
    }
}
