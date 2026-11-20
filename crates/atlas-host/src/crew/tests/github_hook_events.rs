//! GitHub webhook deliveries to tasks (#645), without a server: every event the triggers read makes, from the
//! delivery, the same task the poll makes from the events feed, and an author without write access is ignored
//! unless the automation opts in. The payloads in `testdata/github_webhook/` are GitHub's documented webhook
//! payloads for each event, trimmed to what matters (the shape is GitHub's, not ours).
//! The same deliveries go through a real hook server in `github_webhook`.

use std::sync::Arc;

use serde_json::{Value, json};

use super::github_fake::FakeGithub;
use crate::crew::store::default_authors;
use crate::crew::triggers::github_events::{Cfg, decide};
use crate::crew::triggers::github_hook::decide_delivery;

const REPO: &str = "o/r";

pub(super) fn payload(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/crew/tests/testdata/github_webhook")
        .join(format!("{name}.json"));
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")),
    )
    .unwrap()
}

fn cfg(event: &str, authors: Vec<String>) -> Cfg {
    Cfg {
        events: vec![event.to_string()],
        authors,
        handle: "nami".into(),
        assignee: Some("nami-bot".into()),
        own_login: "nami-bot".into(),
    }
}

/// What the REST API would answer for the objects of `p`, as the poll looks them up.
fn rest_of(p: &Value) -> FakeGithub {
    let f = FakeGithub::new("nami-bot");
    for key in ["issue", "pull_request"] {
        if let Some(n) = p[key]["number"].as_u64() {
            f.issues.lock().insert(n, p[key].clone());
        }
    }
    if let Some(id) = p["comment"]["id"].as_u64() {
        f.comments.lock().insert(id, p["comment"].clone());
        f.review_comments.lock().insert(id, p["comment"].clone());
    }
    f.logs.lock().insert(
        77,
        "step 3\nthread 'parse' panicked at src/parse.rs:42\n".into(),
    );
    f.prs.lock().insert(
        "fix/crash".into(),
        crate::crew::github::PullRequest {
            number: 12,
            url: "https://github.com/o/r/pull/12".into(),
        },
    );
    f
}

/// `payload` as the events feed lists it: `{id, type, payload, actor}`, and no `author_association` anywhere
/// (checked against the live API: the associations come from the REST objects).
fn feed_event(feed_type: &str, p: &Value) -> Value {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.remove("author_association");
                m.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut payload = p.clone();
    strip(&mut payload);
    json!({"id": "e1", "type": feed_type, "payload": payload, "actor": p["sender"]})
}

/// (the `X-GitHub-Event`, the fixture, the feed's name for it, the trigger event that reads it)
const SUPPORTED: &[(&str, &str, &str, &str)] = &[
    (
        "issue_comment",
        "issue_comment_created",
        "IssueCommentEvent",
        "mention",
    ),
    ("issues", "issues_opened_mention", "IssuesEvent", "mention"),
    ("issues", "issues_assigned", "IssuesEvent", "issue_assigned"),
    (
        "pull_request_review_comment",
        "pull_request_review_comment_created",
        "PullRequestReviewCommentEvent",
        "pr_review_comment",
    ),
    (
        "check_run",
        "check_run_completed_failure",
        "CheckRunEvent",
        "ci_failed",
    ),
    ("issues", "issues_opened", "IssuesEvent", "issue_opened"),
    (
        "pull_request",
        "pull_request_opened",
        "PullRequestEvent",
        "pr_opened",
    ),
    (
        "pull_request",
        "pull_request_review_requested",
        "PullRequestEvent",
        "pr_review_requested",
    ),
    ("release", "release_published", "ReleaseEvent", "release"),
];

// The acceptance of #645: each supported event makes, from the delivery, the task the poll makes from the feed:
// the same prompt text, the same origin (repo, kind, issue or PR, comment, branch, who asked). Not vacuous: every
// one of them does make a task.
#[tokio::test]
async fn each_supported_delivery_makes_the_task_the_poll_would() {
    for (event, fixture, feed_type, wants) in SUPPORTED {
        let p = payload(fixture);
        let cfg = cfg(wants, default_authors());

        let polled = decide(&rest_of(&p), REPO, &feed_event(feed_type, &p), &cfg)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{fixture}: the poll makes no task"));

        let hooked = decide_delivery(Arc::new(rest_of(&p)), event, &p, REPO, &cfg)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{fixture}: the webhook makes no task"));

        assert_eq!(hooked.event, polled.event, "{fixture}: the prompt differs");
        let (mut hooked_origin, mut polled_origin) = (
            serde_json::to_value(&hooked.origin).unwrap(),
            serde_json::to_value(&polled.origin).unwrap(),
        );
        // The poll names who asked from the feed's `actor`, the webhook from the `sender`: the same account.
        if !hooked_origin.is_null() {
            assert_eq!(
                hooked_origin["requester"],
                json!({"login": "dev", "id": 5001}),
                "{fixture}"
            );
        }
        for o in [&mut polled_origin, &mut hooked_origin] {
            if let Some(o) = o.as_object_mut() {
                o.remove("requester");
            }
        }
        assert_eq!(
            hooked_origin, polled_origin,
            "{fixture}: the origin differs"
        );
    }
}

