//! `POST /github/webhook` (#645): GitHub's deliveries to the crew server's App, turned into the tasks the poll would
//! have made, at once instead of on the 2-minute poll.
//!
//! Order, each step before the next: the signature over the RAW body (`app::webhook`: nothing is parsed, claimed or
//! looked up before it passes; 401 and an audit line otherwise) -> the delivery id and event headers -> the JSON ->
//! the kill switch (paused: `503`, not claimed, GitHub shows a failure) -> the delivery id is claimed (a replay is
//! answered `200` and does nothing) -> `202` at once, and the work runs in a task of its own: GitHub counts a reply
//! slower than 10 s as a failure and does not retry it.
//!
//! What a delivery means is decided by [`decide`], the function the poll uses, so the same event makes the same task.
//! The payload carries the `author_association` of every object whose text reaches the prompt, so [`FromPayload`]
//! answers the lookups `decide` makes from the delivery itself: no REST call, and an object that is not in the
//! delivery is an error, never "allowed". Authors without write access are ignored unless the trigger opts in.

use std::sync::Arc;

use serde_json::{Value, json};

use super::github::actor_of;
use super::github_events::{Cfg, Fired, decide};
use crate::crew::github::app::webhook::valid_delivery_id;
use crate::crew::github::{Call, GithubApi, PullRequest};
use crate::crew::store::GithubOrigin;
use crate::server::Hub;

/// How often the hook is reconciled with GitHub's delivery list.
const RECONCILE_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Test-only: how often the reconciliation runs.
#[cfg(test)]
pub(crate) static TEST_RECONCILE: parking_lot::Mutex<Option<std::time::Duration>> =
    parking_lot::Mutex::new(None);

fn reconcile_every() -> std::time::Duration {
    #[cfg(test)]
    if let Some(d) = *TEST_RECONCILE.lock() {
        return d;
    }
    RECONCILE_EVERY
}

/// The headers of a delivery the route reads.
pub(crate) struct Headers<'a> {
    /// `X-Hub-Signature-256`
    pub signature: Option<&'a str>,
    /// `X-GitHub-Delivery`
    pub delivery: Option<&'a str>,
    /// `X-GitHub-Event`
    pub event: Option<&'a str>,
}

fn reply(status: &'static str, body: Value) -> (&'static str, String) {
    (status, body.to_string())
}

/// Answer one delivery: the status and body of the HTTP reply. See the module docs for the order.
pub(crate) async fn receive(hub: &Arc<Hub>, h: Headers<'_>, raw: &[u8]) -> (&'static str, String) {
    // Off (no public URL) or not a crew server: the route does not exist.
    if !hub.crew_host || hub.crew.store.load_crew_settings().public_url.is_none() {
        return ("404 Not Found", String::new());
    }
    let app = &hub.crew.github_app;
    if let Err(r) = app.verify_delivery(h.signature, raw) {
        return (r.status(), String::new());
    }
    // Signed by GitHub from here: what is wrong now is the request, not an intruder.
    let (Some(delivery), Some(event)) = (
        h.delivery.filter(|d| valid_delivery_id(d)),
        h.event.filter(|e| (1..=64).contains(&e.len())),
    ) else {
        return reply(
            "400 Bad Request",
            json!({"error": "X-GitHub-Delivery or X-GitHub-Event is missing or malformed"}),
        );
    };
    let Ok(payload) = serde_json::from_slice::<Value>(raw) else {
        return reply("400 Bad Request", json!({"error": "the body is not JSON"}));
    };
    // Paused: not claimed, so GitHub shows it as failed. The reconciliation does not redeliver while paused
    // either, and resumes when the crew does.
    if hub.crew.is_paused() {
        return reply(
            "503 Service Unavailable",
            json!({"error": "the crew is paused"}),
        );
    }
    match app.claim_delivery(delivery) {
        Ok(true) => {}
        Ok(false) => {
            app.audit_duplicate(delivery, event);
            return reply("200 OK", json!({"duplicate": true}));
        }
        Err(e) => {
            // i18n-ignore: log
            eprintln!("atlas crew: github webhook: cannot record delivery {delivery}: {e}");
            return reply(
                "500 Internal Server Error",
                json!({"error": "cannot record the delivery"}),
            );
        }
    }
    if let Some(installation) = payload["installation"]["id"].as_u64() {
        app.note_verified(installation);
    }
    let (hub, event) = (hub.clone(), event.to_string());
    tokio::spawn(async move { process(&hub, &event, &payload).await });
    reply("202 Accepted", json!({}))
}

