//! Answering a card by voice (#648): `agent_approve_voice` and `crewmate_action_answer_voice`. A client sends the
//! transcript; this decides what it means (`voice_answer`) and whether a spoken answer may decide it at all.
//!
//! A spoken **allow** never reaches a prompt that always needs a tap on screen, and says why (`needs_tap`):
//! - `bypass`: the crewmate is in Bypass (every prompt it still has is one the owner wants to see);
//! - `rail`: a protected path, or a task started from a stranger's text;
//! - `destructive`: [`approval::destructive`] reads the call as one that destroys data;
//! - `untrusted`: the transcript came from a source that is not the owner's (a Telegram chat that is not linked).
//!
//! A spoken **deny** is always allowed (denying is safe), except from an untrusted source. An allow answers ONCE,
//! never for the task. Every refusal is one `approval_voice_refused` audit entry; every decided answer is audited
//! like a tap, with `via: voice` and the transcript (`approval_decision`; `card_answered` for a choice).
//! Everything that is not exactly a phrase of `voice_answer` decides nothing: it comes back as a comment to confirm
//! by tap, or as unclear.

use serde_json::{Value, json};

use super::store::{Mode, TaskAction, TaskState};
use super::voice_answer::{self, Heard};
use super::{approval, pane};
use crate::agents::{Answer, ApproveScope, MAX_TRANSCRIPT};
use crate::server::Hub;

/// `origin` is who is talking (`Hub::origin_of`): the owner's desktop, a phone that was paired, a linked Telegram
/// chat (`linked` says which). Anything else (a crewmate's own connection, a chat that is not linked, a name no
/// client sends) is not the owner, and no transcript of it answers anything.
fn trusted(origin: &str, linked: impl Fn(i64) -> bool) -> bool {
    match origin {
        "desktop" => true,
        o if o.starts_with("phone:") => true,
        o => o
            .strip_prefix("telegram:")
            .and_then(|chat| chat.parse::<i64>().ok())
            .is_some_and(linked),
    }
}

fn owner(hub: &Hub, origin: &str) -> bool {
    trusted(origin, |chat| hub.crew.telegram.is_linked(chat))
}

/// The transcript as the audit keeps it.
fn clip(transcript: &str) -> String {
    transcript.trim().chars().take(MAX_TRANSCRIPT).collect()
}

/// `{"verdict": "needs_tap", "reason": …}`: nothing was decided.
pub(crate) fn needs_tap(reason: Option<&str>) -> Value {
    json!({"verdict": "needs_tap", "reason": reason})
}

/// A spoken answer was turned away: say so in the crewmate's audit.
fn refused(
    hub: &Hub,
    pane: &str,
    origin: &str,
    transcript: &str,
    reason: &str,
    what: Value,
) -> Value {
    let mut f = json!({"reason": reason, "device": origin, "transcript": clip(transcript)});
    if let (Some(f), Some(what)) = (f.as_object_mut(), what.as_object()) {
        f.extend(what.clone());
    }
    hub.crew
        .audit
        .record(pane::crewmate_id(pane), "approval_voice_refused", f);
    needs_tap(Some(reason))
}

/// Why an allow of `event` (an `agent_approval` push) cannot be spoken, in the order a person would check.
fn tap_reason(hub: &Hub, event: &Value) -> Option<&'static str> {
    let pane = event["pane_key"].as_str().unwrap_or_default();
    let in_bypass = pane::crewmate_id(pane)
        .and_then(|id| hub.crew.store.load_crewmate(id).ok())
        .is_some_and(|c| c.mode == Mode::Bypass);
    if in_bypass {
        Some("bypass")
    } else if event["rail"].is_string() {
        Some("rail")
    } else if approval::destructive(&approval::Call {
        tool: event["tool_name"].as_str().unwrap_or_default(),
        input: &event["tool_input"],
        work_dir: None,
        no_auto: false,
    }) {
        Some("destructive")
    } else {
        None
    }
}

