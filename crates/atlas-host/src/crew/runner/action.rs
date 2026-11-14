//! `ATLAS_ACTION:` lines: what an agent asks of the user (connect GitHub, a secret, a choice, a
//! sign-in, an automation). Parsing a reply, parking the task as `waiting_action`, waiting for the
//! answer and turning it into the next prompt.

use std::sync::Arc;
use std::time::Duration;

use super::TASK_TIMEOUT;
use super::github::{WaitResult, wait_for_github_connect};
use super::outcome::{broadcast_task, finish_task, was_cancelled};
use crate::crew::store::{Crewmate, Task, TaskAction, TaskState, Trigger};
use crate::crew::triggers;
use crate::server::Hub;

const ATLAS_ACTION_PREFIX: &str = "ATLAS_ACTION: ";

pub(super) enum AnswerResult {
    Answer(String),
    Cancelled,
    Timeout,
}

/// Parse the last `ATLAS_ACTION: {…}` line in `text`.
/// Unknown kinds fall back to `Input`.
pub(crate) fn parse_atlas_action(text: &str) -> Option<TaskAction> {
    let line = text
        .lines()
        .rev()
        .find(|l| l.trim().starts_with(ATLAS_ACTION_PREFIX))?;
    let json_str = line.trim().strip_prefix(ATLAS_ACTION_PREFIX)?;
    if let Ok(action) = serde_json::from_str::<TaskAction>(json_str) {
        return Some(action);
    }
    // Unknown kind → fall back to Input
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let label = v["label"].as_str().unwrap_or("Response needed").to_string();
    let reason = v["reason"].as_str().unwrap_or("").to_string();
    let placeholder = v["placeholder"].as_str().map(str::to_string);
    Some(TaskAction::Input {
        label,
        reason,
        placeholder,
    })
}

/// The reply as people should read it: without the `ATLAS_ACTION:` line, which is for Atlas (it becomes the card).
pub(crate) fn visible_reply(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim().starts_with(ATLAS_ACTION_PREFIX))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// What the chat shows as the user's own message after an action: what they picked or typed. Secrets, GitHub and
/// sign-ins resolve with a system note ("The secret X is now set") that is not something the user said.
pub(crate) fn user_answer(action: &TaskAction, resolved: &str) -> Option<String> {
    matches!(action, TaskAction::Choose { .. } | TaskAction::Input { .. })
        .then(|| resolved.to_string())
}

/// The next turn after the user answered an action. A terminal agent starts fresh each run, so it gets its own last
/// reply and the answer; re-sending only the original prompt made it ask the same question again.
pub(crate) fn continuation_prompt(base: &str, reply: &str, answer: &str) -> String {
    format!(
        "{base}\n\n\
         You already replied, and asked the user a question:\n\
         ---\n{said}\n---\n\
         The user answered: {answer}\n\
         Continue from that answer. Do not ask it again.",
        said = visible_reply(reply),
    )
}

pub(super) async fn wait_for_action_answer(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    task_id: &str,
) -> AnswerResult {
    let rx = hub.crew.wait_action(task_id.to_string());
    let deadline = tokio::time::sleep(TASK_TIMEOUT);
    tokio::pin!(deadline);
    tokio::pin!(rx);
    loop {
        let cancel_check = tokio::time::sleep(Duration::from_secs(30));
        tokio::select! {
            biased;
            _ = &mut deadline => return AnswerResult::Timeout,
            result = &mut rx => match result {
                Ok(value) => return AnswerResult::Answer(value),
                Err(_) if was_cancelled(hub, crewmate_id, task_id) => return AnswerResult::Cancelled,
                Err(_) => return AnswerResult::Timeout,
            },
            _ = cancel_check => {
                if was_cancelled(hub, crewmate_id, task_id) {
                    return AnswerResult::Cancelled;
                }
            }
        }
    }
}

/// Park `task` as WaitingAction with `action`, store, and broadcast.
pub(super) fn park_task(hub: &Hub, crewmate: &Crewmate, task: &mut Task, action: TaskAction) {
    task.state = TaskState::WaitingAction;
    task.action = Some(action);
    let _ = hub.crew.store.append_task(&crewmate.id, task);
    broadcast_task(hub, &crewmate.id, task);
}

/// The action got no answer (it was cancelled, or timed out): a cancelled task stays cancelled, any
/// other ends failed. Both runners end an unanswered action here.
pub(super) fn finish_unanswered(hub: &Hub, crewmate: &Crewmate, task: &mut Task) {
    if was_cancelled(hub, &crewmate.id, &task.id) {
        task.state = TaskState::Cancelled;
    } else {
        finish_task(
            hub,
            crewmate,
            task,
            Err("timed out waiting for action response".into()),
            vec![],
        );
    }
}

