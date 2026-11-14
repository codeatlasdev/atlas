//! The gate between a successful run and `Done` (#214): run the guards on the work tree, and when
//! they refuse, park the task on a `Refused` card until the owner answers.
//!
//! - `override`: the owner accepts the work as it is; the task ends `Done`.
//! - `retry`: the task runs again from the start with the reasons added to its prompt.

use std::sync::Arc;

use super::action::{AnswerResult, finish_unanswered, park_task, wait_for_action_answer};
use super::enqueue_front;
use super::guard::{self, Input, OVERRIDE, RETRY};
use super::outcome::persist;
use crate::crew::project;
use crate::crew::store::{Crewmate, Task, TaskAction, TaskState};
use crate::server::Hub;

/// What the runner does next with a result that passed (or was overridden).
pub(super) enum Gate {
    /// End the task with this result (the claims the guard could not back are already removed).
    Finish(String),
    /// The task was queued again or ended unanswered: the runner has nothing left to record.
    Handled,
}

/// Check `result`, the reply of a run that succeeded, against the work tree at `work_dir`.
/// A directory that is not a git work tree has no diff to read and passes the diff checks.
pub(super) async fn gate_success(
    hub: &Arc<Hub>,
    crewmate: &Crewmate,
    task: &mut Task,
    result: String,
    work_dir: &str,
) -> Gate {
    let dir = std::path::Path::new(work_dir);
    // Hub tapes are per task id; a retry starts a fresh one.
    let commands = hub.crew.tape.of(&task.id);
    let repo = project::infer_github_repo(dir);
    let api = hub.crew.github();
    let verdict = guard::review(
        &Input {
            work_dir: dir,
            repo: repo.as_deref(),
            result: &result,
            commands: &commands,
        },
        api.as_ref(),
    )
    .await;

    // The claims that no command backs are removed whether or not the work is refused: a refused
    // task overridden by its owner still must not carry the sentence.
    let mut kept = verdict.result.clone();
    if !verdict.removed.is_empty() {
        kept.push_str(&guard::removal_note(&verdict.removed));
    }
    if !verdict.refused() {
        return Gate::Finish(kept);
    }

    hub.crew.audit.record(
        Some(&crewmate.id),
        "task_refused",
        serde_json::json!({"task_id": task.id, "reasons": verdict.reasons}),
    );
    // What the agent said stays above the card.
    task.result = Some(super::action::visible_reply(&kept)).filter(|s| !s.is_empty());
    park_task(
        hub,
        crewmate,
        task,
        TaskAction::Refused {
            reasons: verdict.reasons.clone(),
        },
    );
    hub.crew
        .telegram
        .send_refusal(&crewmate.name, &task.id, &verdict.reasons);
    answer_refusal(hub, crewmate, task, kept, &verdict.reasons).await
}

/// A refusal parked when hostd stopped is still waiting: the owner's answer decides it as it would
/// have before. Returns `true` when `task` was such a task (the runner has nothing more to do).
pub(super) async fn resume_refusal(hub: &Arc<Hub>, crewmate: &Crewmate, task: &mut Task) -> bool {
    let Some(TaskAction::Refused { reasons }) = task.action.clone() else {
        return false;
    };
    if task.state != TaskState::WaitingAction {
        return false;
    }
    let kept = task.result.clone().unwrap_or_default();
    if let Gate::Finish(result) = answer_refusal(hub, crewmate, task, kept, &reasons).await {
        // Overridden: the work stands as it was, with the same end any task gets.
        task.reply = Some(result.clone());
        super::outcome::finish_task(hub, crewmate, task, Ok(result), vec![]);
    }
    true
}

/// Wait for the owner's answer to a refusal and carry it out.
async fn answer_refusal(
    hub: &Arc<Hub>,
    crewmate: &Crewmate,
    task: &mut Task,
    kept: String,
    reasons: &[String],
) -> Gate {
    match wait_for_action_answer(hub, &crewmate.id, &task.id).await {
        AnswerResult::Answer(a) if a == OVERRIDE => {
            hub.crew.audit.record(
                Some(&crewmate.id),
                "refusal_overridden",
                serde_json::json!({"task_id": task.id}),
            );
            task.action = None;
            Gate::Finish(kept)
        }
        AnswerResult::Answer(a) if a == RETRY => {
            retry(hub, crewmate, task, reasons);
            Gate::Handled
        }
        AnswerResult::Answer(other) => {
            // An answer this card does not offer: the work stays refused, and so does the task.
            eprintln!(
                "atlas crew: crewmate {} task {}: unknown answer {other:?} to a refusal",
                crewmate.id, task.id
            );
            task.action = None;
            super::outcome::finish_task(
                hub,
                crewmate,
                task,
                Err(format!("refused: {}", reasons.join("; "))),
                vec![],
            );
            Gate::Handled
        }
        AnswerResult::Cancelled | AnswerResult::Timeout => {
            finish_unanswered(hub, crewmate, task);
            Gate::Handled
        }
    }
}

/// Run the task again with `reasons` appended to its prompt, ahead of the tasks queued after it.
fn retry(hub: &Hub, crewmate: &Crewmate, task: &mut Task, reasons: &[String]) {
    hub.crew.tape.forget(&task.id);
    // The run that was refused left its pane `done` with its last message. The retry reuses the pane
    // key, and a second `done` with the same message changes nothing a client shows, so no status
    // would be pushed and the runner would never see the agent finish (it would fail it on exit).
    hub.agents
        .idle(&crate::crew::pane::key(&crewmate.id, &task.id));
    task.prompt = guard::retry_prompt(&task.prompt, reasons);
    task.state = TaskState::Queued;
    task.action = None;
    task.result = None;
    task.reply = None;
    task.reply_full = None;
    task.summarized = false;
    task.finished_at = None;
    task.started_at = None;
    task.answer = None;
    let _ = persist(hub, &crewmate.id, task);
    super::outcome::broadcast_task(hub, &crewmate.id, task);
    enqueue_front(hub, crewmate, task.clone());
}