/// `agent_approve_voice`: the transcript against the prompt `req` of `pane`.
pub(crate) fn approve(
    hub: &Hub,
    pane: &str,
    req: &str,
    transcript: &str,
    lang: Option<&str>,
    origin: &str,
) -> Result<Value, String> {
    let which = json!({"req": req});
    if !owner(hub, origin) {
        return Ok(refused(hub, pane, origin, transcript, "untrusted", which));
    }
    // A stale or foreign request is the error a tap gets (`AgentHub::answer`), whatever was said.
    let event = hub
        .agents
        .pending_approvals()
        .into_iter()
        .find(|e| e["req"] == req && e["pane_key"] == pane)
        .ok_or_else(|| "no such pending request".to_string())?;
    let allow = match voice_answer::approval(transcript, lang) {
        Heard::Unclear => return Ok(json!({"verdict": "unclear"})),
        Heard::Comment => return Ok(json!({"verdict": "comment", "comment": transcript.trim()})),
        Heard::Deny => false,
        Heard::Allow => {
            if let Some(reason) = tap_reason(hub, &event) {
                let what = json!({"req": req, "tool": event["tool_name"]});
                return Ok(refused(hub, pane, origin, transcript, reason, what));
            }
            true
        }
    };
    hub.agents.answer(
        pane,
        req,
        Answer {
            allow,
            by: origin.to_string(),
            scope: ApproveScope::Once,
            comment: None,
            source: "person",
            voice: Some(clip(transcript)),
        },
    )?;
    Ok(json!({"verdict": if allow { "allow" } else { "deny" }}))
}

/// `crewmate_action_answer_voice`: the transcript against the card task `task_id` of crewmate `id` waits on.
pub(crate) fn answer_action(
    hub: &Hub,
    id: &str,
    task_id: &str,
    transcript: &str,
    origin: &str,
) -> Result<Value, String> {
    let pane = pane::key(id, task_id);
    if !owner(hub, origin) {
        let what = json!({"task_id": task_id});
        return Ok(refused(hub, &pane, origin, transcript, "untrusted", what));
    }
    let task = hub
        .crew
        .store
        .task(id, task_id)
        .filter(|t| t.state == TaskState::WaitingAction)
        .ok_or_else(|| format!("no waiting-action task {task_id} for crewmate {id}"))?;
    let Some(TaskAction::Choose { options, .. }) = &task.action else {
        // Secrets, sign-ins, refusals, proofs, automations: each is answered on screen.
        return Ok(needs_tap(None));
    };
    let Some(picked) = voice_answer::choice(transcript, options) else {
        return Ok(json!({"verdict": "unclear"}));
    };
    let choice = options[picked].clone();
    super::handlers::tasks::action_answer(
        hub,
        id.to_string(),
        task_id.to_string(),
        choice.clone(),
        vec![],
    )?;
    hub.crew.audit.record(
        Some(id),
        "card_answered",
        json!({"task_id": task_id, "card": "choose", "choice": choice, "via": "voice",
               "device": origin, "transcript": clip(transcript)}),
    );
    Ok(json!({"verdict": "choice", "choice": choice}))
}

/// What a crewmate has waiting for an answer, for a voice note that arrives with no card named (Telegram).
pub(crate) enum Waiting {
    Nothing,
    /// One permission prompt: its pane and request.
    Approval {
        pane: String,
        req: String,
    },
    /// One choice card: its task.
    Choice {
        task: String,
    },
    /// More than one thing waits: a spoken answer would be a guess.
    Several,
}

/// The prompts and the choice cards `crewmate_id` waits on.
pub(crate) fn waiting(hub: &Hub, crewmate_id: &str) -> Waiting {
    let mut found: Vec<Waiting> = hub
        .agents
        .pending_approvals()
        .into_iter()
        .filter(|e| {
            e["pane_key"]
                .as_str()
                .is_some_and(|p| pane::is_pane_of(p, crewmate_id))
        })
        .filter_map(|e| {
            Some(Waiting::Approval {
                pane: e["pane_key"].as_str()?.to_string(),
                req: e["req"].as_str()?.to_string(),
            })
        })
        .collect();
    found.extend(
        hub.crew
            .store
            .read_tasks(crewmate_id, 20)
            .into_iter()
            .filter(|t| {
                t.state == TaskState::WaitingAction
                    && matches!(t.action, Some(TaskAction::Choose { .. }))
                    && hub.crew.action_waiting(&t.id)
            })
            .map(|t| Waiting::Choice { task: t.id }),
    );
    match found.len() {
        0 => Waiting::Nothing,
        1 => found.remove(0),
        _ => Waiting::Several,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #648: only the owner's own clients answer by voice. A crewmate's own connection, a chat that is not linked
    // and any other name never do, whatever they said.
    #[test]
    fn only_the_owners_clients_are_trusted() {
        let linked = |chat: i64| chat == 42;
        assert!(trusted("desktop", linked));
        assert!(trusted("phone:abc123", linked));
        assert!(trusted("telegram:42", linked));
        assert!(!trusted("telegram:99", linked), "a chat that is not linked");
        assert!(!trusted("telegram:", linked));
        assert!(!trusted("telegram:abc", linked));
        assert!(!trusted("crewmate:c1", linked), "a crewmate's own process");
        assert!(!trusted("", linked));
        assert!(!trusted("Desktop", linked));
    }
}
