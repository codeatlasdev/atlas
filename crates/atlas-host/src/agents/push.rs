//! Push notifications for plain agent panes (omp, Claude Code, Codex running in a terminal on this
//! computer), so a phone paired only to a Mac hears that an agent finished or needs it, without the
//! Atlas window open and without the phone connected.
//!
//! Crewmate panes are not handled here: they push through [`crate::crew`] (`task_done`, `needs_you`).
//!
//! What goes to the relay is [`PushMsg`]: the event `kind`, the pane key and a short title
//! (`<agent> · <project folder>`). Never the transcript, the prompt, the last message or the
//! session title (omp names a session after what was asked).
//!
//! One push per *transition*, never per event: [`Tracker`] watches the `agent_status` stream and
//! speaks when a pane goes Working→Done/Idle ([`KIND_DONE`]) or enters Blocked/Waiting
//! ([`KIND_NEEDS_YOU`]), then holds that pane quiet for [`COOLDOWN_MS`]. A phone that is connected
//! right now is not notified (it sees the change live); a phone that disconnected is.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};

use serde::Serialize;
use serde_json::Value;

use super::{AgentStatus, State};
use crate::crew::PushToken;
use crate::server::Hub;

/// An agent finished a turn (Working → Done, or its program exited while it worked).
pub const KIND_DONE: &str = "agent_done";
/// An agent waits for a permission answer (Blocked) or for the human's reply to its question (Waiting).
pub const KIND_NEEDS_YOU: &str = "agent_needs_you";

/// Per pane and kind, how long after one push the next of the same kind is held back. A turn that
/// ends and restarts within seconds (an agent chaining turns) is one piece of news.
pub const COOLDOWN_MS: u64 = 30_000;

/// Longest title sent (characters): the relay cuts at the same length.
const TITLE_MAX: usize = 60;

/// What a phone is told: the `data` of the notification, plus nothing else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PushMsg {
    pub kind: &'static str,
    pub pane: String,
    pub title: String,
}

/// Per-pane memory of what was last seen and said.
#[derive(Default)]
struct PaneTrack {
    state: Option<State>,
    /// The pane worked since it was last announced done: Done/Idle without it is not news (a hook
    /// replay, a pane that just opened and was closed).
    worked: bool,
    last_done: Option<u64>,
    last_needs: Option<u64>,
}

/// The transition detector. Pure: time comes in as an argument.
#[derive(Default)]
pub struct Tracker {
    panes: HashMap<String, PaneTrack>,
}

impl Tracker {
    /// Feed one `agent_status`. `Some(kind)` when this very change is worth a push.
    pub fn on_status(&mut self, pane: &str, state: State, now: u64) -> Option<&'static str> {
        let t = self.panes.entry(pane.to_string()).or_default();
        let before = t.state.replace(state);
        if before == Some(state) {
            return None; // heartbeat or a change nothing here shows (a title, a model)
        }
        let say = |last: &mut Option<u64>, kind| {
            if last.is_some_and(|l| now.saturating_sub(l) < COOLDOWN_MS) {
                return None;
            }
            *last = Some(now);
            Some(kind)
        };
        match state {
            State::Working => {
                t.worked = true;
                None
            }
            State::Blocked | State::Waiting => {
                // Blocked ↔ Waiting is the same wait; entering it from anything else is the news.
                if matches!(before, Some(State::Blocked | State::Waiting)) {
                    return None;
                }
                say(&mut t.last_needs, KIND_NEEDS_YOU)
            }
            State::Done | State::Idle => {
                let worked = std::mem::take(&mut t.worked);
                if worked && before != Some(State::Done) {
                    say(&mut t.last_done, KIND_DONE)
                } else {
                    None
                }
            }
        }
    }

    /// Adopt `state` as the pane's current one without announcing anything (a start-up or a missed
    /// stretch of events: we cannot tell what happened, so we say nothing).
    pub fn adopt(&mut self, pane: &str, state: State) {
        self.panes.entry(pane.to_string()).or_default().state = Some(state);
    }
}

