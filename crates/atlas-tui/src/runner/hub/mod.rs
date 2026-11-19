//! The hub: memory of the run (log tail, side events, action runs) plus the
//! attached clients. Every manager event is recorded and fanned out as one JSON
//! line; a `subscribe` snapshot is built in the same turn that registers the
//! client's stream, which is what keeps snapshot and stream gap-free.

mod actions;
mod control;
mod subscriber;

use std::collections::VecDeque;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::wire::{log_json, service_json, state_name, tunnel_json};
use crate::runtime::clock;
use crate::runtime::logs::LogBuffer;
use crate::runtime::manager::{ManagerEvent, ServiceManager};
pub(in crate::runner) use actions::ACTION_RUNS;
#[cfg(test)]
pub(in crate::runner) use actions::ACTION_TAIL_LINES;
use actions::ActionRun;
use subscriber::Subscriber;

/// Log lines kept for a newly attached client (per service ring, like the TUI's).
const LOG_TAIL_PER_SERVICE: usize = 1000;
/// Raw side events replayed to a new client (the sidebar shows far fewer).
pub(in crate::runner) const EVENT_HISTORY: usize = 50;

/// Memory of the run plus the attached clients.
pub struct Hub {
    project: String,
    logs: LogBuffer,
    /// Recent `state`/`note`/`alert`/`tunnel` events, replayed to new clients.
    history: VecDeque<Value>,
    subscribers: Vec<Subscriber>,
    /// Last `rows` payload sent; a new one goes out only when it differs.
    last_rows: String,
    started: bool,
    /// Non-session action in flight (`label`, last output line).
    operation: Option<(String, String)>,
    /// Recent action runs, oldest first, for clients that attach later.
    action_runs: VecDeque<ActionRun>,
    mode: &'static str,
}

impl Hub {
    pub fn new(project: &str) -> Self {
        Self {
            project: project.to_string(),
            logs: LogBuffer::new(LOG_TAIL_PER_SERVICE),
            history: VecDeque::with_capacity(EVENT_HISTORY),
            subscribers: Vec::new(),
            last_rows: String::new(),
            started: false,
            operation: None,
            action_runs: VecDeque::with_capacity(ACTION_RUNS),
            mode: "runner",
        }
    }

    pub fn clients(&self) -> usize {
        self.subscribers.len()
    }

    fn broadcast(&mut self, ev: &Value) {
        if self.subscribers.is_empty() {
            return;
        }
        let line = ev.to_string();
        let is_log = ev["t"] == "log";
        self.subscribers.retain_mut(|s| s.send(&line, is_log));
    }

    fn remember(&mut self, ev: Value) {
        if self.history.len() >= EVENT_HISTORY {
            self.history.pop_front();
        }
        self.broadcast(&ev);
        self.history.push_back(ev);
    }

    /// Record one manager event and push it to subscribers. Called by the run loop
    /// *before* the manager reacts, so `state` facts describe the transition.
    pub fn on_event(&mut self, manager: &ServiceManager, event: &ManagerEvent) {
        match event {
            ManagerEvent::LogLine { name, line, stream } => {
                let ev = log_json(self.logs.push(name.clone(), line.clone(), *stream));
                self.broadcast(&ev);
            }
            ManagerEvent::ServiceRestarted { name } => {
                if self.logs.filter_service(name).is_empty() {
                    return;
                }
                self.logs.push_restart(name.clone());
                if let Some(e) = self.logs.last() {
                    self.broadcast(&log_json(e));
                }
            }
            ManagerEvent::StateChanged { name, state } => {
                let mut ev = service_json(manager, name, state);
                ev["t"] = "state".into();
                self.remember(ev);
            }
            // `message` is the English every client always read; `code` and `args` are additive (an attached TUI or
            // the desktop writes the sentence in its own language, an older client ignores them).
            ManagerEvent::ServiceNote { name, note } => {
                self.remember(json!({
                    "t": "note", "name": name, "message": note.message,
                    "code": note.code, "args": note.args,
                }));
            }
            ManagerEvent::CriticalAlert { service, note } => {
                self.remember(json!({
                    "t": "alert", "service": service, "message": note.message,
                    "code": note.code, "args": note.args,
                }));
            }
            ManagerEvent::TunnelState { state } => {
                self.remember(
                    json!({ "t": "tunnel", "state": state_name(state), "tunnel": tunnel_json(manager) }),
                );
            }
            ManagerEvent::ActionProgress { label, line } => {
                if let Some(op) = self.operation.as_mut()
                    && op.0 == *label
                {
                    op.1 = line.clone();
                }
                self.action_run(label).push(line);
                self.broadcast(&json!({ "t": "action_progress", "label": label, "line": line }));
            }
            ManagerEvent::ActionCompleted { label, ok, .. } => {
                if self.operation.as_ref().is_some_and(|o| o.0 == *label) {
                    self.operation = None;
                }
                // Empty label = the manager's resume ping after a session, not an action.
                if !label.is_empty() {
                    self.action_run(label).done = Some((clock::now_ms(), *ok));
                }
                self.broadcast(&json!({ "t": "action_done", "label": label, "ok": ok }));
            }
            ManagerEvent::AllStarted => {
                self.started = true;
                self.broadcast(&json!({ "t": "started" }));
            }
            _ => {}
        }
    }

