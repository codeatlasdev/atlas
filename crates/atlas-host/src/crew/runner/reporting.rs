//! A task that came from GitHub answers there: the reply comment is posted when the task starts,
//! edited as steps arrive and edited a last time when the task ends, whichever way it ends, with the
//! link of the PR the agent opened.
//!
//! The live [`Live`] of a task sits in `CrewHub::reporters` keyed by task id, so the runner's start,
//! its step loop and `finish_task` (where every ending lands) reach it without each of them carrying
//! one around. Everything here is best effort: GitHub failing never fails the task.

use std::path::PathBuf;

use super::outcome::persist;
use crate::crew::github::reply::Reporter;
use crate::crew::gitrun::Git;
use crate::crew::project::{current_branch, work_dir_for};
use crate::crew::store::{Step, Task, TaskState};
use crate::server::Hub;

/// A task's reply comment and what is needed to find its PR at the end.
pub(crate) struct Live {
    reporter: Reporter,
    /// Where the agent works, and the branch it was on when the task started.
    work_dir: PathBuf,
    /// Who runs git in `work_dir` (the crewmate itself for an isolated tree).
    git: Git,
    started_on: Option<String>,
}

/// The branch to look a PR up for: the one the agent moved to during the task. Staying on the
/// branch it started on (the crewmate's own tracking branch) is no work to link, and an old PR of
/// that head is not this task's.
pub(crate) fn branch_to_report(
    started_on: Option<String>,
    ended_on: Option<String>,
) -> Option<String> {
    ended_on.filter(|e| started_on.as_ref() != Some(e))
}

/// Post "Working on it" for a task with a GitHub origin and remember the comment on the task, so a
/// restart keeps editing it. No origin: nothing.
pub(super) async fn start(hub: &Hub, crewmate_id: &str, task: &mut Task) {
    // The agent can run twice for one task (a second turn after an answered action): one comment.
    if hub.crew.reporters.lock().contains_key(&task.id) {
        return;
    }
    let Some(origin) = task.origin.clone() else {
        return;
    };
    // Where the agent works: an isolated crewmate's own folder, not the crew home.
    let dir = work_dir_for(
        &hub.crew.dir,
        crewmate_id,
        hub.crew.store.isolated(crewmate_id).as_ref(),
    );
    let git = hub.crew.store.git_in(&dir);
    let mut reporter = Reporter::new(hub.crew.github(), origin);
    if let Some(id) = reporter.start().await {
        if let Some(o) = task.origin.as_mut() {
            o.reply_comment_id = Some(id);
        }
        let _ = persist(hub, crewmate_id, task);
    }
    let live = Live {
        reporter,
        started_on: current_branch(&dir, &git).await,
        work_dir: dir,
        git,
    };
    hub.crew.reporters.lock().insert(task.id.clone(), live);
}

/// The steps so far (throttled inside the reporter).
pub(super) async fn progress(hub: &Hub, task_id: &str, steps: &[Step]) {
    // Taken out for the await: the map lock is never held across a GitHub call.
    let Some(mut live) = hub.crew.reporters.lock().remove(task_id) else {
        return;
    };
    live.reporter.progress(steps).await;
    hub.crew.reporters.lock().insert(task_id.to_string(), live);
}

/// The task ended: edit the comment with its result or error. Called by `finish_task`, which is
/// sync, so the GitHub calls run on their own task. A task without a reporter does nothing.
pub(super) fn finish(hub: &Hub, task: &Task) {
    let Some(mut live) = hub.crew.reporters.lock().remove(&task.id) else {
        return;
    };
    let (state, result, error, steps) = (
        task.state,
        task.result.clone().unwrap_or_default(),
        task.error.clone().unwrap_or_default(),
        task.steps.clone(),
    );
    tokio::spawn(async move {
        match state {
            TaskState::Done => {
                let ended_on = current_branch(&live.work_dir, &live.git).await;
                live.reporter
                    .set_branch(branch_to_report(live.started_on.take(), ended_on));
                live.reporter.finish_ok(&result, &steps).await;
            }
            TaskState::Cancelled => live.reporter.finish_err("cancelled").await,
            _ => live.reporter.finish_err(&error).await,
        }
    });
}
