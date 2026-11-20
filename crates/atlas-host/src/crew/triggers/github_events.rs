//! What one GitHub event means to one `Github` trigger: whether it starts a task, the `{{event}}`
//! text (all of it wrapped as untrusted) and where the crewmate replies.
//!
//! The events feed (`repos/<r>/events`) carries no `author_association` on issues, comments or PRs
//! (checked against the live API), so the author filter cannot read it there. Every writer whose
//! text reaches the prompt is looked up through the REST objects instead; a lookup that fails is an
//! error, never "allowed": the poller then leaves the event unseen and asks again.

use serde_json::Value;

use super::{author_allowed, untrusted_event};
use crate::crew::github::{GithubApi, REPLY_MARKER};
use crate::crew::store::{GithubOrigin, GithubOriginKind};

/// The log tail a failed check puts in the prompt: the end of the log, which is where the failure is.
const LOG_LINES: usize = 200;
const LOG_BYTES: usize = 32 * 1024;
/// An issue body or comment is cut here before it is quoted.
const TEXT_BYTES: usize = 16 * 1024;

/// A trigger's settings as `decide` needs them.
pub(crate) struct Cfg {
    pub events: Vec<String>,
    pub authors: Vec<String>,
    /// Without the `@`.
    pub handle: String,
    pub assignee: Option<String>,
    /// The login the crew writes as: its own comments never trigger it.
    pub own_login: String,
}

/// A task to start.
#[derive(Debug)]
pub(crate) struct Fired {
    /// The `{{event}}` text, already wrapped as untrusted.
    pub event: String,
    pub origin: Option<GithubOrigin>,
}