    /// Per-second upkeep: drop clients that hung up (so `--status` counts only live
    /// ones), then push the `rows` event when anything a row shows changed.
    pub fn tick(&mut self, manager: &mut ServiceManager) {
        self.subscribers.retain(|s| !s.tx.is_closed());
        manager.set_clients(self.clients());
        if self.subscribers.is_empty() {
            return;
        }
        let ev = self.rows_event(manager);
        let text = ev.to_string();
        if text != self.last_rows {
            self.last_rows = text;
            self.broadcast(&ev);
        }
    }

    fn rows_event(&self, manager: &ServiceManager) -> Value {
        let rows: Vec<Value> = manager
            .services()
            .iter()
            .map(|s| service_json(manager, &s.name, &s.state))
            .collect();
        json!({ "t": "rows", "services": rows, "tunnel": tunnel_json(manager) })
    }

    /// The unfinished run of `label` (a label has at most one in flight), started on
    /// first need: output and completion may arrive without a prior `start`.
    fn action_run(&mut self, label: &str) -> &mut ActionRun {
        let open = self
            .action_runs
            .iter()
            .rposition(|r| r.label == label && r.done.is_none());
        let i = open.unwrap_or_else(|| {
            if self.action_runs.len() >= ACTION_RUNS {
                self.action_runs.pop_front();
            }
            self.action_runs.push_back(ActionRun::new(label));
            self.action_runs.len() - 1
        });
        &mut self.action_runs[i]
    }

    /// Everything a new client needs; registering `stream` in the same call is what
    /// makes the following events gap-free.
    pub(super) fn subscribe(
        &mut self,
        manager: &ServiceManager,
        stream: mpsc::Sender<String>,
    ) -> Value {
        let mut rows = self.rows_event(manager);
        let logs: Vec<Value> = self
            .logs
            .all()
            .iter()
            .map(|e| {
                let mut v = log_json(e);
                v["name"] = e.service.clone().into();
                v.as_object_mut().map(|o| o.remove("t"));
                v
            })
            .collect();
        let operation = self
            .operation
            .as_ref()
            .map(|(label, detail)| json!({ "label": label, "detail": detail }));
        let action_runs: Vec<Value> = self.action_runs.iter().map(ActionRun::json).collect();
        self.subscribers.push(Subscriber {
            tx: stream,
            skipped: 0,
        });
        // The client's first `rows` event must not be skipped for being "unchanged".
        self.last_rows.clear();
        json!({
            "project": self.project,
            "pid": std::process::id(),
            "mode": self.mode,
            "version": env!("CARGO_PKG_VERSION"),
            "started": self.started,
            "services": rows["services"].take(),
            "tunnel": rows["tunnel"].take(),
            "logs": logs,
            "events": self.history,
            "operation": operation,
            "action_runs": action_runs,
        })
    }

    /// Tell every client the runner is going away (they then exit their TUI).
    pub fn bye(&mut self) {
        self.broadcast(&json!({ "t": "bye" }));
        self.subscribers.clear();
    }
}
