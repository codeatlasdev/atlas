//! GitHub in and out, end to end (issue #204): a real hostd polls a fake GitHub, a collaborator's
//! mention starts a task that carries its GitHub origin as structured fields, the crewmate posts a
//! reply comment, edits it with the result and the PR link, and the same comment never starts a
//! second task. A fake agent stands in for the engine; no network.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin};
use super::github_fake::{FakeGithub, Write};
use crate::HostClient;
use crate::crew::github::{PullRequest, REPLY_MARKER, TEST_GITHUB_API};
use crate::crew::triggers::TEST_GITHUB_POLL;

fn mention_event(event_id: &str, comment_id: u64) -> Value {
    json!({"id": event_id, "type": "IssueCommentEvent", "payload": {
        "action": "created",
        "issue": {"number": 7, "title": "Crash on start", "user": {"login": "someone"}},
        "comment": {"id": comment_id, "body": "@nami please fix it", "user": {"login": "dev"}}}})
}

fn seeded() -> Arc<FakeGithub> {
    let f = Arc::new(FakeGithub::new("nami-bot"));
    f.issues.lock().insert(
        7,
        json!({"number": 7, "title": "Crash on start", "body": "null pointer in parse()",
               "author_association": "MEMBER", "user": {"login": "someone"}}),
    );
    f.comments.lock().insert(
        100,
        json!({"id": 100, "body": "@nami please fix it", "author_association": "COLLABORATOR",
               "user": {"login": "dev"}}),
    );
    f
}

/// Add the GitHub automation, the way a user does: a new trigger starts at the head of the feed (what
/// GitHub already lists is history), so the feed the test staged is held back until the baseline
/// is written, then it is "what happened after".
async fn watch_repo(
    client: &HostClient,
    dir: &std::path::Path,
    api: &FakeGithub,
    id: &str,
    extra: Value,
) {
    let staged = std::mem::take(&mut *api.events.lock());
    let mut kind =
        json!({"type": "github", "repo": "o/r", "events": ["mention"], "handle": "nami"});
    for (k, v) in extra.as_object().unwrap() {
        kind[k] = v.clone();
    }
    client
        .call(
            "crewmate_triggers_set",
            json!({"id": id, "triggers": [
                {"id": "gh", "kind": kind, "enabled": true, "prompt": "Handle this: {{event}}"}]}),
        )
        .await
        .unwrap();
    let seen = dir.join("crew").join(id).join("gh_seen.jsonl");
    until("the new trigger's baseline", async || {
        seen.exists().then_some(())
    })
    .await;
    *api.events.lock() = staged;
}

