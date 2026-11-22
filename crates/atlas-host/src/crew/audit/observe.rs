//! Audit entries that come from what the agents report: every tool call (`agent_step`) and every approval asked
//! (`agent_approval`). How an approval ended is `approval_decision`, recorded where it is decided
//! (`crew::approval`). One subscriber sees all crewmate panes, so a task in a terminal and
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
        loop {
            let ev = match rx.recv().await {
                Ok(v) => v,
                Err(RecvError::Lagged(n)) => {
                    // Say so in the log instead of leaving a silent gap in an audit.
                    // i18n-ignore: log
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
            record_event(&h.crew, &ev);
        }
    });
}

fn record_event(crew: &CrewHub, ev: &Value) {
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
        // How a prompt ended is `approval_decision`, recorded by `crew::approval::decide` where the decision is made
        // (a person's answer, the mode, a grant, a rail, a timeout), not here.
        Some("agent_approval") => crew.audit.record(
            Some(crewmate_id),
            "approval_asked",
            json!({
                "task_id": task_id,
                "req": ev["req"],
                "tool_name": ev["tool_name"],
                "tool_input": ev["tool_input"],
                "rail": ev["rail"],
            }),
        ),
        _ => {}
    }
}