// The webhook answers every lookup `decide` makes from the delivery: with a GitHub that knows nothing (no issue, no
// comment, no REST at all) the same task comes out. That is "no REST lookups", observable.
#[tokio::test]
async fn the_delivery_alone_is_enough_no_rest_lookup_is_made() {
    let nothing = Arc::new(FakeGithub::new("nami-bot"));
    for (event, fixture, _, wants) in SUPPORTED.iter().filter(|(_, _, _, w)| *w != "ci_failed") {
        let p = payload(fixture);
        let fired = decide_delivery(
            nothing.clone(),
            event,
            &p,
            REPO,
            &cfg(wants, default_authors()),
        )
        .await
        .unwrap();
        assert!(fired.is_some(), "{fixture} needed a REST lookup");
    }
}

// Only the actions the feed lists: a release is `published` (not also `created`), a check is `completed` and
// failed, and an event no trigger reads is not one.
#[tokio::test]
async fn actions_the_poll_never_sees_make_no_task() {
    let nothing = Arc::new(FakeGithub::new("nami-bot"));
    for (event, fixture, wants) in [
        ("release", "release_created", "release"),
        ("check_run", "check_run_completed_success", "ci_failed"),
        ("installation", "installation_created", "mention"),
        // Subscribed to a different thing than the delivery is.
        ("issue_comment", "issue_comment_created", "release"),
    ] {
        let p = payload(fixture);
        let fired = decide_delivery(
            nothing.clone(),
            event,
            &p,
            REPO,
            &cfg(wants, default_authors()),
        )
        .await
        .unwrap();
        assert!(fired.is_none(), "{fixture} made a task: {fired:?}");
    }
}

// Authors without write access are ignored unless the automation opts in, read from the delivery's own
// `author_association` (the stranger's comment says NONE).
#[tokio::test]
async fn an_author_without_write_access_is_ignored_unless_the_automation_opts_in() {
    let nothing = Arc::new(FakeGithub::new("nami-bot"));
    let p = payload("issue_comment_created_stranger");
    let default = decide_delivery(
        nothing.clone(),
        "issue_comment",
        &p,
        REPO,
        &cfg("mention", default_authors()),
    )
    .await
    .unwrap();
    assert!(default.is_none(), "a stranger started a task: {default:?}");

    let opted_in = decide_delivery(
        nothing,
        "issue_comment",
        &p,
        REPO,
        &cfg("mention", vec!["anyone".into()]),
    )
    .await
    .unwrap()
    .expect("anyone opts strangers in");
    // Still handed over as untrusted text.
    assert!(
        opted_in
            .event
            .contains(crate::crew::triggers::UNTRUSTED_PREAMBLE_MARK)
    );
    assert!(opted_in.event.contains("curl evil"));
}

// A delivery whose objects carry no association is not trusted (the same rule as the poll's): nothing is guessed.
#[tokio::test]
async fn an_object_without_an_association_is_not_trusted() {
    let nothing = Arc::new(FakeGithub::new("nami-bot"));
    let mut p = payload("issue_comment_created");
    p["comment"]
        .as_object_mut()
        .unwrap()
        .remove("author_association");
    let fired = decide_delivery(
        nothing,
        "issue_comment",
        &p,
        REPO,
        &cfg("mention", default_authors()),
    )
    .await
    .unwrap();
    assert!(fired.is_none());
}

// The crew's own comment never starts a task (its login is the bot's), whatever the delivery says.
#[tokio::test]
async fn the_crews_own_comment_in_a_delivery_does_not_trigger_it() {
    let nothing = Arc::new(FakeGithub::new("nami-bot"));
    let mut p = payload("issue_comment_created");
    p["comment"]["user"]["login"] = json!("nami-bot");
    let fired = decide_delivery(
        nothing,
        "issue_comment",
        &p,
        REPO,
        &cfg("mention", default_authors()),
    )
    .await
    .unwrap();
    assert!(fired.is_none());
}