pub(crate) async fn decide(
    api: &dyn GithubApi,
    repo: &str,
    ev: &Value,
    cfg: &Cfg,
) -> Result<Option<Fired>, String> {
    let ev_type = ev["type"].as_str().unwrap_or("");
    let action = ev["payload"]["action"].as_str().unwrap_or("");
    let p = &ev["payload"];
    let wants = |name: &str| cfg.events.iter().any(|e| e == name);

    match (ev_type, action) {
        ("IssueCommentEvent", "created") if wants("mention") => {
            let number = p["issue"]["number"]
                .as_u64()
                .ok_or("comment without issue")?;
            let id = p["comment"]["id"].as_u64().ok_or("comment without id")?;
            let login = p["comment"]["user"]["login"].as_str().unwrap_or("");
            if login.eq_ignore_ascii_case(&cfg.own_login)
                || p["comment"]["body"].as_str().is_some_and(is_ours)
            {
                return Ok(None);
            }
            let comment = api.issue_comment(repo, id).await?;
            if is_ours_obj(&comment, cfg) || !mentions(body_of(&comment), &cfg.handle) {
                return Ok(None);
            }
            let issue = api.issue(repo, number).await?;
            if !trusted(cfg, &[&issue, &comment]) {
                return Ok(None);
            }
            let text = format!(
                "Comment on {repo}#{number} \"{}\".\nIssue body:\n{}\n\nThe comment that mentions you:\n{}",
                title_of(&issue),
                cut(body_of(&issue)),
                cut(body_of(&comment)),
            );
            Ok(Some(fired(
                text,
                repo,
                GithubOriginKind::Mention,
                Some(number),
                Some(id),
                None,
            )))
        }
        ("IssuesEvent", "opened" | "edited") if wants("mention") => {
            let number = p["issue"]["number"]
                .as_u64()
                .ok_or("issue without number")?;
            let issue = api.issue(repo, number).await?;
            if is_ours_obj(&issue, cfg)
                || !mentions(body_of(&issue), &cfg.handle)
                || !trusted(cfg, &[&issue])
            {
                return Ok(None);
            }
            let text = format!(
                "Issue {repo}#{number} \"{}\" mentions you:\n{}",
                title_of(&issue),
                cut(body_of(&issue))
            );
            Ok(Some(fired(
                text,
                repo,
                GithubOriginKind::Mention,
                Some(number),
                None,
                None,
            )))
        }
        ("IssuesEvent", "assigned") if wants("issue_assigned") => {
            let assignee = p["assignee"]["login"].as_str().unwrap_or("");
            if cfg
                .assignee
                .as_deref()
                .is_none_or(|a| !a.eq_ignore_ascii_case(assignee))
            {
                return Ok(None);
            }
            let number = p["issue"]["number"]
                .as_u64()
                .ok_or("issue without number")?;
            let issue = api.issue(repo, number).await?;
            if !trusted(cfg, &[&issue]) {
                return Ok(None);
            }
            let text = format!(
                "Issue {repo}#{number} \"{}\" was assigned to you:\n{}",
                title_of(&issue),
                cut(body_of(&issue))
            );
            Ok(Some(fired(
                text,
                repo,
                GithubOriginKind::IssueAssigned,
                Some(number),
                None,
                None,
            )))
        }
        ("PullRequestReviewCommentEvent", "created") if wants("pr_review_comment") => {
            let pr = &p["pull_request"];
            let number = pr["number"].as_u64().ok_or("review comment without PR")?;
            let id = p["comment"]["id"]
                .as_u64()
                .ok_or("review comment without id")?;
            if p["comment"]["user"]["login"]
                .as_str()
                .is_some_and(|l| l.eq_ignore_ascii_case(&cfg.own_login))
            {
                return Ok(None);
            }
            let comment = api.review_comment(repo, id).await?;
            if is_ours_obj(&comment, cfg) {
                return Ok(None);
            }
            let pr_obj = api.issue(repo, number).await?;
            if !trusted(cfg, &[&pr_obj, &comment]) {
                return Ok(None);
            }
            let line = comment["line"]
                .as_u64()
                .or(comment["original_line"].as_u64());
            let text = format!(
                "Review comment on {repo}#{number} \"{}\", file {} line {}:\n{}",
                title_of(&pr_obj),
                comment["path"].as_str().unwrap_or("?"),
                line.map_or("?".to_string(), |l| l.to_string()),
                cut(body_of(&comment)),
            );
            let branch = pr["head"]["ref"].as_str().map(String::from);
            Ok(Some(fired(
                text,
                repo,
                GithubOriginKind::PrReviewComment,
                Some(number),
                Some(id),
                branch,
            )))
        }
        ("CheckRunEvent", _)
            if wants("ci_failed") && p["check_run"]["conclusion"].as_str() == Some("failure") =>
        {
            let run = &p["check_run"];
            let branch = run["check_suite"]["head_branch"].as_str().map(String::from);
            let run_id = run_id_of(run);
            let log = match run_id {
                Some(id) => api
                    .failed_log(repo, id)
                    .await
                    .inspect_err(|e| {
                        // i18n-ignore: log
                        eprintln!("atlas crew: github: no failed log for run {id} in {repo}: {e}")
                    })
                    .ok(),
                None => None,
            };
            let pr = match branch.as_deref() {
                Some(b) => api.pr_for_branch(repo, b).await.ok().flatten(),
                None => None,
            };
            let mut text = format!(
                "Check \"{}\" failed in {repo} on branch {}{}.",
                run["name"].as_str().unwrap_or("?"),
                branch.as_deref().unwrap_or("?"),
                pr.as_ref()
                    .map_or(String::new(), |p| format!(" (PR #{})", p.number)),
            );
            match log {
                Some(l) if !l.trim().is_empty() => {
                    text.push_str(&format!("\nEnd of the failed log:\n{}", tail(&l)));
                }
                _ => text.push_str("\n(The failed log is not available.)"),
            }
            Ok(Some(fired(
                text,
                repo,
                GithubOriginKind::CiFailed,
                pr.map(|p| p.number),
                None,
                branch,
            )))
        }
        ("IssuesEvent", "opened") if wants("issue_opened") => {
            legacy(api, repo, ev_type, &p["issue"], cfg).await
        }
        ("PullRequestEvent", "opened") if wants("pr_opened") => {
            legacy(api, repo, ev_type, &p["pull_request"], cfg).await
        }
        // The feed reports a review request as a PullRequestEvent; the old type name never occurs.
        ("PullRequestEvent", "review_requested") | ("PullRequestReviewRequestedEvent", _)
            if wants("pr_review_requested") =>
        {
            legacy(api, repo, ev_type, &p["pull_request"], cfg).await
        }
        ("ReleaseEvent", _) if wants("release") => {
            let name = p["release"]["name"]
                .as_str()
                .or(p["release"]["tag_name"].as_str());
            let text = format!("{ev_type} in {repo}: {}", name.unwrap_or("(no title)"));
            Ok(Some(Fired {
                event: untrusted_event("a GitHub event", &text),
                origin: None,
            }))
        }
        _ => Ok(None),
    }
}

/// An event the trigger wants that only says "this happened to that issue or PR": the title, once
/// the real association of whoever opened it is known to be accepted.
async fn legacy(
    api: &dyn GithubApi,
    repo: &str,
    ev_type: &str,
    subject: &Value,
    cfg: &Cfg,
) -> Result<Option<Fired>, String> {
    let number = subject["number"].as_u64().ok_or("event without a number")?;
    let obj = api.issue(repo, number).await?;
    if !trusted(cfg, &[&obj]) {
        return Ok(None);
    }
    let text = format!("{ev_type} in {repo}: {}", title_of(&obj));
    Ok(Some(Fired {
        event: untrusted_event("a GitHub event", &text),
        origin: None,
    }))
}