/// Wait for the action to be resolved; return the follow-up message or None (cancelled/timeout).
pub(super) async fn resolve_action(
    hub: &Arc<Hub>,
    crewmate_id: &str,
    task_id: &str,
    action: &TaskAction,
) -> Option<String> {
    match action {
        TaskAction::ConnectGithub { .. } => {
            match wait_for_github_connect(hub, crewmate_id, task_id).await {
                WaitResult::Connected => Some("GitHub is connected now. Continue.".to_string()),
                _ => None,
            }
        }
        TaskAction::ProvideSecret { .. } => {
            match wait_for_action_answer(hub, crewmate_id, task_id).await {
                AnswerResult::Answer(received_name) => {
                    Some(format!("The secret {received_name} is now set. Continue."))
                }
                _ => None,
            }
        }
        TaskAction::Input { .. } => match wait_for_action_answer(hub, crewmate_id, task_id).await {
            AnswerResult::Answer(text) => Some(text),
            _ => None,
        },
        TaskAction::Choose { .. } => {
            match wait_for_action_answer(hub, crewmate_id, task_id).await {
                AnswerResult::Answer(choice) => Some(choice),
                _ => None,
            }
        }
        TaskAction::SignInEngine { engine } => {
            let engine = engine.clone();
            match wait_for_action_answer(hub, crewmate_id, task_id).await {
                AnswerResult::Answer(_) => {
                    Some(format!("The sign-in for {engine} is complete. Continue."))
                }
                _ => None,
            }
        }
        TaskAction::AddTrigger { trigger, .. } => {
            // Validate the trigger before waiting; if invalid, resolve immediately so
            // the agent can correct and retry (no user card is shown for a bad proposal).
            if let Err(e) = triggers::validate_trigger(&Trigger {
                id: String::new(),
                kind: trigger.clone(),
                enabled: true,
                prompt: String::new(),
                last_fired: None,
                tz: None,
                next_fire: None,
            }) {
                return Some(format!(
                    "The automation was invalid: {e}. Please fix it and try again."
                ));
            }
            match wait_for_action_answer(hub, crewmate_id, task_id).await {
                AnswerResult::Answer(v) if v == "add" => {
                    Some("The user added the automation.".to_string())
                }
                AnswerResult::Answer(_) => Some("The user did not add the automation.".to_string()),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident: carlos offered "DANFE / Pix / …" as a choice, the user picked DANFE, and carlos asked the very same
    // question again. The terminal runner re-ran the ORIGINAL prompt after the answer, so the agent never saw it.
    #[test]
    fn the_follow_up_carries_the_answer_and_what_was_asked() {
        let reply = "Sugiro DANFE ou Pix.\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Por onde?\",\"options\":[\"DANFE\",\"Pix\"]}";
        let p = continuation_prompt("BASE", reply, "DANFE");
        assert!(p.starts_with("BASE"));
        assert!(
            p.contains("Sugiro DANFE ou Pix."),
            "the agent sees its own last reply"
        );
        assert!(p.contains("DANFE"), "and the user's answer");
        assert!(
            !p.contains("ATLAS_ACTION"),
            "not the machine line it already resolved"
        );
        assert!(p.to_lowercase().contains("do not ask it again"));
    }

    // The action line is for Atlas, not for people: it was shown raw at the end of the reply.
    #[test]
    fn the_reply_shown_to_the_user_drops_the_action_line() {
        let reply = "Posso fazer A ou B.\n\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"q\",\"options\":[\"A\"]}\n";
        assert_eq!(visible_reply(reply), "Posso fazer A ou B.");
        assert_eq!(visible_reply("Sem ação."), "Sem ação.");
    }

    // Incident: the user picked "Baixar o DANFE" on a choice card and the conversation showed nothing; the pick must be
    // recorded as their message, while a secret's note must never pose as something they said.
    #[test]
    fn only_picks_and_typed_answers_become_the_users_message() {
        let choose = TaskAction::Choose {
            question: "q".into(),
            options: vec!["DANFE".into()],
        };
        assert_eq!(user_answer(&choose, "DANFE").as_deref(), Some("DANFE"));
        let secret = TaskAction::ProvideSecret {
            name: "K".into(),
            label: "K".into(),
            reason: "r".into(),
        };
        assert_eq!(
            user_answer(&secret, "The secret K is now set. Continue."),
            None
        );
        assert_eq!(
            user_answer(
                &TaskAction::ConnectGithub { reason: "r".into() },
                "GitHub is connected now."
            ),
            None
        );
    }
}