/// The work of one accepted delivery: every enabled `Github` trigger on its repository decides on it.
async fn process(hub: &Arc<Hub>, event: &str, payload: &Value) {
    if feed_type(event, payload).is_none() {
        return;
    }
    let Some(repo) = payload["repository"]["full_name"]
        .as_str()
        .filter(|r| !r.is_empty())
    else {
        return;
    };
    let api = hub.crew.github();
    // Who the crew writes as: its own comments never start a task.
    let own_login = match api.login().await {
        Ok(l) => l,
        Err(e) => {
            // i18n-ignore: log
            eprintln!("atlas crew: github webhook: cannot tell the crew's own GitHub login: {e}");
            return;
        }
    };
    for crewmate in hub.crew.store.list_crewmates() {
        for trigger in crewmate.triggers.iter().filter(|t| t.enabled) {
            let Some((watched, cfg)) = Cfg::for_trigger(&crewmate, trigger, own_login.clone())
            else {
                continue;
            };
            if !watched.eq_ignore_ascii_case(repo) {
                continue;
            }
            match decide_delivery(api.clone(), event, payload, &watched, &cfg).await {
                Ok(Some(fired)) => super::fire(
                    hub,
                    &crewmate.id,
                    &trigger.id,
                    &trigger.prompt,
                    super::Event::Github {
                        text: &fired.event,
                        origin: fired.origin,
                    },
                ),
                Ok(None) => {}
                Err(e) => {
                    // i18n-ignore: log
                    eprintln!(
                        "atlas crew: github webhook: {event} in {repo} for crewmate {}: {e}",
                        crewmate.id
                    );
                }
            }
        }
    }
}

/// The events feed's name for a webhook event, `None` for one no trigger reads. Only the actions the feed lists too:
/// a release is `published` (not also `created` and `released`), a check is `completed`.
fn feed_type(event: &str, payload: &Value) -> Option<&'static str> {
    let action = payload["action"].as_str().unwrap_or("");
    Some(match event {
        "issue_comment" => "IssueCommentEvent",
        "issues" => "IssuesEvent",
        "pull_request" => "PullRequestEvent",
        "pull_request_review_comment" => "PullRequestReviewCommentEvent",
        "check_run" if action == "completed" => "CheckRunEvent",
        "release" if action == "published" => "ReleaseEvent",
        _ => return None,
    })
}

/// What one delivery means to one trigger's settings: the same [`decide`] the poll runs, on the delivery as the feed
/// would have listed it, with the objects the lookups ask for taken from the delivery.
pub(crate) async fn decide_delivery(
    inner: Arc<dyn GithubApi>,
    event: &str,
    payload: &Value,
    repo: &str,
    cfg: &Cfg,
) -> Result<Option<Fired>, String> {
    let Some(feed) = feed_type(event, payload) else {
        return Ok(None);
    };
    // The sender is the feed's `actor`.
    let ev = json!({"type": feed, "payload": payload, "actor": payload["sender"]});
    let api = FromPayload {
        payload: payload.clone(),
        inner,
    };
    let mut fired = decide(&api, repo, &ev, cfg).await?;
    if let Some(Fired {
        origin: Some(GithubOrigin { requester, .. }),
        ..
    }) = &mut fired
    {
        *requester = actor_of(&ev);
    }
    Ok(fired)
}

/// A [`GithubApi`] whose reads of issues, PRs and comments are answered by the delivery itself (they carry
/// `author_association`), and everything else (who the crew is, a failed job's log, the PR of a branch) by `inner`.
struct FromPayload {
    payload: Value,
    inner: Arc<dyn GithubApi>,
}

impl FromPayload {
    fn find(&self, keys: &[&str], what: &str, id: u64, by: &str) -> Call<'static, Value> {
        let found = keys
            .iter()
            .map(|k| &self.payload[*k])
            .find(|o| o[by].as_u64() == Some(id))
            .cloned();
        let what = what.to_string();
        Box::pin(async move { found.ok_or(format!("{what} {id} is not in the delivery")) })
    }
}

impl GithubApi for FromPayload {
    fn login(&self) -> Call<'_, String> {
        self.inner.login()
    }
    fn events(&self, repo: &str) -> Call<'_, Vec<Value>> {
        self.inner.events(repo)
    }
    fn issue(&self, _repo: &str, number: u64) -> Call<'_, Value> {
        self.find(&["issue", "pull_request"], "issue or PR", number, "number")
    }
    fn issue_comment(&self, _repo: &str, id: u64) -> Call<'_, Value> {
        self.find(&["comment"], "comment", id, "id")
    }
    fn review_comment(&self, _repo: &str, id: u64) -> Call<'_, Value> {
        self.find(&["comment"], "review comment", id, "id")
    }
    fn failed_log(&self, repo: &str, run_id: u64) -> Call<'_, String> {
        self.inner.failed_log(repo, run_id)
    }
    fn pr_for_branch(&self, repo: &str, branch: &str) -> Call<'_, Option<PullRequest>> {
        self.inner.pr_for_branch(repo, branch)
    }
    fn create_comment(&self, repo: &str, number: u64, body: &str) -> Call<'_, u64> {
        self.inner.create_comment(repo, number, body)
    }
    fn edit_comment(&self, repo: &str, comment_id: u64, body: &str) -> Call<'_, ()> {
        self.inner.edit_comment(repo, comment_id, body)
    }
}

/// The reconciliation loop: tell GitHub the URL when it was not told yet, and redeliver what failed
/// (`AppService::reconcile`). Not while the crew is paused: a redelivery then would be refused and spend an attempt.
pub(super) async fn reconcile_loop(hub_weak: std::sync::Weak<Hub>) {
    loop {
        tokio::time::sleep(reconcile_every()).await;
        let Some(hub) = hub_weak.upgrade() else { break };
        if hub.crew.is_paused() {
            continue;
        }
        let url = hub.crew.store.load_crew_settings().public_url;
        if let Err(e) = hub.crew.github_app.reconcile(url.as_deref()).await {
            // i18n-ignore: log
            eprintln!("atlas crew: github webhook: reconciliation: {e}");
        }
    }
}
