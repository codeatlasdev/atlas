//! A task of a crewmate without an engine: the agent runs in a terminal (pane `crew:<id>:<task>`)
//! and reports through the hook endpoint; the runner watches its status and its exit.

use std::collections::BTreeMap;

use super::action::{
    continuation_prompt, finish_unanswered, park_task, parse_atlas_action, resolve_action,
    user_answer, visible_reply,
};
use super::gate::{Gate, gate_success};
use super::outcome::{audit_task, broadcast_task, finish_task, persist, wait_for_outcome};
use super::prompt::{Prepared, load_agent_env, prepare_task};
use super::recover::recovered_wait;
use super::summary::maybe_summarize;
#[cfg(test)]
use super::testing::{TEST_AGENT_BIN, TEST_PAUSE_AFTER_SPAWN};
use crate::crew::store::{Crewmate, Step, Task, TaskState};
use crate::crew::{now_s, pane, triggers};
use crate::paths;
use crate::protocol::{Mode, PtyCreate, Scope};
use crate::pty::Session;
use crate::server::Hub;

pub(super) async fn run_one_pty(hub: &Hub, crewmate: &Crewmate, mut task: Task) {
    let pane_key = pane::key(&crewmate.id, &task.id);
    let Some(Prepared {
        work_dir,
        prompt: full_prompt,
    }) = prepare_task(hub, crewmate, &mut task).await
    else {
        return;
    };

    // The summarizer runs with the same agent env as the terminal agent.
    let agent_env = load_agent_env(&hub.dir, &crewmate.agent);

    // A refusal parked when hostd stopped is decided by the owner's answer, not by running again.
    if let Some(hub_arc) = hub.crew.hub_arc()
        && super::gate::resume_refusal(&hub_arc, crewmate, &mut task).await
    {
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }

    // A task parked on an action when hostd stopped keeps waiting: the agent's last reply is the
    // `result` stored with the card, and the answer starts the next run as it would have before.
    let (outcome, steps) = if recovered_wait(&task) {
        (Ok(task.result.clone().unwrap_or_default()), vec![])
    } else {
        run_pty_inner(hub, crewmate, &mut task, &full_prompt, &pane_key, &work_dir).await
    };
    let rewaiting = recovered_wait(&task).then(|| task.action.clone()).flatten();
    // Check for ATLAS_ACTION in a successful result (or the card the task was waiting on).
    if let Ok(result_text) = &outcome
        && let Some(action) = rewaiting.or_else(|| parse_atlas_action(result_text))
        && let Some(hub_arc) = hub.crew.hub_arc()
    {
        // Show what the agent said above the card while it waits (the card replaces the machine line).
        task.result = Some(visible_reply(result_text)).filter(|s| !s.is_empty());
        park_task(hub, crewmate, &mut task, action.clone());
        match resolve_action(&hub_arc, &crewmate.id, &task.id, &action).await {
            Some(answer) => {
                task.answer = user_answer(&action, &answer);
                let next = continuation_prompt(&full_prompt, result_text, &answer);
                let (outcome2, steps2) =
                    run_pty_inner(hub, crewmate, &mut task, &next, &pane_key, &work_dir).await;
                finish_summarized(
                    hub, crewmate, &mut task, outcome2, steps2, &agent_env, &work_dir,
                )
                .await;
            }
            None => finish_unanswered(hub, crewmate, &mut task),
        }
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }
    finish_summarized(
        hub, crewmate, &mut task, outcome, steps, &agent_env, &work_dir,
    )
    .await;
    triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
}

/// Guard the work, shorten the reply when the crew allows it, then record the task's end.
async fn finish_summarized(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
    outcome: Result<String, String>,
    steps: Vec<Step>,
    agent_env: &BTreeMap<String, String>,
    work_dir: &str,
) {
    let outcome = match (outcome, hub.crew.hub_arc()) {
        (Ok(result), Some(hub_arc)) => {
            match gate_success(&hub_arc, crewmate, task, result, work_dir).await {
                Gate::Finish(result) => Ok(result),
                Gate::Handled => return,
            }
        }
        (outcome, _) => outcome,
    };
    let (final_outcome, reply_full, was_summarized) =
        maybe_summarize(hub, crewmate, outcome, agent_env).await;
    task.reply_full = reply_full;
    task.summarized = was_summarized;
    finish_task(hub, crewmate, task, final_outcome, steps);
}

