//! Audit entries that come from what the agents report: every tool call (`agent_step`), every
//! approval asked (`agent_approval`) and how it ended (`agent_approval_done`: allow, deny or
//! timeout, and who answered). One subscriber sees all crewmate panes, so a task in a terminal and
//! one on a headless engine are audited the same way, and no other code path has to remember to.

use std::sync::Weak;

use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::crew::{hub::CrewHub, pane};
use crate::server::Hub;

/// Start recording the agent hub's events. The receiver is taken before this returns (a broadcast
/// receiver only sees what is sent after it subscribes), so a task started right after is covered.
pub(crate) fn observe(hub_w: Weak<Hub>) {
    let Some(h) = hub_w.upgrade() else { return };
    let mut rx = h.agents.subscribe();
    drop(h);
    tokio::spawn(async move {
        // Asked prompts by `req`: the done event does not repeat the tool.
        let mut asked: std::collections::HashMap<String, Value> = Default::default();
        loop {
            let ev = match rx.recv().await {
                Ok(v) => v,
                Err(RecvError::Lagged(n)) => {
                    // Say so in the log instead of leaving a silent gap in an audit.
                    eprintln!("atlas crew: audit: {n} agent events were dropped under load");
                    if let Some(h) = hub_w.upgrade() {
                        h.crew
                            .audit
                            .record(None, "audit_gap", json!({"dropped": n}));
                    }
                    continue;
                }
                Err(RecvError::Closed) => return,
            };
            let Some(h) = hub_w.upgrade() else { return };
            record_event(&h.crew, &ev, &mut asked);
        }
    });
}

fn record_event(crew: &CrewHub, ev: &Value, asked: &mut std::collections::HashMap<String, Value>) {
    let Some(pane_key) = ev["pane_key"]
        .as_str()
        .filter(|p| pane::is_crewmate_pane(p))
    else {
        return;
    };
    let Some(crewmate_id) = pane::crewmate_id(pane_key) else {
        return;
    };
    let task_id = pane_key.rsplit(':').next().unwrap_or_default();
    match ev["ev"].as_str() {
        Some("agent_step") => crew.audit.record(
            Some(crewmate_id),
            "tool_call",
            json!({
                "task_id": task_id,
                "tool_name": ev["tool_name"],
                "tool_input": ev["tool_input"],
            }),
        ),
        Some("agent_approval") => {
            let req = ev["req"].as_str().unwrap_or_default().to_string();
            crew.audit.record(
                Some(crewmate_id),
                "approval_asked",
                json!({
                    "task_id": task_id,
                    "req": req,
                    "tool_name": ev["tool_name"],
                    "tool_input": ev["tool_input"],
                }),
            );
            asked.insert(req, ev.clone());
        }
        Some("agent_approval_done") => {
            let req = ev["req"].as_str().unwrap_or_default();
            let tool = asked.remove(req).unwrap_or_default();
            crew.audit.record(
                Some(crewmate_id),
                "approval_decision",
                json!({
                    "task_id": task_id,
                    "req": req,
                    "tool_name": tool["tool_name"],
                    "decision": ev["decision"],
                    "by": ev["by"],
                }),
            );
        }
        _ => {}
    }
}