async fn tasks_of(client: &HostClient, id: &str) -> Vec<Value> {
    client
        .call("crewmate_tasks", json!({"id": id, "limit": 20}))
        .await
        .unwrap()["tasks"]
        .as_array()
        .unwrap()
        .clone()
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mention_starts_a_task_with_its_origin_replies_and_finishes_with_the_pr_link() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = seeded();
    api.prs.lock().insert(
        "fix/crash".into(),
        PullRequest {
            number: 12,
            url: "https://github.com/o/r/pull/12".into(),
        },
    );
    api.events.lock().push(mention_event("ev-1", 100));
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new()
            .sleep(1)
            .done("Fixed parse(), PR opened.")
            .script(),
    );
    use_agent_bin(&agent);

    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch_repo(&client, &dir, &api, &id, json!({})).await;

    let task = until("the mention to become a finished task", async || {
        tasks_of(&client, &id)
            .await
            .into_iter()
            .find(|t| t["state"] == "done")
    })
    .await;

    // Structured origin on the task, not parsed from the prompt.
    assert_eq!(task["origin"]["repo"], "o/r", "{task}");
    assert_eq!(task["origin"]["number"], 7);
    assert_eq!(task["origin"]["comment_id"], 100);
    assert_eq!(task["origin"]["kind"], "mention");
    assert!(
        task["origin"]["reply_comment_id"].as_u64().is_some(),
        "the comment is remembered: {task}"
    );
    // The prompt carries issue and comment, quoted behind the untrusted wrapper.
    let prompt = task["prompt"].as_str().unwrap();
    for needle in [
        "Crash on start",
        "null pointer in parse()",
        "@nami please fix it",
    ] {
        assert!(prompt.contains(needle), "{needle}: {prompt}");
    }
    assert!(prompt.contains(crate::crew::triggers::UNTRUSTED_PREAMBLE_MARK));
    assert_eq!(
        task["no_auto"], true,
        "a stranger-writable task runs as ask"
    );

    // The reply: created once, edited to the end, the first body says working, the last has the result.
    let w = until("the final edit", async || {
        let w = api.writes();
        w.iter()
            .any(|x| matches!(x, Write::Edit { body, .. } if body.contains("Fixed parse()")))
            .then_some(w)
    })
    .await;
    assert_eq!(
        w.iter()
            .filter(|x| matches!(x, Write::Create { .. }))
            .count(),
        1,
        "{w:?}"
    );
    let Write::Create { number, body } = &w[0] else {
        panic!("{w:?}")
    };
    assert_eq!(*number, 7);
    assert!(body.contains("Working on it") && body.contains(REPLY_MARKER));
    let Write::Edit { comment_id, body } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert_eq!(
        Some(*comment_id),
        task["origin"]["reply_comment_id"].as_u64()
    );
    assert!(body.contains("Fixed parse(), PR opened."), "{body}");
    // The PR link needs the branch: the mention carries none, so none is invented.
    assert!(!body.contains("pull/"), "{body}");

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Incident guarded: GitHub lists an event until it ages out; every poll sees it again.
#[tokio::test(flavor = "multi_thread")]
async fn the_same_comment_never_starts_two_tasks_and_a_strangers_never_starts_one() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = seeded();
    api.comments.lock().insert(
        101,
        json!({"id": 101, "body": "@nami do evil", "author_association": "NONE", "user": {"login": "rando"}}),
    );
    // The same event twice in one feed, then a stranger's.
    api.events.lock().extend([
        mention_event("ev-1", 100),
        mention_event("ev-1", 100),
        mention_event("ev-2", 101),
    ]);
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);

    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch_repo(&client, &dir, &api, &id, json!({})).await;

    until("the one task", async || {
        tasks_of(&client, &id)
            .await
            .iter()
            .any(|t| t["state"] == "done")
            .then_some(())
    })
    .await;
    // Let several more polls see the same events again.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let tasks = tasks_of(&client, &id).await;
    assert_eq!(tasks.len(), 1, "one comment, one task: {tasks:?}");
    assert_eq!(tasks[0]["origin"]["comment_id"], 100);
    assert!(
        api.writes()
            .iter()
            .all(|w| matches!(w, Write::Create { number: 7, .. } | Write::Edit { .. })),
        "{:?}",
        api.writes()
    );
    assert_eq!(
        api.writes()
            .iter()
            .filter(|w| matches!(w, Write::Create { .. }))
            .count(),
        1,
        "the stranger got no reply either"
    );

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The crew's own reply is a comment on the same issue and carries the handle in its text: it must
// not loop back as a task, with or without the login.
#[tokio::test(flavor = "multi_thread")]
async fn the_reply_the_crew_posted_does_not_trigger_it_again() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = seeded();
    api.events.lock().push(mention_event("ev-1", 100));
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(tmp.path(), "claude", &Hooks::new().done("ok").script());
    use_agent_bin(&agent);

    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch_repo(&client, &dir, &api, &id, json!({})).await;
    until("the task", async || {
        tasks_of(&client, &id)
            .await
            .iter()
            .any(|t| t["state"] == "done")
            .then_some(())
    })
    .await;

    // GitHub now lists our own reply (by the marker, under another login) as a new event.
    let own = format!("{REPLY_MARKER}\nDone. @nami fixed it");
    api.comments.lock().insert(
        9001,
        json!({"id": 9001, "body": own, "author_association": "MEMBER", "user": {"login": "someone-else"}}),
    );
    api.events.lock().push(
        json!({"id": "ev-own", "type": "IssueCommentEvent", "payload": {
        "action": "created",
        "issue": {"number": 7, "title": "t", "user": {"login": "someone"}},
        "comment": {"id": 9001, "body": own, "user": {"login": "someone-else"}}}}),
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(tasks_of(&client, &id).await.len(), 1);

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// A review comment names the PR's branch, so when the agent has pushed to it the PR link is found.
#[tokio::test(flavor = "multi_thread")]
async fn a_review_comment_task_finishes_with_the_pr_link_of_its_branch() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = Arc::new(FakeGithub::new("nami-bot"));
    api.issues.lock().insert(
        12,
        json!({"number": 12, "title": "Add parser", "body": "b", "author_association": "MEMBER",
               "user": {"login": "someone"}}),
    );
    api.review_comments.lock().insert(
        200,
        json!({"id": 200, "body": "O(n^2) loop", "path": "src/parse.rs", "line": 42,
               "author_association": "OWNER", "user": {"login": "dev"}}),
    );
    api.prs.lock().insert(
        "feat/parser".into(),
        PullRequest {
            number: 12,
            url: "https://github.com/o/r/pull/12".into(),
        },
    );
    api.events.lock().push(
        json!({"id": "r1", "type": "PullRequestReviewCommentEvent", "payload": {
        "action": "created",
        "pull_request": {"number": 12, "title": "Add parser", "head": {"ref": "feat/parser"}},
        "comment": {"id": 200, "body": "O(n^2) loop", "path": "src/parse.rs", "line": 42,
                    "user": {"login": "dev"}}}}),
    );
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    let agent = fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("Made the loop linear.").script(),
    );
    use_agent_bin(&agent);

    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch_repo(
        &client,
        &dir,
        &api,
        &id,
        json!({"events": ["pr_review_comment"]}),
    )
    .await;

    let task = until("the review task", async || {
        tasks_of(&client, &id)
            .await
            .into_iter()
            .find(|t| t["state"] == "done")
    })
    .await;
    assert_eq!(task["origin"]["kind"], "pr_review_comment");
    assert_eq!(task["origin"]["branch"], "feat/parser");
    assert!(task["prompt"].as_str().unwrap().contains("src/parse.rs"));
    let w = until("the final edit", async || {
        let w = api.writes();
        w.iter()
            .any(|x| matches!(x, Write::Edit { body, .. } if body.contains("pull/12")))
            .then_some(w)
    })
    .await;
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert!(
        body.contains("Made the loop linear.") && body.contains("https://github.com/o/r/pull/12"),
        "{body}"
    );

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// The agent dies: the comment says so instead of staying on "Working on it" forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_task_edits_the_comment_with_the_error() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let api = seeded();
    api.events.lock().push(mention_event("ev-1", 100));
    *TEST_GITHUB_API.lock() = Some(api.clone());
    *TEST_GITHUB_POLL.lock() = Some(Duration::from_millis(100));
    // Exits 3 without ever reporting done.
    let agent = fake_agent(tmp.path(), "claude", "exit 3");
    use_agent_bin(&agent);

    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Nami").await;
    watch_repo(&client, &dir, &api, &id, json!({})).await;

    let task = until("the failed task", async || {
        tasks_of(&client, &id)
            .await
            .into_iter()
            .find(|t| t["state"] == "failed")
    })
    .await;
    let err = task["error"].as_str().unwrap().to_string();
    let w = until("the failure edit", async || {
        let w = api.writes();
        w.iter()
            .any(|x| matches!(x, Write::Edit { body, .. } if body.contains("Failed")))
            .then_some(w)
    })
    .await;
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert!(
        body.contains(&err),
        "the comment carries the task's error {err:?}: {body}"
    );

    *TEST_GITHUB_API.lock() = None;
    *TEST_GITHUB_POLL.lock() = None;
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}
