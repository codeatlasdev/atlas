//! Coding-agent state per terminal pane, fed by lifecycle hooks.
//!
//! Lives in hostd (not the app) so the state survives the window closing and
//! every client (desktop, mobile) sees the same thing. Hooks reach it through
//! the loopback HTTP server in [`hook_server`]; clients get `agent_status`
//! pushes and the `agent_*` commands.

pub mod history;
pub mod hook_server;
pub mod integrations;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Working,
    Blocked,
    Waiting,
    Done,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentStatus {
    pub state: State,
    pub agent: String,
    pub session_id: Option<String>,
    pub session_file: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub last_message: Option<String>,
    pub updated_at: u64,
    pub state_started_at: u64,
    pub worktree_id: Option<String>,
    pub tab_id: Option<String>,
}

/// A prompt waiting for a remote answer: (pane, its `agent_approval` event, answer channel).
type Pending = (String, Value, oneshot::Sender<bool>);

type BotAnswerFn = Box<dyn Fn(&str) -> bool + Send>;

pub struct AgentHub {
    panes: Mutex<HashMap<String, AgentStatus>>,
    /// `agent|cwd` → pane key, for hooks that arrive without one (Codex runs
    /// hooks in a shared daemon that lacks the terminal's env).
    by_cwd: Mutex<HashMap<String, String>>,
    /// Hook server port (0 until bound).
    pub port: Mutex<u16>,
    /// Push events for clients (`agent_status`, `agent_approval`, `agent_approval_done`).
    tx: broadcast::Sender<Value>,
    /// Permission prompts waiting for a remote answer: `req` → (pane, the `agent_approval`
    /// event as pushed, answer channel). The event is kept so a client that subscribes
    /// after the push can still list it.
    pending: Mutex<HashMap<String, Pending>>,
    /// For bot panes: callback that returns true when telegram or push tokens can receive approvals.
    bot_pane_can_answer: Mutex<Option<BotAnswerFn>>,
}

impl Default for AgentHub {
    fn default() -> Self {
        Self {
            panes: Mutex::default(),
            by_cwd: Mutex::default(),
            port: Mutex::new(0),
            tx: broadcast::channel(256).0,
            pending: Mutex::default(),
            bot_pane_can_answer: Mutex::new(None),
        }
    }
}

impl AgentHub {
    /// What a successor daemon needs to keep every agent: per-pane state and the cwd
    /// bindings. Pending prompts stay behind: their hook connection is to this process.
    pub fn snapshot(&self) -> Value {
        json!({"panes": *self.panes.lock(), "by_cwd": *self.by_cwd.lock()})
    }

