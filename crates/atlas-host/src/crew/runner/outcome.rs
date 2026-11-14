//! How a task ends: waiting for the agent's status or exit, then recording the result, telling the
//! clients (push events, Telegram, phone notifications).

use std::sync::Arc;
use std::time::Duration;

use super::TASK_TIMEOUT;
use super::action::visible_reply;
use crate::crew::store::{CrewStore, Crewmate, Step, Task, TaskState};
use crate::crew::{events, now_s};
use crate::pty::Session;
use crate::server::Hub;

/// The agent's own title for what it is doing, fit for the live card. omp first titles a session with the start of
/// our instructions ("You are carlos. …") and decorates real titles with an emoji ("🏪 STORE: …"); neither belongs on
/// the card.
fn clean_title(raw: &str) -> Option<String> {
    let t = raw
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim();
    (!t.is_empty() && !t.starts_with("You are ")).then(|| t.to_string())
}

/// Poll agent status events and pty exit to determine the task outcome. `agent_rx` must be
/// subscribed before the agent was spawned.
pub(super) async fn wait_for_outcome(
    hub: &Hub,
    mut agent_rx: tokio::sync::broadcast::Receiver<serde_json::Value>,
    session: &Arc<Session>,
    pane_key: &str,
    crewmate_id: &str,
    task: &mut Task,
) -> (Result<String, String>, Vec<Step>) {
    let deadline = tokio::time::sleep(TASK_TIMEOUT);
    tokio::pin!(deadline);
    let mut steps: Vec<Step> = Vec::new();

    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => {
                session.kill().await;
                return (Err("timed out after 2h".to_string()), steps);
            }
            ev = agent_rx.recv() => {
                match ev {
                    Ok(v) if v["ev"] == "agent_status"
                        && v["pane_key"].as_str() == Some(pane_key) =>
                    {
                        // The task the agent set itself ("DANFE: Add DANFE and XML download to portal"), shown on
                        // the live card so the user sees what it is doing, not just "Working".
                        if let Some(title) = v["status"]["title"].as_str().and_then(clean_title)
                            && task.title.as_deref() != Some(title.as_str())
                        {
                            task.title = Some(title);
                            let _ = persist(hub, crewmate_id, task);
                            broadcast_task(hub, crewmate_id, task);
                        }
                        let state = v["status"]["state"].as_str().unwrap_or("");
                        if state == "done" {
                            let result = v["status"]["last_message"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            session.kill().await;
                            return (Ok(result), steps);
                        }
                    }
                    Ok(v) if v["ev"] == "agent_step"
                        && v["pane_key"].as_str() == Some(pane_key) =>
                    {
                        if steps.len() < 50 {
                            let tool_name = v["tool_name"].as_str().unwrap_or("?");
                            let target = v["tool_input"]
                                .as_object()
                                .and_then(|m| m.values().find_map(|v| v.as_str()))
                                .map(|s| s.chars().take(80).collect::<String>());
                            let text = match target {
                                Some(t) => format!("{tool_name} {t}"),
                                None => tool_name.to_string(),
                            };
                            steps.push(Step { ts: now_s(), text, group: task.title.clone() });
                            // Live, not only at the end: the working line shows the latest step while it runs.
                            task.steps = steps.clone();
                            let _ = persist(hub, crewmate_id, task);
                            broadcast_task(hub, crewmate_id, task);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return (Err("agent hub closed".to_string()), steps);
                    }
                    // Other panes' events overflowed the channel: this pane's `done` may be among the
                    // dropped ones, and its exit would then fail the task. The stored status is current.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Some(st) = hub.agents.status_of(pane_key)
                            && st.state == crate::agents::State::Done
                        {
                            session.kill().await;
                            return (Ok(st.last_message.unwrap_or_default()), steps);
                        }
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if let Some(code) = session.exited() {
                    return (Err(format!(
                        "agent exited (code {})",
                        code.map_or_else(|| "?".to_string(), |c| c.to_string())
                    )), steps);
                }
            }
        }
    }
}

pub(super) fn finish_task(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
    outcome: Result<String, String>,
    steps: Vec<Step>,
) {
    task.finished_at = Some(now_s());
    task.steps = steps;
    match outcome {
        Ok(result) => {
            task.state = TaskState::Done;
            // Every path ends here, so the machine line never reaches what people read (desktop, phone, Telegram).
            task.result = Some(visible_reply(&result));
        }
        Err(e) => {
            // Don't overwrite Cancelled state.
            if task.state != TaskState::Cancelled {
                task.state = TaskState::Failed;
                task.error = Some(e);
            }
        }
    }
    // The result is announced even when it cannot be stored: the clients and Telegram still hear it,
    // and `persist` logged why the log lacks it.
    let _ = persist(hub, &crewmate.id, task);
    broadcast_task(hub, &crewmate.id, task);

    // Telegram outbound — done and failed both get a message.
    let telegram_msg: Option<String> = match task.state {
        TaskState::Done => {
            let r = task.result.as_deref().unwrap_or("");
            Some(
                format!("{}: done\n\n{}", crewmate.name, r)
                    .chars()
                    .take(4000)
                    .collect(),
            )
        }
        TaskState::Failed => {
            let e = task.error.as_deref().unwrap_or("unknown");
            Some(
                format!("{}: failed: {}", crewmate.name, e)
                    .chars()
                    .take(4000)
                    .collect(),
            )
        }
        _ => None,
    };
    if let Some(msg) = telegram_msg {
        hub.crew.telegram.send_message(&msg);
    }

    // Push notification — only for task_done, skip phones already connected.
    if task.state != TaskState::Done {
        return;
    }
    let live = hub.mobile.live_node_ids();
    let tokens: Vec<_> = hub
        .crew
        .push_tokens
        .lock()
        .iter()
        .filter(|t| !live.contains(&t.node_id))
        .cloned()
        .collect();
    if !tokens.is_empty() {
        tokio::spawn(async move {
            let expired = crate::crew::push::notify(&tokens, "task_done").await;
            if !expired.is_empty() {
                eprintln!("atlas crew: push: {} expired token(s)", expired.len());
            }
        });
    }
}

/// Append `task`'s current state to the crewmate's task log. A failure is never silent: it is logged
/// with the crewmate and task id (`atlas crew logs`) and returned as the message a failed task shows.
pub(super) fn persist(hub: &Hub, crewmate_id: &str, task: &Task) -> Result<(), String> {
    hub.crew.store.append_task(crewmate_id, task).map_err(|e| {
        let msg = format!(
            "cannot record task state to {}: {e}",
            hub.crew.store.tasks_path(crewmate_id).display()
        );
        eprintln!(
            "atlas crew: crewmate {crewmate_id} task {}: {msg} (state {:?})",
            task.id, task.state
        );
        msg
    })
}

/// Record that `task` is now running. When that cannot be stored the task fails with the error
/// (clients are told over the push event) and `false` is returned: the caller must stop, or not
/// start, the agent. Without this the task showed "running" with nothing behind it on disk.
pub(super) fn persist_running(hub: &Hub, crewmate_id: &str, task: &mut Task) -> bool {
    let stored = persist(hub, crewmate_id, task);
    if let Err(msg) = &stored {
        task.state = TaskState::Failed;
        task.error = Some(msg.clone());
        task.finished_at = Some(now_s());
    }
    broadcast_task(hub, crewmate_id, task);
    stored.is_ok()
}

/// Whether the user cancelled this task (a cancel records `Cancelled` in the store while a waiter sleeps).
pub(super) fn was_cancelled(hub: &Hub, crewmate_id: &str, task_id: &str) -> bool {
    hub.crew
        .store
        .read_tasks(crewmate_id, 10_000)
        .iter()
        .any(|t| t.id == task_id && t.state == TaskState::Cancelled)
}

pub(super) fn broadcast_task(hub: &Hub, crewmate_id: &str, task: &Task) {
    hub.broadcast(events::task(crewmate_id, Some(task)));
}

pub(super) fn update_task(
    hub: &Hub,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    f: impl FnOnce(&mut Task),
) {
    // Find the task in the store (last state).
    let tasks = store.read_tasks(crewmate_id, 10_000);
    if let Some(mut t) = tasks.into_iter().find(|t| t.id == task_id) {
        f(&mut t);
        if let Err(msg) = persist(hub, crewmate_id, &t) {
            t.error.get_or_insert(msg);
        }
        broadcast_task(hub, crewmate_id, &t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real titles from omp on the test ship: the first one is the start of our prompt, the next carries an emoji.
    #[test]
    fn live_card_title_drops_the_prompt_echo_and_the_emoji() {
        assert_eq!(clean_title("You are carlos. Something else"), None);
        assert_eq!(
            clean_title("🏪 STORE: Resumir propósito do apps/store").as_deref(),
            Some("STORE: Resumir propósito do apps/store")
        );
        assert_eq!(
            clean_title("DANFE: Add DANFE and XML download").as_deref(),
            Some("DANFE: Add DANFE and XML download")
        );
        assert_eq!(clean_title("  "), None);
    }
}
