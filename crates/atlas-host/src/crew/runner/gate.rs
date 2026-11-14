//! The gate between a successful run and `Done` (#214, #210): run the guards on the work tree, then
//! check a `ui` task proved itself, and when either refuses, park the task on a card until the owner
//! answers.
//!
//! - `Refused { reasons }`: the guards refused the work.
//! - `ProveUi { reason }`: the task is `ui` and the agent attached fewer than
//!   [`PROOF_IMAGES`](crate::crew::attachments::PROOF_IMAGES) images (captures): "could not prove it".
//!
//! Both are answered the same way, through one path:
//! - `override`: the owner accepts the work as it is; the task ends `Done`.
//! - `retry`: the task runs again from the start with the reasons added to its prompt, and with the
//!   images the owner attached to the answer (a marked screenshot).

use std::sync::Arc;

use super::action::{AnswerResult, finish_unanswered, park_task, wait_for_action_answer};
use super::enqueue_front;
use super::guard::{self, Input, OVERRIDE, RETRY};
use super::outcome::persist;
use crate::crew::attachments::{PROOF_IMAGES, SOURCE_AGENT};
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

/// Why a parked task waits, as the retry needs it: the sentences for its prompt.
enum Card {
    Refused(Vec<String>),
    ProveUi(String),
}

impl Card {
    fn of(action: &TaskAction) -> Option<Card> {
        match action {
            TaskAction::Refused { reasons } => Some(Card::Refused(reasons.clone())),
            TaskAction::ProveUi { reason } => Some(Card::ProveUi(reason.clone())),
            _ => None,
        }
    }

    fn reasons(&self) -> Vec<String> {
        match self {
            Card::Refused(r) => r.clone(),
            Card::ProveUi(r) => vec![r.clone()],
        }
    }

    fn retry_prompt(&self, prompt: &str) -> String {
        match self {
            Card::Refused(r) => guard::retry_prompt(prompt, r),
            Card::ProveUi(r) => prove_ui_retry_prompt(prompt, r),
        }
    }
}

/// How many images the agent attached to `task`.
fn agent_images(task: &Task) -> usize {
    task.attachments
        .iter()
        .filter(|a| a.source == SOURCE_AGENT)
        .count()
}

/// The reason a `ui` task is parked: the count it has and what it needs. `None` when it is proven (or is
/// not a `ui` task at all).
fn unproven(task: &Task) -> Option<String> {
    if !task.ui {
        return None;
    }
    let have = agent_images(task);
    if have >= PROOF_IMAGES {
        return None;
    }
    Some(format!(
        "Could not prove it: this is a UI task and the agent attached {have} of {PROOF_IMAGES} captures. \
         It must look at the result (the capture tool: before and after, top and end of the page) before it is done."
    ))
}

/// The prompt of a `ui` task run again after `retry`.
fn prove_ui_retry_prompt(prompt: &str, reason: &str) -> String {
    format!(
        "{prompt}\n\nYour previous attempt could not prove it: {reason}\n\
         Use the capture tool on the page you changed (before and after your change, at least two images) and \
         look at them, fix what you see, then finish again."
    )
}

/// Pull what hook routes wrote while the runner held its own copy of the task: the agent's captures
/// (`capture`) and a `ui` mark (`set_ui`) land on the stored task, not on this one.
fn refresh_from_store(hub: &Hub, crewmate: &Crewmate, task: &mut Task) {
    if let Some(stored) = hub
        .crew
        .store
        .read_tasks(&crewmate.id, usize::MAX)
        .into_iter()
        .find(|t| t.id == task.id)
    {
        task.attachments = stored.attachments;
        task.ui = task.ui || stored.ui;
    }
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
    if verdict.refused() {
        hub.crew.audit.record(
            Some(&crewmate.id),
            "task_refused",
            serde_json::json!({"task_id": task.id, "reasons": verdict.reasons}),
        );
        let action = TaskAction::Refused {
            reasons: verdict.reasons.clone(),
        };
        match park_and_wait(
            hub,
            crewmate,
            task,
            &kept,
            action,
            Card::Refused(verdict.reasons),
        )
        .await
        {
            Gate::Finish(_) => {} // overridden: the proof below still applies
            handled => return handled,
        }
    }

    // The work passed the guards (or the owner accepted it): a UI task must also have shown itself.
    prove_ui(hub, crewmate, task, kept).await
}