    /// Take over a predecessor's [`snapshot`](Self::snapshot); anything unreadable is
    /// skipped (an older daemon sends nothing).
    pub fn restore(&self, v: &Value) {
        if let Ok(p) = serde_json::from_value::<HashMap<String, AgentStatus>>(v["panes"].clone()) {
            self.panes.lock().extend(p);
        }
        if let Ok(b) = serde_json::from_value::<HashMap<String, String>>(v["by_cwd"].clone()) {
            self.by_cwd.lock().extend(b);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Map one lifecycle event to the next state (`None` = keep).
pub fn normalize(cur: Option<State>, p: &Value) -> Option<State> {
    let ev = p["event"].as_str().unwrap_or_default();
    match ev {
        "tool_approval_requested" | "permission_request" => Some(State::Blocked),
        "tool_call" if p["tool_name"] == "ask" => Some(State::Waiting),
        "agent_end" | "stop" if p["will_continue"] != true => Some(State::Done),
        "session_shutdown" | "session_end" => Some(State::Idle),
        // An approval prompt holds Blocked until it resolves or a new turn starts.
        "tool_execution_start" | "tool_execution_end" | "message_end"
            if cur == Some(State::Blocked) =>
        {
            None
        }
        "before_agent_start"
        | "agent_start"
        | "tool_execution_start"
        | "tool_execution_end"
        | "message_end"
        | "tool_approval_resolved"
        | "user_prompt_submit"
        | "pre_tool_use"
        | "post_tool_use" => Some(State::Working),
        _ => None,
    }
}

/// `/private/tmp/x/` and `/tmp/x` are the same directory on macOS.
fn norm_cwd(p: &str) -> String {
    let p = p.trim_end_matches('/');
    p.strip_prefix("/private").unwrap_or(p).to_string()
}

impl AgentHub {
    pub fn bind(&self, pane: &str, agent: &str, cwd: &str) {
        self.by_cwd
            .lock()
            .insert(format!("{agent}|{}", norm_cwd(cwd)), pane.to_string());
    }

    pub fn apply(&self, agent: &str, body: &Value) -> Option<(String, AgentStatus)> {
        let pane = match body["pane_key"].as_str() {
            Some(p) => p.to_string(),
            None => {
                let cwd = norm_cwd(body["payload"]["cwd"].as_str()?);
                self.by_cwd.lock().get(&format!("{agent}|{cwd}"))?.clone()
            }
        };
        let p = &body["payload"];
        let now = now_ms();
        let mut panes = self.panes.lock();
        let st = panes.entry(pane.clone()).or_insert_with(|| AgentStatus {
            state: State::Idle,
            agent: agent.to_string(),
            session_id: None,
            session_file: None,
            model: None,
            prompt: None,
            last_message: None,
            updated_at: now,
            state_started_at: now,
            worktree_id: None,
            tab_id: None,
        });
        let str_of = |k: &str| p[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
        st.worktree_id = body["worktree_id"]
            .as_str()
            .map(str::to_string)
            .or(st.worktree_id.take());
        st.tab_id = body["tab_id"]
            .as_str()
            .map(str::to_string)
            .or(st.tab_id.take());
        st.session_id = str_of("session_id").or(st.session_id.take());
        st.session_file = str_of("session_file").or(st.session_file.take());
        st.model = str_of("model").or(st.model.take());
        if let Some(pr) = str_of("prompt") {
            st.prompt = Some(pr);
        }
        if let Some(t) = str_of("text") {
            st.last_message = Some(t.chars().take(2000).collect());
        }
        if let Some(next) = normalize(Some(st.state), p)
            && next != st.state
        {
            st.state = next;
            st.state_started_at = now;
        }
        st.updated_at = now;
        let out = (pane, st.clone());
        drop(panes);
        let payload_ev = p["event"].as_str().unwrap_or("");
        if payload_ev == "pre_tool_use" || payload_ev == "post_tool_use" {
            let _ = self.tx.send(json!({
                "ev": "agent_step",
                "pane_key": out.0,
                "tool_name": p["tool_name"],
                "tool_input": p["tool_input"],
            }));
        }
        self.push_status(&out.0, &out.1);
        Some(out)
    }

    /// The pane's program exited: back to Idle (the pane stays listed).
    pub fn idle(&self, pane: &str) -> Option<AgentStatus> {
        let mut panes = self.panes.lock();
        let st = panes.get_mut(pane)?;
        st.state = State::Idle;
        st.state_started_at = now_ms();
        let st = st.clone();
        drop(panes);
        self.push_status(pane, &st);
        Some(st)
    }

    fn push_status(&self, pane: &str, st: &AgentStatus) {
        let _ = self
            .tx
            .send(json!({"ev": "agent_status", "pane_key": pane, "status": st}));
    }

    /// Push events (one JSON object each, with `ev`) for every connected client.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.tx.subscribe()
    }

    /// Wait up to `wait` for a remote answer to a permission prompt in `pane`.
    /// `None` at once unless `someone_can_answer` (a connected phone with `approve`).
    pub async fn ask(
        &self,
        pane: &str,
        agent: &str,
        payload: &Value,
        wait: Duration,
        someone_can_answer: bool,
    ) -> Option<bool> {
        // Bot panes can also be answered by telegram or push.
        let is_bot = pane.starts_with("bot:");
        let bot_can = is_bot
            && self
                .bot_pane_can_answer
                .lock()
                .as_deref()
                .is_some_and(|f| f(pane));
        if !someone_can_answer && !bot_can {
            return None;
        }
        let wait = if is_bot {
            std::time::Duration::from_secs(30 * 60) // BOT_DECISION_WAIT
        } else {
            wait
        };
        let req = crate::paths::random_hex(6).ok()?;
        let (otx, orx) = oneshot::channel();
        let event = json!({
            "ev": "agent_approval",
            "pane_key": pane,
            "agent": agent,
            "req": req,
            "tool_name": payload["tool_name"],
            "tool_input": payload["tool_input"],
        });
        self.pending
            .lock()
            .insert(req.clone(), (pane.to_string(), event.clone(), otx));
        let _ = self.tx.send(event);
        let res = tokio::time::timeout(wait, orx)
            .await
            .ok()
            .and_then(Result::ok);
        self.pending.lock().remove(&req);
        let _ = self
            .tx
            .send(json!({"ev": "agent_approval_done", "req": req, "pane_key": pane}));
        res
    }

    /// Answer a pending prompt. A `req` that is unknown, expired or belongs to another pane
    /// is refused, so a stale tap can never answer a newer prompt.
    pub fn approve(&self, pane: &str, req: &str, allow: bool) -> Result<(), String> {
        let mut p = self.pending.lock();
        match p.get(req) {
            Some((owner, _, _)) if owner == pane => {}
            _ => return Err("no such pending request".into()),
        }
        let (_, _, tx) = p.remove(req).expect("checked above");
        tx.send(allow)
            .map_err(|_| "no such pending request".to_string())
    }

    /// Prompts still waiting, as the `agent_approval` events that announced them.
    pub fn pending_approvals(&self) -> Vec<Value> {
        self.pending
            .lock()
            .values()
            .map(|(_, ev, _)| ev.clone())
            .collect()
    }

    pub fn session_file(&self, pane: &str) -> Option<String> {
        self.panes.lock().get(pane)?.session_file.clone()
    }

    pub fn state_of(&self, pane: &str) -> Option<State> {
        self.panes.lock().get(pane).map(|s| s.state)
    }

    pub fn all(&self) -> HashMap<String, AgentStatus> {
        self.panes.lock().clone()
    }

    pub fn set_bot_pane_can_answer(&self, f: BotAnswerFn) {
        *self.bot_pane_can_answer.lock() = Some(f);
    }
}
