//! What each running task ran, for the guards (`guard`): the commands the agent reported through
//! its hooks, with the output when the hook carried it. One subscriber sees every crewmate pane, so
//! a task in a terminal and one on a headless engine are covered the same way (the same reason the
//! audit observes the hub instead of each runner).
//!
//! In memory and per task: it is the evidence of one run, not a log. It is dropped when the task
//! ends. A restart loses it, and a claim made after one is then judged without it (and so removed),
//! which is the safe direction.

use std::collections::HashMap;
use std::sync::Weak;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;

use super::guard::Ran;
use crate::crew::pane;
use crate::server::Hub;

/// Commands kept per task: the last ones, which are the ones a final claim rests on.
const MAX_COMMANDS: usize = 300;
/// Bytes of output kept per command (the tail: test runners print their verdict last).
const MAX_OUTPUT: usize = 16 * 1024;

#[derive(Default)]
pub(crate) struct Tape(Mutex<HashMap<String, Vec<Ran>>>);

impl Tape {
    /// A tool call of `task_id`. Only shell-like calls carry a command; the rest are not evidence of
    /// anything the guards ask for.
    pub(crate) fn record(&self, task_id: &str, tool_input: &Value, tool_output: &Value) {
        let Some(command) = tool_input["command"]
            .as_str()
            .or(tool_input["cmd"].as_str())
        else {
            return;
        };
        let output = output_text(tool_output);
        let mut map = self.0.lock();
        let list = map.entry(task_id.to_string()).or_default();
        // A hook that reports the call twice (before and after it) is one command: the second
        // report carries the output.
        if let Some(last) = list.last_mut()
            && last.command == command
            && last.output.is_none()
        {
            last.output = output;
            return;
        }
        if list.len() >= MAX_COMMANDS {
            list.remove(0);
        }
        list.push(Ran {
            command: command.to_string(),
            output,
        });
    }

    /// What `task_id` ran so far.
    pub(crate) fn of(&self, task_id: &str) -> Vec<Ran> {
        self.0.lock().get(task_id).cloned().unwrap_or_default()
    }

    /// The task ended (or runs again from scratch): forget its commands.
    pub(crate) fn forget(&self, task_id: &str) {
        self.0.lock().remove(task_id);
    }
}

/// The text of a tool output: a string, or an object with `stdout`/`stderr`/`output`/`content`.
fn output_text(v: &Value) -> Option<String> {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Object(_) => ["stdout", "stderr", "output", "content", "text"]
            .iter()
            .filter_map(|k| v[*k].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    if text.trim().is_empty() {
        return None;
    }
    // The tail, on a char boundary.
    let start = text.len().saturating_sub(MAX_OUTPUT);
    let start = (start..=text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0);
    Some(text[start..].to_string())
}

/// Start collecting. The receiver is taken before this returns, so a task started right after is
/// covered (a broadcast receiver only sees what is sent after it subscribes).
pub(crate) fn observe(hub_w: Weak<Hub>) {
    let Some(h) = hub_w.upgrade() else { return };
    let mut rx = h.agents.subscribe();
    drop(h);
    tokio::spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(v) => v,
                Err(RecvError::Lagged(n)) => {
                    // Evidence was dropped; a claim that needed it is removed, never invented.
                    eprintln!("atlas crew: command tape: {n} agent events were dropped under load");
                    continue;
                }
                Err(RecvError::Closed) => return,
            };
            if ev["ev"] != "agent_step" {
                continue;
            }
            let Some(pane_key) = ev["pane_key"]
                .as_str()
                .filter(|p| pane::is_crewmate_pane(p))
            else {
                continue;
            };
            let Some(task_id) = pane_key.rsplit(':').next() else {
                continue;
            };
            let Some(h) = hub_w.upgrade() else { return };
            h.crew
                .tape
                .record(task_id, &ev["tool_input"], &ev["tool_output"]);
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_call_reported_before_and_after_is_one_command_with_its_output() {
        let t = Tape::default();
        t.record("t1", &json!({"command": "cargo test"}), &Value::Null);
        t.record(
            "t1",
            &json!({"command": "cargo test"}),
            &json!({"stdout": "test result: ok. 3 passed"}),
        );
        let ran = t.of("t1");
        assert_eq!(ran.len(), 1);
        assert_eq!(ran[0].output.as_deref(), Some("test result: ok. 3 passed"));
    }

    #[test]
    fn calls_without_a_command_are_not_evidence() {
        let t = Tape::default();
        t.record("t1", &json!({"path": "src/a.rs"}), &Value::Null);
        assert!(t.of("t1").is_empty());
    }

    #[test]
    fn output_keeps_its_tail_on_a_char_boundary() {
        let big = format!("{}é{}", "x".repeat(MAX_OUTPUT), "ok");
        let out = output_text(&json!(big)).unwrap();
        assert!(out.ends_with("ok") && out.len() <= MAX_OUTPUT + 1);
    }

    #[test]
    fn forgetting_a_task_drops_only_its_commands() {
        let t = Tape::default();
        t.record("a", &json!({"command": "ls"}), &Value::Null);
        t.record("b", &json!({"command": "ls"}), &Value::Null);
        t.forget("a");
        assert!(t.of("a").is_empty());
        assert_eq!(t.of("b").len(), 1);
    }
}