/// Spawn a PTY for the agent and wait for completion; returns (outcome, steps).
/// Sets task state to Running, stores and broadcasts the update.
async fn run_pty_inner(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
    full_prompt: &str,
    pane_key: &str,
    work_dir: &str,
) -> (Result<String, String>, Vec<Step>) {
    let ctx = super::isolation::Ctx::of(hub, &crewmate.id);
    if let Err(e) = ctx.prepare(hub, &crewmate.id) {
        return (
            Err(format!("cannot prepare the crewmate's runtime files: {e}")),
            vec![],
        );
    }
    // Resolve the agent definition.
    let Some(agent_def) = atlas_core::agents::get(&crewmate.agent) else {
        return (Err(format!("unknown agent '{}'", crewmate.agent)), vec![]);
    };

    // In tests a fake binary can be injected; production always uses the agent def.
    #[cfg(test)]
    let argv = match TEST_AGENT_BIN
        .lock()
        .clone()
        .filter(|_| ctx.isolated().is_none())
    {
        // An isolated agent is an engine by name, found by the helper in its own directories.
        Some(bin) => vec![bin],
        None => agent_def.argv(
            triggers::yolo_for(crewmate.approval, task),
            Some(full_prompt),
        ),
    };
    #[cfg(not(test))]
    let argv = {
        let mut argv = agent_def.argv(
            triggers::yolo_for(crewmate.approval, task),
            Some(full_prompt),
        );
        if crewmate.agent == "omp" {
            let overlay =
                crate::crew::engine::omp::overlay_args(&ctx.runtime_dir(hub, &crewmate.id));
            argv.splice(1..1, overlay);
        }
        argv
    };

    // Build PtyCreate, started in the crewmate's project (prepare_work_dir), never in hostd's own directory.
    let mut req = PtyCreate {
        cwd: work_dir.to_string(),
        argv,
        env: BTreeMap::from([("ATLAS_HOOK_DECISION_SECS".to_string(), "1800".to_string())]),
        cols: 120,
        rows: 40,
        scope: Scope {
            pane_key: Some(pane_key.to_string()),
            ..Default::default()
        },
        title: Some(format!("crew:{}", crewmate.name)),
        mode: Mode::Pty,
    };

    // Per-agent env overrides from <dir>/agent-env/<agent>.env.
    req.env.extend(load_agent_env(&hub.dir, &crewmate.agent));

    // An isolated agent reports to its own endpoint (a token that opens only its routes); the
    // request's values win over `hook_env`'s defaults, which name the crew user's global endpoint.
    req.env.extend(ctx.agent_env(hub, &crewmate.id));
    // Inject hook endpoint env vars (must come after crewmate env so ATLAS_* always wins).
    crate::server::hook_env(hub, &mut req);

    // Listen before the agent exists: an agent can report its end (or its title) in the first
    // milliseconds, and a status sent before the subscription is gone; the exit that follows
    // would then fail a finished task as "agent exited (code 0)".
    let agent_rx = hub.agents.subscribe();

    // Spawn the pty.
    let scroll = paths::scrollback_dir(&hub.dir);
    #[cfg(unix)]
    let spawned = Session::spawn_as(req, &scroll, ctx.as_user());
    #[cfg(not(unix))]
    let spawned = Session::spawn(req, &scroll);
    let session = match spawned {
        Ok(s) => s,
        Err(e) => return (Err(format!("spawn failed: {e:#}")), vec![]),
    };

    let pty_id = session.id.clone();
    hub.sessions.lock().insert(pty_id.clone(), session.clone());
    #[cfg(test)]
    let pause = *TEST_PAUSE_AFTER_SPAWN.lock();
    #[cfg(test)]
    if let Some(pause) = pause {
        tokio::time::sleep(pause).await;
    }

    // Mark task as running and record start time. If that cannot be stored there is no honest
    // "running" to show: stop the agent and fail the task with the store error.
    task.state = TaskState::Running;
    task.started_at = Some(now_s());
    task.pane = Some(pane_key.to_string());
    if let Err(msg) = persist(hub, &crewmate.id, task) {
        session.kill().await;
        hub.sessions.lock().remove(&pty_id);
        return (Err(msg), vec![]);
    }
    audit_task(hub, &crewmate.id, "task_started", task);
    broadcast_task(hub, &crewmate.id, task);

    // Wait for completion.
    let (outcome, steps) =
        wait_for_outcome(hub, agent_rx, &session, pane_key, &crewmate.id, task).await;

    // Clean up session.
    hub.sessions.lock().remove(&pty_id);

    (outcome, steps)
}
