//! Commands on a crewmate's tasks: send one, list them, cancel, answer what a task asks, usage.

use serde_json::{Value, json};

use super::super::store::{AskAnswer, Task, TaskAction, TaskState, Trigger};
use super::super::{events, now_s, runner, triggers};
use crate::paths;
use crate::server::Hub;

/// The longest prompt a task takes (docs/crew.md): characters, not bytes.
pub(crate) const MAX_PROMPT_CHARS: usize = 20_000;

/// `CrewmateTask`
pub(crate) fn send_task(
    hub: &Hub,
    id: String,
    prompt: String,
    source: String,
    tz: Option<String>,
) -> Result<Value, String> {
    let crewmate = hub
        .crew
        .store
        .load_crewmate(&id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("prompt must not be empty".into());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "prompt must be at most {MAX_PROMPT_CHARS} characters"
        ));
    }
    let mut task = Task::new(source, prompt);
    task.tz = tz;
    hub.crew
        .store
        .append_task(&id, &task)
        .map_err(|e| format!("append_task: {e}"))?;
    let task_val = serde_json::to_value(&task).map_err(|e| e.to_string())?;
    runner::enqueue(hub, &hub.crew.store, &crewmate, task);
    hub.broadcast(events::task(&id, None));
    Ok(json!({ "task": task_val }))
}

/// `CrewmateTasks`
pub(crate) fn tasks(hub: &Hub, id: String, limit: usize) -> Result<Value, String> {
    let tasks = hub.crew.store.read_tasks(&id, limit);
    Ok(json!({ "tasks": tasks }))
}

/// `CrewmateCancel`
pub(crate) fn cancel(hub: &Hub, id: String, task: String) -> Result<Value, String> {
    runner::cancel(hub, &hub.crew.store, &id, &task).map(|()| json!({}))?;
    Ok(json!({}))
}

/// `CrewmateAnswer`
pub(crate) fn answer(
    hub: &Hub,
    id: String,
    ask_id: String,
    key: Option<String>,
    text: Option<String>,
) -> Result<Value, String> {
    let answer = if let Some(k) = key {
        AskAnswer::Key { value: k }
    } else if let Some(t) = text {
        AskAnswer::Text { value: t }
    } else {
        return Err("key or text required".into());
    };
    let tasks = hub.crew.store.read_tasks(&id, usize::MAX);
    let mut updated = false;
    for mut task in tasks {
        if task.ask.as_ref().map(|a| a.id == ask_id).unwrap_or(false) {
            if let Some(ask) = &mut task.ask {
                ask.answered = Some(answer.clone());
            }
            hub.crew
                .store
                .append_task(&id, &task)
                .map_err(|e| format!("append_task: {e}"))?;
            updated = true;
            break;
        }
    }
    if !updated {
        return Err(format!("ask {ask_id} not found for crewmate {id}"));
    }
    hub.crew.answer_ask(&ask_id, answer);
    Ok(json!({}))
}

/// `CrewmateActionAnswer`
pub(crate) fn action_answer(
    hub: &Hub,
    id: String,
    task_id: String,
    value: String,
) -> Result<Value, String> {
    let tasks = hub.crew.store.read_tasks(&id, usize::MAX);
    let task = tasks
        .iter()
        .find(|t| t.id == task_id && t.state == TaskState::WaitingAction)
        .ok_or_else(|| format!("no waiting-action task {task_id} for crewmate {id}"))?
        .clone();
    match &task.action {
        Some(TaskAction::ProvideSecret { name, .. }) => {
            let name = name.clone();
            hub.crew
                .store
                .set_secret(&id, &name, &value)
                .map_err(|e| format!("set_secret: {e}"))?;
            hub.crew.answer_action(&task_id, name);
        }
        Some(TaskAction::ConnectGithub { .. }) => {
            return Err("use crew_github_connect_resume for connect_github actions".into());
        }
        Some(TaskAction::AddTrigger {
            trigger: trigger_kind,
            prompt: trigger_prompt,
            ..
        }) => {
            if value == "add" {
                // Load fresh crewmate to avoid overwriting concurrent changes.
                let mut fresh = hub
                    .crew
                    .store
                    .load_crewmate(&id)
                    .map_err(|e| format!("load_crewmate: {e}"))?;
                let new_trigger = Trigger {
                    id: paths::new_id(),
                    kind: trigger_kind.clone(),
                    enabled: true,
                    prompt: trigger_prompt.clone(),
                    last_fired: Some(now_s()), // baseline: next fire at real schedule time
                    tz: task.tz.clone(),       // stamp tz from task
                    next_fire: None,
                };
                triggers::validate_trigger(&new_trigger)?;
                fresh.triggers.push(new_trigger);
                hub.crew
                    .store
                    .save_crewmate(&fresh)
                    .map_err(|e| format!("save_crewmate: {e}"))?;
                hub.broadcast(events::crewmate_updated(&id));
            }
            hub.crew.answer_action(&task_id, value);
        }
        _ => {
            hub.crew.answer_action(&task_id, value);
        }
    }
    Ok(json!({}))
}

/// `CrewmateUsage`
pub(crate) fn usage(hub: &Hub, id: Option<String>, since: Option<u64>) -> Result<Value, String> {
    let records = hub.crew.store.read_usage(id.as_deref(), since.unwrap_or(0));
    Ok(json!({ "records": records }))
}
