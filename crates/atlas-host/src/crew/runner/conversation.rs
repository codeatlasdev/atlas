//! A task of a crewmate with an `engine` (omp, claude, codex, opencode): one conversational turn
//! per run, the session kept between turns. An `ATLAS_ACTION` in the reply parks the task until the
//! user answers, then a second turn carries the answer.

use std::collections::BTreeMap;

use super::action::{
    finish_unanswered, park_task, parse_atlas_action, resolve_action, user_answer,
};
use super::outcome::{finish_task, persist_running};
use super::prompt::{Prepared, load_agent_env, load_hook_endpoint, prepare_task};
use super::summary::maybe_summarize;
use crate::crew::store::{Crewmate, EngineConfig, Task, TaskState};
use crate::crew::{engine, now_s, pane, triggers};
use crate::server::Hub;

pub(super) async fn run_one_conversational(
    hub: &Hub,
    crewmate: &Crewmate,
    engine_cfg: &EngineConfig,
    mut task: Task,
) {
    let pane_key = pane::key(&crewmate.id, &task.id);
    let Some(Prepared {
        work_dir,
        prompt: full_prompt,
    }) = prepare_task(hub, crewmate, &mut task).await
    else {
        return;
    };

    let (hook_port, hook_token) = load_hook_endpoint(&hub.dir);

    // A stranger's text (GitHub, webhook) never runs with the bypass flags unless its trigger said
    // `allow_auto`. An engine that cannot ask has no other mode, so such a task is refused.
    let yolo = triggers::yolo_for(crewmate.approval, &task);
    if task.no_auto && !engine::can_ask(&engine_cfg.kind) {
        let msg = format!(
            "{} cannot ask for approval, and this task came from text anyone can write (a GitHub event or a webhook); \
             set allow_auto on the trigger to run it without approval",
            engine_cfg.kind
        );
        finish_task(hub, crewmate, &mut task, Err(msg), vec![]);
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }

    // Mark running. If that cannot be stored the task is failed (clients told) and nothing starts.
    task.state = TaskState::Running;
    task.started_at = Some(now_s());
    task.pane = Some(pane_key.clone());
    if !persist_running(hub, &crewmate.id, &mut task) {
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }

    // For omp with approval==ask: write the extension file before engine::run_turn
    // (even if TEST_ENGINE_BIN bypasses the real omp, the file must exist for the test).
    if engine_cfg.kind == "omp" && !yolo {
        let ext_path = hub
            .crew
            .store
            .crewmate_dir(&crewmate.id)
            .join("omp-approval.ts");
        let _ =
            engine::omp::write_approval_extension_pub(&ext_path, hook_port, &hook_token, &pane_key);
    }

    let turn = Turn {
        hub,
        crewmate,
        engine_cfg,
        work_dir: &work_dir,
        pane_key: &pane_key,
        hook: (hook_port, &hook_token),
        yolo,
    };
    match turn.run(&full_prompt).await {
        Ok((reply, usage, env)) => {
            // Check for ATLAS_ACTION in the reply.
            let action = parse_atlas_action(&reply);
            match (action, hub.crew.hub_arc()) {
                (Some(action), Some(hub_arc)) => {
                    park_task(hub, crewmate, &mut task, action.clone());
                    match resolve_action(&hub_arc, &crewmate.id, &task.id, &action).await {
                        Some(follow_up) => {
                            // The action stays on the task (cards only show while waiting): with `answer` it lets the
                            // chat show what was asked and what the user picked.
                            task.answer = user_answer(&action, &follow_up);
                            // The env is read again: it may include secrets written while waiting.
                            match turn.run(&follow_up).await {
                                Ok((reply2, usage2, env2)) => {
                                    settle_reply(hub, crewmate, &mut task, reply2, usage2, &env2)
                                        .await
                                }
                                Err(e) => finish_task(hub, crewmate, &mut task, Err(e), vec![]),
                            }
                        }
                        None => finish_unanswered(hub, crewmate, &mut task),
                    }
                }
                _ => settle_reply(hub, crewmate, &mut task, reply, usage, &env).await,
            }
        }
        Err(e) => finish_task(hub, crewmate, &mut task, Err(e), vec![]),
    }
    triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
}

/// What one engine turn of this task needs, so the first turn and the one after an answered action
/// are the same call.
struct Turn<'a> {
    hub: &'a Hub,
    crewmate: &'a Crewmate,
    engine_cfg: &'a EngineConfig,
    work_dir: &'a str,
    pane_key: &'a str,
    hook: (u16, &'a str),
    yolo: bool,
}

impl Turn<'_> {
    /// Run `prompt` as one turn on the stored session, save the new session and the usage, and return
    /// `(reply, usage, env the turn ran with)`.
    async fn run(
        &self,
        prompt: &str,
    ) -> Result<
        (
            String,
            Option<crate::crew::store::UsageRecord>,
            BTreeMap<String, String>,
        ),
        String,
    > {
        let (hub, crewmate) = (self.hub, self.crewmate);
        let session = hub.crew.store.load_session(&crewmate.id);
        // Crewmate secrets (secrets.json) win over the per-agent env file (`or_insert`): the file is
        // where the provider setup puts a custom provider key, so every engine can reach it without a
        // per-crewmate secret.
        let mut env = hub.crew.store.secret_env(&crewmate.id);
        for (k, v) in load_agent_env(&hub.dir, &crewmate.agent) {
            env.entry(k).or_insert(v);
        }
        let turn = engine::run_turn(
            self.engine_cfg,
            session,
            prompt,
            &hub.crew.store.crewmate_dir(&crewmate.id),
            &crewmate.id,
            self.hook.0,
            self.hook.1,
            self.pane_key,
            &env,
            self.work_dir,
            self.yolo,
        )
        .await?;
        if let Err(e) = hub.crew.store.save_session(&crewmate.id, &turn.session) {
            // The next turn would start a new conversation instead of continuing this one.
            eprintln!(
                "atlas crew: crewmate {} ({}): cannot save the session: {e}",
                crewmate.id, self.pane_key
            );
        }
        if let Some(u) = &turn.usage
            && let Err(e) =
                hub.crew
                    .store
                    .append_usage(&crewmate.id, &self.engine_cfg.kind, now_s(), u)
        {
            eprintln!(
                "atlas crew: crewmate {} ({}): cannot record usage: {e}",
                crewmate.id, self.pane_key
            );
        }
        Ok((turn.reply, turn.usage, env))
    }
}

/// The turn's final reply becomes the task's result: stored as `reply`, shortened when the crew allows it.
async fn settle_reply(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
    reply: String,
    usage: Option<crate::crew::store::UsageRecord>,
    env: &BTreeMap<String, String>,
) {
    task.reply = Some(reply.clone());
    task.usage = usage;
    let (final_outcome, reply_full, was_summarized) =
        maybe_summarize(hub, crewmate, Ok(reply), env).await;
    task.reply_full = reply_full;
    task.summarized = was_summarized;
    finish_task(hub, crewmate, task, final_outcome, vec![]);
}