/// The proof a `ui` task owes before it is `done`: finish when it has it, else park on `ProveUi` and wait for
/// the owner. The one place that decides it, for a run that just finished and for a `refused` card the owner
/// overrode after a restart: an override accepts what the guards refused, never the missing proof.
async fn prove_ui(hub: &Arc<Hub>, crewmate: &Crewmate, task: &mut Task, kept: String) -> Gate {
    refresh_from_store(hub, crewmate, task);
    let Some(reason) = unproven(task) else {
        return Gate::Finish(kept);
    };
    hub.crew.audit.record(
        Some(&crewmate.id),
        "task_unproven_ui",
        serde_json::json!({"task_id": task.id, "reason": reason, "captures": agent_images(task)}),
    );
    let action = TaskAction::ProveUi {
        reason: reason.clone(),
    };
    park_and_wait(hub, crewmate, task, &kept, action, Card::ProveUi(reason)).await
}

/// Park the task on `action`, tell Telegram (the agent's images, then the buttons) and wait for the
/// owner's answer.
async fn park_and_wait(
    hub: &Arc<Hub>,
    crewmate: &Crewmate,
    task: &mut Task,
    kept: &str,
    action: TaskAction,
    card: Card,
) -> Gate {
    // What the agent said stays above the card.
    task.result = Some(super::action::visible_reply(kept)).filter(|s| !s.is_empty());
    park_task(hub, crewmate, task, action);
    refresh_from_store(hub, crewmate, task);
    hub.crew
        .telegram
        .send_card(&crewmate.name, &crewmate.id, &task.id, &card.reasons());
    answer_card(hub, crewmate, task, kept.to_string(), card).await
}

/// A card parked when hostd stopped is still waiting: the owner's answer decides it as it would
/// have before. Returns `true` when `task` was such a task (the runner has nothing more to do).
pub(super) async fn resume_refusal(hub: &Arc<Hub>, crewmate: &Crewmate, task: &mut Task) -> bool {
    let Some(card) = task.action.as_ref().and_then(Card::of) else {
        return false;
    };
    if task.state != TaskState::WaitingAction {
        return false;
    }
    let kept = task.result.clone().unwrap_or_default();
    let refused = matches!(card, Card::Refused(_));
    let mut gate = answer_card(hub, crewmate, task, kept, card).await;
    // As when nothing restarted: the guards were overridden, the proof a UI task owes still has to be shown.
    if refused && let Gate::Finish(kept) = gate {
        gate = prove_ui(hub, crewmate, task, kept).await;
    }
    if let Gate::Finish(result) = gate {
        // Accepted: the work stands as it was, with the same end any task gets.
        task.reply = Some(result.clone());
        super::outcome::finish_task(hub, crewmate, task, Ok(result), vec![]);
    }
    true
}

/// Wait for the owner's answer to a card and carry it out.
async fn answer_card(
    hub: &Arc<Hub>,
    crewmate: &Crewmate,
    task: &mut Task,
    kept: String,
    card: Card,
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
            retry(hub, crewmate, task, &card);
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
                Err(format!("refused: {}", card.reasons().join("; "))),
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

/// Run the task again with the card's reasons appended to its prompt, ahead of the tasks queued after
/// it. The images the owner attached to the answer are on the stored task: they are kept with it.
fn retry(hub: &Hub, crewmate: &Crewmate, task: &mut Task, card: &Card) {
    hub.crew.tape.forget(&task.id);
    // The run that was refused left its pane `done` with its last message. The retry reuses the pane
    // key, and a second `done` with the same message changes nothing a client shows, so no status
    // would be pushed and the runner would never see the agent finish (it would fail it on exit).
    hub.agents
        .idle(&crate::crew::pane::key(&crewmate.id, &task.id));
    refresh_from_store(hub, crewmate, task);
    task.prompt = card.retry_prompt(&task.prompt);
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
