//! GitHub out (issue #204): the crewmate answers where it was asked. A comment is posted when the
//! task starts, edited (never re-posted) as steps arrive, at most once per 30 s, and edited a last
//! time with the result and the PR link, or the error. No network: a fake `GithubApi`.

use std::sync::Arc;
use std::time::Duration;

use super::github_fake::{FakeGithub, Write};
use crate::crew::github::reply::{PROGRESS_INTERVAL, Reporter};
use crate::crew::github::{PullRequest, REPLY_MARKER};
use crate::crew::store::{GithubOrigin, GithubOriginKind, Step};

fn origin(branch: Option<&str>) -> GithubOrigin {
    GithubOrigin {
        repo: "o/r".into(),
        kind: GithubOriginKind::Mention,
        number: Some(7),
        comment_id: Some(100),
        branch: branch.map(String::from),
        reply_comment_id: None,
        requester: None,
    }
}

fn steps(n: usize) -> Vec<Step> {
    (1..=n)
        .map(|i| Step {
            ts: i as u64,
            text: format!("edit src/file{i}.rs"),
            group: None,
        })
        .collect()
}

fn reporter(api: &Arc<FakeGithub>, o: GithubOrigin) -> Reporter {
    Reporter::new(api.clone(), o)
}

#[tokio::test(start_paused = true)]
async fn a_reply_comment_is_created_on_start_and_marked_as_ours() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(None));
    let id = r.start().await.expect("a comment id");
    let w = api.writes();
    assert_eq!(w.len(), 1, "{w:?}");
    let Write::Create { number, body } = &w[0] else {
        panic!("{w:?}")
    };
    assert_eq!(*number, 7);
    assert!(body.contains("Working on it"), "{body}");
    assert!(
        body.contains(REPLY_MARKER),
        "carries the hidden marker so it never triggers itself: {body}"
    );
    assert_eq!(
        id, 9001,
        "the id of the comment that was created is returned to the runner"
    );
}

// The checklist is link-free: a path or a command in a step must not become a link, a mention or
// an image that pings someone from a comment the agent wrote.
#[tokio::test(start_paused = true)]
async fn progress_edits_the_same_comment_with_a_link_free_checklist() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(None));
    let id = r.start().await.unwrap();
    tokio::time::advance(PROGRESS_INTERVAL + Duration::from_secs(1)).await;
    let mut s = steps(2);
    s[1].text =
        "bash curl https://evil.example/x @someone ![i](http://x/y.png) [a](http://b)".into();
    r.progress(&s).await;
    let w = api.writes();
    let Write::Edit { comment_id, body } = &w[1] else {
        panic!("{w:?}")
    };
    assert_eq!(
        *comment_id, id,
        "edits the one comment, never posts a second"
    );
    assert!(body.contains("- [x] edit src/file1.rs"), "{body}");
    assert!(!body.contains("http"), "no links: {body}");
    assert!(!body.contains("](") && !body.contains("!["), "{body}");
    assert!(!body.contains("@someone"), "no pings: {body}");
    assert_eq!(
        w.iter()
            .filter(|x| matches!(x, Write::Create { .. }))
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn progress_is_throttled_to_one_edit_per_interval() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(None));
    r.start().await.unwrap();
    // A burst of steps right after the start: nothing is edited.
    for n in 1..=5 {
        r.progress(&steps(n)).await;
    }
    assert_eq!(api.writes().len(), 1, "only the create: {:?}", api.writes());

    tokio::time::advance(PROGRESS_INTERVAL).await;
    r.progress(&steps(6)).await;
    assert_eq!(api.writes().len(), 2);
    // Still inside the new interval.
    tokio::time::advance(Duration::from_secs(5)).await;
    r.progress(&steps(7)).await;
    assert_eq!(api.writes().len(), 2);
    // The edit that finally goes out carries every step so far (the throttled ones are not lost).
    tokio::time::advance(PROGRESS_INTERVAL).await;
    r.progress(&steps(8)).await;
    let w = api.writes();
    assert_eq!(w.len(), 3);
    let Write::Edit { body, .. } = &w[2] else {
        panic!()
    };
    assert!(body.contains("file8.rs"), "{body}");
}

