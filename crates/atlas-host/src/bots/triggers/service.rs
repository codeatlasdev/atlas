//! Service-state trigger: polls `.logs/<service>.log` every 30 s,
//! fires on `failed` / `recovered` transitions.
//!
//! State is tracked in memory (per trigger ID); unknown → failed does NOT fire
//! to avoid spurious alerts on first startup when logs are stale.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::bots::store::TriggerKind;
use crate::server::Hub;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Unknown,
    Running,
    Failed,
}

pub(super) async fn run(hub_weak: Weak<Hub>) {
    // key = trigger_id → last known state
    let mut states: HashMap<String, State> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(hub) = hub_weak.upgrade() else { break };
        let bots = hub.bots.store.list_bots();
        for bot in &bots {
            for trigger in &bot.triggers {
                if !trigger.enabled {
                    continue;
                }
                if let TriggerKind::Service {
                    project_dir,
                    service,
                    on,
                } = &trigger.kind
                {
                    check(
                        &hub,
                        &bot.id,
                        trigger,
                        project_dir,
                        service,
                        on,
                        &mut states,
                    );
                }
            }
        }
    }
}

fn check(
    hub: &Arc<Hub>,
    bot_id: &str,
    trigger: &crate::bots::store::Trigger,
    project_dir: &str,
    service: &str,
    on: &str,
    states: &mut HashMap<String, State>,
) {
    let log_path = Path::new(project_dir)
        .join(".logs")
        .join(format!("{service}.log"));
    let current = detect(&log_path);
    let prev = states.get(&trigger.id).copied().unwrap_or(State::Unknown);
    states.insert(trigger.id.clone(), current);

    let fires = match (prev, current, on) {
        // Running → Failed: fire "failed"
        (State::Running, State::Failed, "failed") => true,
        // Failed → Running: fire "recovered"
        (State::Failed, State::Running, "recovered") => true,
        // Don't fire Unknown → Failed to avoid startup noise.
        _ => false,
    };

    if fires {
        let label = match current {
            State::Failed => "failed",
            State::Running => "recovered",
            State::Unknown => "unknown",
        };
        let event = format!("Service {service} {label} in {project_dir}");
        super::fire(hub, bot_id, &trigger.id, &trigger.prompt, &event);
    }
}

/// Classify the last meaningful log line.
fn detect(log_path: &Path) -> State {
    let Ok(content) = std::fs::read_to_string(log_path) else {
        return State::Unknown;
    };
    let last = content
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let lower = last.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("failed") || lower.contains("crash") {
        State::Failed
    } else if lower.contains("started") || lower.contains("ready") || lower.contains("listening") {
        State::Running
    } else {
        State::Unknown
    }
}
