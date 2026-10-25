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
pub mod tree;

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
    /// The session's title as the agent named it (omp's `title` entry), for lists.
    #[serde(default)]
    pub title: Option<String>,
}

/// A prompt waiting for a remote answer: (pane, its `agent_approval` event, answer channel).
type Pending = (String, Value, oneshot::Sender<bool>);

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
    /// Prompts raised by ANOTHER hostd that this one mirrors (the lab lane, [`crate::lab`]):
    /// `req` → its `agent_approval` event. Listed and pushed like a local one, but answered
    /// through that daemon, so there is no answer channel here.
    mirrored: Mutex<HashMap<String, Value>>,
}

impl Default for AgentHub {
    fn default() -> Self {
        Self {
            panes: Mutex::default(),
            by_cwd: Mutex::default(),
            port: Mutex::new(0),
            tx: broadcast::channel(256).0,
            pending: Mutex::default(),
            mirrored: Mutex::default(),
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

/// A subagent's transcript lives under its parent's session dir (`<parent>.jsonl` →
/// `<parent>/<Name>.jsonl`, nested ones deeper).
fn is_subagent_file(parent: Option<&str>, file: Option<&str>) -> bool {
    let (Some(parent), Some(file)) = (parent, file) else {
        return false;
    };
    let dir = parent.strip_suffix(".jsonl").unwrap_or(parent);
    file.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// `/private/tmp/x/` and `/tmp/x` are the same directory on macOS.
fn norm_cwd(p: &str) -> String {
    let p = p.trim_end_matches('/');
    p.strip_prefix("/private").unwrap_or(p).to_string()
}

/// Whether a phone may type into an agent in `state`: idle or done, always; while the agent
/// works only as steering (`steer`) and only for omp, which queues a message typed mid-turn
/// for the running turn. Claude Code and Codex would take it as the next prompt or garble
/// the turn, so they keep the old rule. `Err` is the refusal text.
pub fn send_allowed(state: State, agent: &str, steer: bool) -> Result<(), &'static str> {
    match state {
        State::Idle | State::Done => Ok(()),
        _ if steer && agent == "omp" => Ok(()),
        _ => Err("agent is working"),
    }
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
            title: None,
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
        let file = str_of("session_file");
        // omp runs this extension in its subagents too, in the same terminal: their hooks
        // carry the parent's pane key and a session file inside the parent's session dir.
        // They must not rename, retarget or finish the pane; only a prompt that needs the
        // human (an approval) gets through.
        let sub = is_subagent_file(st.session_file.as_deref(), file.as_deref());
        if sub {
            if normalize(Some(st.state), p) == Some(State::Blocked) && st.state != State::Blocked {
                st.state = State::Blocked;
                st.state_started_at = now;
            }
        } else {
            st.session_id = str_of("session_id").or(st.session_id.take());
            if file.is_some() && file != st.session_file {
                st.title = None;
            }
            st.session_file = file.or(st.session_file.take());
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
            if let Some(f) = &st.session_file
                && (st.title.is_none() || matches!(p["event"].as_str(), Some("agent_end" | "stop")))
            {
                st.title = history::title(f).or(st.title.take());
            }
        }
        st.updated_at = now;
        let out = (pane, st.clone());
        drop(panes);
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
        // Never make an agent wait on a phone that is not there or may not approve.
        if !someone_can_answer {
            return None;
        }
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

    /// Prompts still waiting, as the `agent_approval` events that announced them (this daemon's
    /// own, and the ones it mirrors).
    pub fn pending_approvals(&self) -> Vec<Value> {
        let mut all: Vec<Value> = self
            .pending
            .lock()
            .values()
            .map(|(_, ev, _)| ev.clone())
            .collect();
        all.extend(self.mirrored.lock().values().cloned());
        all
    }

    /// Mirror one pane's status from the daemon this one follows (the lab lane): stored and
    /// pushed as `agent_status`. An older update never replaces a newer one, so events that
    /// were buffered while the seed was read cannot roll a pane back.
    pub fn mirror_status(&self, pane: &str, st: AgentStatus) {
        let mut panes = self.panes.lock();
        if panes
            .get(pane)
            .is_some_and(|cur| cur.updated_at > st.updated_at)
        {
            return;
        }
        panes.insert(pane.to_string(), st.clone());
        drop(panes);
        self.push_status(pane, &st);
    }

    /// Replace everything with the followed daemon's snapshot (`agent_status_all`): panes that
    /// are gone there are gone here (a status push has no removal event, so this is how they
    /// leave), and prompts it no longer holds are dropped (their `agent_approval_done` is
    /// pushed, so a phone clears them).
    pub fn mirror_all(&self, panes: HashMap<String, AgentStatus>, approvals: Vec<Value>) {
        self.panes.lock().retain(|k, _| panes.contains_key(k));
        for (pane, st) in panes {
            self.mirror_status(&pane, st);
        }
        let keep: Vec<String> = approvals
            .iter()
            .filter_map(|e| e["req"].as_str().map(str::to_string))
            .collect();
        let stale: Vec<(String, Value)> = self
            .mirrored
            .lock()
            .iter()
            .filter(|(r, _)| !keep.contains(r))
            .map(|(r, e)| (r.clone(), e.clone()))
            .collect();
        for (req, ev) in stale {
            self.mirror_approval_done(&req, ev["pane_key"].as_str().unwrap_or_default());
        }
        for ev in approvals {
            self.mirror_approval(ev);
        }
    }

    /// A prompt raised by the followed daemon: list it and push it.
    pub fn mirror_approval(&self, ev: Value) {
        let Some(req) = ev["req"].as_str() else {
            return;
        };
        if self
            .mirrored
            .lock()
            .insert(req.to_string(), ev.clone())
            .is_none()
        {
            let _ = self.tx.send(ev);
        }
    }

    /// The followed daemon's prompt `req` was answered, timed out or withdrawn.
    pub fn mirror_approval_done(&self, req: &str, pane: &str) {
        if self.mirrored.lock().remove(req).is_some() {
            let _ = self
                .tx
                .send(json!({"ev": "agent_approval_done", "req": req, "pane_key": pane}));
        }
    }

    pub fn session_file(&self, pane: &str) -> Option<String> {
        self.panes.lock().get(pane)?.session_file.clone()
    }

    pub fn state_of(&self, pane: &str) -> Option<State> {
        self.panes.lock().get(pane).map(|s| s.state)
    }

    pub fn status_of(&self, pane: &str) -> Option<AgentStatus> {
        self.panes.lock().get(pane).cloned()
    }

    pub fn all(&self) -> HashMap<String, AgentStatus> {
        self.panes.lock().clone()
    }
}