#[tokio::test(start_paused = true)]
async fn finish_edits_with_the_result_and_the_pr_link_without_waiting_for_the_throttle() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    api.prs.lock().insert(
        "feat/fix".into(),
        PullRequest {
            number: 12,
            url: "https://github.com/o/r/pull/12".into(),
        },
    );
    let mut r = reporter(&api, origin(Some("feat/fix")));
    let id = r.start().await.unwrap();
    // Finished one second after the start: still edited.
    tokio::time::advance(Duration::from_secs(1)).await;
    r.finish_ok("Fixed the null pointer in parse().", &steps(3))
        .await;
    let w = api.writes();
    let Write::Edit { comment_id, body } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert_eq!(*comment_id, id);
    assert!(
        body.contains("Fixed the null pointer in parse()."),
        "{body}"
    );
    assert!(
        body.contains("https://github.com/o/r/pull/12"),
        "the PR link: {body}"
    );
    assert!(body.contains(REPLY_MARKER));
    assert!(!body.contains("Working on it"), "{body}");
}

#[tokio::test(start_paused = true)]
async fn finish_without_a_pr_has_no_link_and_does_not_invent_one() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(Some("feat/none")));
    r.start().await.unwrap();
    r.finish_ok(
        "Nothing to change: the crash is already fixed on main.",
        &[],
    )
    .await;
    let w = api.writes();
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!()
    };
    assert!(body.contains("already fixed on main"));
    assert!(!body.contains("pull/"), "{body}");
}

#[tokio::test(start_paused = true)]
async fn a_failure_edits_the_comment_with_the_error() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(None));
    r.start().await.unwrap();
    r.finish_err("agent exited (code 1)").await;
    let w = api.writes();
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert!(body.contains("agent exited (code 1)"), "{body}");
    assert!(body.contains("failed") || body.contains("Failed"), "{body}");
    assert!(!body.contains("Working on it"));
}

// A restart in the middle of a task must keep editing the comment it already posted.
#[tokio::test(start_paused = true)]
async fn a_comment_that_already_exists_is_edited_not_posted_again() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut o = origin(None);
    o.reply_comment_id = Some(555);
    let mut r = reporter(&api, o);
    r.start().await.unwrap();
    let w = api.writes();
    assert!(
        matches!(
            &w[0],
            Write::Edit {
                comment_id: 555,
                ..
            }
        ),
        "{w:?}"
    );
    assert!(w.iter().all(|x| !matches!(x, Write::Create { .. })));
}

// GitHub being down must never fail or stall the task: reporting is best effort.
#[tokio::test(start_paused = true)]
async fn a_github_write_error_is_swallowed_and_the_next_call_still_works() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    *api.write_error.lock() = Some("502 bad gateway".into());
    let mut r = reporter(&api, origin(None));
    assert_eq!(r.start().await, None, "no comment id, no panic");
    r.finish_ok("done", &[]).await; // nothing to edit, nothing thrown
    assert!(api.writes().is_empty());
    // GitHub recovers: the finish still reaches the issue as a fresh comment.
    *api.write_error.lock() = None;
    r.finish_ok("done", &[]).await;
    let w = api.writes();
    assert!(
        matches!(&w[0], Write::Create { number: 7, body } if body.contains("done")),
        "{w:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_long_result_is_cut_to_fit_a_comment() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    let mut r = reporter(&api, origin(None));
    r.start().await.unwrap();
    r.finish_ok(&"y".repeat(100_000), &[]).await;
    let w = api.writes();
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!()
    };
    assert!(
        body.chars().count() < 65_536,
        "GitHub rejects comments over 65536: {}",
        body.len()
    );
}