/// `<agent> · <folder>`, at most [`TITLE_MAX`] characters. `cwd` is the terminal's working folder.
pub fn title_for(agent: &str, cwd: Option<&str>) -> String {
    let folder = cwd
        .and_then(|c| Path::new(c.trim_end_matches('/')).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let t = if folder.is_empty() {
        agent.to_string()
    } else {
        format!("{agent} · {folder}")
    };
    t.chars().take(TITLE_MAX).collect()
}

/// Tokens that should hear about a change right now: every registered one except the phones that
/// are connected (they see it live). A token with no phone recorded (one registered by a client that
/// is not a phone) is always eligible: nothing says that phone is looking.
pub fn audience(tokens: &[PushToken], live: &[String]) -> Vec<PushToken> {
    tokens
        .iter()
        .filter(|t| t.node_id.is_empty() || !live.contains(&t.node_id))
        .cloned()
        .collect()
}

/// How a message leaves the process: the relay in production, a channel in tests.
#[derive(Clone)]
pub struct Sender(pub Arc<dyn Fn(Vec<PushToken>, PushMsg) + Send + Sync>);

impl std::fmt::Debug for Sender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sender")
    }
}

/// The production sender: relay notify, dropping the tokens the relay refuses for good.
pub(crate) fn relay_sender(hub: Weak<Hub>) -> Sender {
    Sender(Arc::new(move |tokens, msg| {
        let hub = hub.clone();
        tokio::spawn(async move {
            let expired = crate::crew::push::notify_agent(&tokens, &msg).await;
            if expired.is_empty() {
                return;
            }
            if let Some(h) = hub.upgrade() {
                h.crew.drop_tokens(&expired);
            }
        });
    }))
}

/// Start the emitter for `hub`. Subscribes before returning, so no status change after this call
/// can be missed (a broadcast receiver only sees what is sent after `subscribe`).
pub(crate) fn observe(hub: &Arc<Hub>, send: Sender) {
    let mut rx = hub.agents.subscribe();
    let hub = Arc::downgrade(hub);
    tokio::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        let mut tracker = Tracker::default();
        loop {
            let ev = match rx.recv().await {
                Ok(v) => v,
                Err(RecvError::Lagged(_)) => {
                    let Some(h) = hub.upgrade() else { return };
                    for (pane, st) in h.agents.all() {
                        tracker.adopt(&pane, st.state);
                    }
                    continue;
                }
                Err(RecvError::Closed) => return,
            };
            let Some(h) = hub.upgrade() else { return };
            handle_event(&h, &mut tracker, &ev, &send);
        }
    });
}

fn handle_event(hub: &Hub, tracker: &mut Tracker, ev: &Value, send: &Sender) {
    if ev["ev"] != "agent_status" {
        return;
    }
    let (Some(pane), Ok(st)) = (
        ev["pane_key"].as_str(),
        serde_json::from_value::<AgentStatus>(ev["status"].clone()),
    ) else {
        return;
    };
    if crate::crew::pane::is_crewmate_pane(pane) {
        return; // crewmates push through the crew hub
    }
    let Some(kind) = tracker.on_status(pane, st.state, super::now_ms()) else {
        return;
    };
    let tokens: Vec<PushToken> = hub.crew.push_tokens.lock().clone();
    let tokens = audience(&tokens, &hub.mobile.live_node_ids());
    if tokens.is_empty() {
        return;
    }
    let cwd = hub
        .sessions_snapshot()
        .into_iter()
        .find(|s| s.meta.scope.pane_key.as_deref() == Some(pane))
        .map(|s| s.meta.cwd.clone());
    (send.0)(
        tokens,
        PushMsg {
            kind,
            pane: pane.to_string(),
            title: title_for(&st.agent, cwd.as_deref()),
        },
    );
}

#[cfg(test)]
mod tests;
