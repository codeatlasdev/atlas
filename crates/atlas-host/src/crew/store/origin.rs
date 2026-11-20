//! Where a task came from on GitHub, as structured fields on the task (never parsed back out of its
//! prompt): the runner replies to the issue/PR/comment it names, and clients can link to it.

use serde::{Deserialize, Serialize};

/// What kind of GitHub thing started the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GithubOriginKind {
    /// `@handle` in an issue/PR comment or an issue body.
    Mention,
    /// The issue was assigned to the trigger's `assignee`.
    IssueAssigned,
    /// A failed check run; `number` is the PR of its branch when it has one.
    CiFailed,
    /// A review comment on a PR's diff.
    PrReviewComment,
}

/// The issue, PR or comment a task answers. Serialized on the task as `origin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubOrigin {
    /// `owner/repo`.
    pub repo: String,
    pub kind: GithubOriginKind,
    /// Issue or PR number the reply goes to. Absent for a CI failure on a branch without a PR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    /// The comment that triggered it (mention, review comment), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<u64>,
    /// The branch the work belongs to (a PR's head, a failed run's branch): where a PR is looked up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The progress comment the crewmate posted for this task, once it exists (so a restart keeps
    /// editing it instead of posting a second one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_comment_id: Option<u64>,
    /// Who set the event off (the feed's `actor`): credited with a `Co-authored-by:` trailer on the commits the
    /// crewmate makes as the GitHub App's bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester: Option<GithubUser>,
}

/// A GitHub account: the login and the numeric id its noreply email is built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubUser {
    pub login: String,
    pub id: u64,
}