// The agent was asked in a comment, so the origin names no branch: the one it pushed is read from
// the work dir and its PR is found with `gh pr list --head <branch>`.
#[tokio::test(start_paused = true)]
async fn a_branch_found_in_the_work_dir_gives_the_pr_link_when_the_origin_has_none() {
    let api = Arc::new(FakeGithub::new("nami-bot"));
    api.prs.lock().insert(
        "nami/fix-crash".into(),
        PullRequest {
            number: 31,
            url: "https://github.com/o/r/pull/31".into(),
        },
    );
    let mut r = reporter(&api, origin(None));
    r.start().await.unwrap();
    r.set_branch(Some("nami/fix-crash".into()));
    r.finish_ok("Opened a PR.", &[]).await;
    let w = api.writes();
    let Write::Edit { body, .. } = w.last().unwrap() else {
        panic!("{w:?}")
    };
    assert!(body.contains("https://github.com/o/r/pull/31"), "{body}");

    // The branch the origin names (a PR's own head) wins over the work dir's.
    let api = Arc::new(FakeGithub::new("nami-bot"));
    api.prs.lock().insert(
        "feat/x".into(),
        PullRequest {
            number: 5,
            url: "https://github.com/o/r/pull/5".into(),
        },
    );
    api.prs.lock().insert(
        "other".into(),
        PullRequest {
            number: 6,
            url: "https://github.com/o/r/pull/6".into(),
        },
    );
    let mut r = reporter(&api, origin(Some("feat/x")));
    r.start().await.unwrap();
    r.set_branch(Some("other".into()));
    r.finish_ok("ok", &[]).await;
    let Write::Edit { body, .. } = api.writes().last().unwrap().clone() else {
        panic!()
    };
    assert!(
        body.contains("pull/5") && !body.contains("pull/6"),
        "{body}"
    );
}

// What is read is the checked-out branch: nothing for a detached HEAD or a folder that is no repo.
#[tokio::test]
async fn the_branch_of_a_git_work_dir_is_read() {
    use crate::crew::project::current_branch;
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git")
            .args(args)
            .current_dir(d)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "i",
    ]);
    assert_eq!(
        current_branch(d, &crate::crew::gitrun::Git::local())
            .await
            .as_deref(),
        Some("main")
    );
    git(&["checkout", "-q", "-b", "nami/fix"]);
    assert_eq!(
        current_branch(d, &crate::crew::gitrun::Git::local())
            .await
            .as_deref(),
        Some("nami/fix")
    );
    git(&["checkout", "-q", "--detach"]);
    assert_eq!(
        current_branch(d, &crate::crew::gitrun::Git::local()).await,
        None,
        "a detached HEAD has no branch"
    );
    assert_eq!(
        current_branch(&d.join("missing"), &crate::crew::gitrun::Git::local()).await,
        None,
        "not a repo, no panic"
    );
}

// A crewmate's worktree sits on its own tracking branch (`atlas/<id>/main`). Only a branch the
// agent moved to during the task is its work: staying where it started links no PR (an old PR of
// that head is not this task's).
#[test]
fn only_a_branch_the_agent_moved_to_is_reported() {
    use crate::crew::runner::branch_to_report;
    let s = |x: &str| Some(x.to_string());
    assert_eq!(
        branch_to_report(s("atlas/c1/main"), s("nami/fix")),
        s("nami/fix")
    );
    assert_eq!(
        branch_to_report(s("atlas/c1/main"), s("atlas/c1/main")),
        None,
        "did not move"
    );
    assert_eq!(
        branch_to_report(None, s("nami/fix")),
        s("nami/fix"),
        "started detached"
    );
    assert_eq!(
        branch_to_report(s("atlas/c1/main"), None),
        None,
        "ended detached"
    );
    assert_eq!(branch_to_report(None, None), None);
}