fn fired(
    text: String,
    repo: &str,
    kind: GithubOriginKind,
    number: Option<u64>,
    comment_id: Option<u64>,
    branch: Option<String>,
) -> Fired {
    Fired {
        event: untrusted_event("a GitHub event", &text),
        origin: Some(GithubOrigin {
            repo: repo.to_string(),
            kind,
            number,
            comment_id,
            branch,
            reply_comment_id: None,
            requester: None,
        }),
    }
}

/// Every object whose text reaches the prompt must come from an accepted author. No association
/// (`null`) is not accepted: that is what an unresolved or odd object looks like.
fn trusted(cfg: &Cfg, objs: &[&Value]) -> bool {
    objs.iter().all(|o| match o["author_association"].as_str() {
        Some(a) => author_allowed(&cfg.authors, Some(a)),
        None => false,
    })
}

fn is_ours(body: &str) -> bool {
    body.contains(REPLY_MARKER)
}

fn is_ours_obj(o: &Value, cfg: &Cfg) -> bool {
    o["user"]["login"]
        .as_str()
        .is_some_and(|l| l.eq_ignore_ascii_case(&cfg.own_login))
        || is_ours(body_of(o))
}

fn body_of(o: &Value) -> &str {
    o["body"].as_str().unwrap_or("")
}

fn title_of(o: &Value) -> &str {
    o["title"].as_str().unwrap_or("(no title)")
}

fn cut(s: &str) -> &str {
    if s.len() <= TEXT_BYTES {
        return s;
    }
    let mut end = TEXT_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `@handle` as a whole word: not `@handles`, not the middle of an address (`a@handle.com`).
pub(crate) fn mentions(text: &str, handle: &str) -> bool {
    let want = format!("@{}", handle.to_ascii_lowercase());
    let lower = text.to_ascii_lowercase();
    lower.match_indices(&want).any(|(i, _)| {
        let before_ok = lower[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.'));
        let after_ok = lower[i + want.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
        before_ok && after_ok
    })
}

/// `https://github.com/o/r/actions/runs/77/job/1` -> 77.
fn run_id_of(run: &Value) -> Option<u64> {
    let url = run["html_url"].as_str()?;
    let rest = url.split("/actions/runs/").nth(1)?;
    rest.split('/').next()?.parse().ok()
}

/// The last [`LOG_LINES`] lines, within [`LOG_BYTES`], the end of the log kept.
fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let mut kept: Vec<&str> = lines[lines.len().saturating_sub(LOG_LINES)..].to_vec();
    let mut total: usize = kept.iter().map(|l| l.len() + 1).sum();
    while total > LOG_BYTES && kept.len() > 1 {
        total -= kept.remove(0).len() + 1;
    }
    // A single enormous line is cut from its start, on a char boundary.
    let last = kept.last().copied().unwrap_or("");
    if kept.len() == 1 && last.len() > LOG_BYTES {
        let mut start = last.len() - LOG_BYTES;
        while !last.is_char_boundary(start) {
            start += 1;
        }
        return last[start..].to_string();
    }
    kept.join("\n")
}

/// A GitHub login: letters, digits and single hyphens, 1 to 39 characters.
pub(crate) fn valid_handle(h: &str) -> bool {
    (1..=39).contains(&h.len()) && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The `@handle` a trigger answers to: its own, else the crewmate's name as a slug (`Code Reviewer`
/// -> `code-reviewer`).
pub(crate) fn handle_for(handle: Option<&str>, crewmate_name: &str) -> String {
    if let Some(h) = handle {
        return h.to_string();
    }
    crewmate_name
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A git ref name that is safe to put in the trusted instructions: plain characters only. The name
/// is written by whoever opened the PR, so anything that could pass for a shell command, a git
/// option or a new instruction (spaces, newlines, quotes, `$`, a leading `-`, `..`) is refused.
fn plain_ref(b: &str) -> bool {
    !b.is_empty()
        && b.len() <= 100
        && !b.starts_with(['-', '/'])
        && !b.ends_with('/')
        && !b.contains("..")
        && b.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'/' | b'-'))
}

/// The instruction that makes a CI-fix or review-comment task work on the PR's own branch: the one
/// part of such a task the crewmate's instructions (not the untrusted block) must carry. `None`
/// for any other task and for a branch name that is not a plain ref.
pub(crate) fn branch_instruction(origin: &GithubOrigin) -> Option<String> {
    if !matches!(
        origin.kind,
        GithubOriginKind::CiFailed | GithubOriginKind::PrReviewComment
    ) {
        return None;
    }
    let b = origin.branch.as_deref().filter(|b| plain_ref(b))?;
    Some(format!(
        "This task is about the branch {b} of {}. Before changing anything run `git fetch origin {b} && git checkout {b}`, \
         make the fix there, and push to that same branch.\n",
        origin.repo
    ))
}
