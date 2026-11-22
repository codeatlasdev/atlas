//! A task of a crewmate with an `engine` (omp, claude, codex, opencode): one conversational turn
//! per run, the session kept between turns. An `ATLAS_ACTION` in the reply parks the task until the
//! user answers, then a second turn carries the answer.

use std::collections::BTreeMap;

use super::action::{
    answer_accepted, finish_unanswered, park_task, parse_atlas_action, resolve_action,
    running_again,
};
use super::gate::{Gate, gate_success};
use super::isolation::Ctx;
use super::outcome::{finish_task, persist_running, was_cancelled};
use super::prompt::{Prepared, load_agent_env, prepare_task};
use super::recover::{recovered_wait, resume_prompt};
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

    // How this crewmate runs: its own Linux user or the crew user. One answer for the whole task.
    let ctx = Ctx::of(hub, &crewmate.id);
    if let Err(e) = ctx.prepare(hub, &crewmate.id) {
        let msg = format!("cannot prepare the crewmate's runtime files: {e}");
        finish_task(hub, crewmate, &mut task, Err(msg), vec![]);
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }
    let (hook_port, hook_token) = ctx.hook_endpoint(hub, &crewmate.id);

    // The engines that can ask (claude, omp) are always launched in ask mode, with no bypass flag, and hostd decides
    // each call from the crewmate's current mode (`crew::approval`). An engine that cannot ask (codex, opencode) has
    // no such channel: it always runs unattended, so a task from a stranger's text (GitHub, webhook; unless its
    // trigger said `allow_auto`) is refused, and so is any task when the crew does not allow Bypass.
    let can_ask = engine::can_ask(&engine_cfg.kind);
    let refusal = if task.no_auto && !can_ask {
        Some(format!(
            "{} cannot ask for approval, and this task came from text anyone can write (a GitHub event or a webhook); \
             set allow_auto on the trigger to run it without approval",
            engine_cfg.kind
        ))
    } else if !can_ask && !hub.crew.store.load_crew_settings().allow_bypass {
        Some(format!(
            "{} has no approval channel: it always runs unattended, and this crew does not allow Bypass",
            engine_cfg.kind
        ))
    } else {
        None
    };
    if let Some(msg) = refusal {
        finish_task(hub, crewmate, &mut task, Err(msg), vec![]);
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }

    // A refusal parked when hostd stopped is decided by the owner's answer, not by running again.
    if let Some(hub_arc) = hub.crew.hub_arc()
        && super::gate::resume_refusal(&hub_arc, crewmate, &mut task).await
    {
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }

    // A task parked on an action when hostd stopped keeps waiting (it is not running again).
    let waiting = recovered_wait(&task);
    // A slot for the agent, then mark running. A task cancelled while it prepared or waited for a slot never gets
    // its agent: the store has the cancel (#292). If running cannot be stored the task is failed (clients told) and
    // nothing starts.
    let mut slot: Option<super::slots::Slot> = None;
    if !waiting {
        let Some(s) = super::slots::take(hub, crewmate, &mut task).await else {
            finish_task(
                hub,
                crewmate,
                &mut task,
                Err("cancelled before the agent started".into()),
                vec![],
            );
            triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
            return;
        };
        slot = Some(s);
        task.state = TaskState::Running;
        task.started_at = Some(now_s());
        task.pane = Some(pane_key.clone());
        if !persist_running(hub, &crewmate.id, &mut task) {
            triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
            return;
        }
    }

    // For omp: write the approval extension before engine::run_turn, whatever the mode (hostd decides every call,
    // so omp never gets `--auto-approve`); even if TEST_ENGINE_BIN bypasses the real omp, the file must exist for
    // the test.
    if engine_cfg.kind == "omp" {
        let ext_path = ctx.runtime_dir(hub, &crewmate.id).join("omp-approval.ts");
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
        ctx: &ctx,
    };
    // A task parked on an action when hostd stopped goes straight back to waiting for the answer;
    // one a restart interrupted mid-turn continues the stored session instead of starting over.
    if waiting {
        let action = task.action.clone().expect("recovered_wait has an action");
        answer_action_turn(&turn, &mut task, &action).await;
        triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
        return;
    }
    let first = if task.resumed
        && hub
            .crew
            .store
            .load_session(&crewmate.id)
            .session_id
            .is_some()
    {
        resume_prompt(&task.prompt)
    } else {
        full_prompt
    };
    match turn.run(&first).await {
        Ok((reply, usage, env)) => {
            // Check for ATLAS_ACTION in the reply.
            let action = parse_atlas_action(&reply);
            match (action, hub.crew.hub_arc()) {
                (Some(action), Some(_)) => {
                    // Parked on a person: the slot goes back so another crewmate's agent can run meanwhile.
                    drop(slot.take());
                    park_task(hub, crewmate, &mut task, action.clone());
                    answer_action_turn(&turn, &mut task, &action).await;
                }
                _ => settle_reply(&turn, &mut task, reply, usage, &env).await,
            }
        }
        Err(e) => finish_task(hub, crewmate, &mut task, Err(e), vec![]),
    }
    drop(slot);
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
    ctx: &'a Ctx,
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
        // Git through the GitHub App (helper and bot identity, never a token): hostd's, so it wins over a secret.
        env.extend(crate::crew::github::app::agent_env(hub, &crewmate.id, self.pane_key).await);
        // What the engine's stream says the agent ran, for the guards (`runner::tape`).
        let task_id = self
            .pane_key
            .rsplit(':')
            .next()
            .unwrap_or_default()
            .to_string();
        // Registered for the length of the turn: a cancel or a pause fires it (`runner::cancel_with`).
        // `cancel_with` stores `Cancelled` before it looks the handle up, so one that landed before
        // this registration is seen here instead of being lost.
        let cancel = engine::TurnCancel::new();
        let _registered = TurnRegistration::new(hub, &task_id, cancel.clone());
        if was_cancelled(hub, &crewmate.id, &task_id) {
            cancel.fire();
        }
        let tools = {
            let hub_w = hub.crew.hub_arc().map(|h| std::sync::Arc::downgrade(&h));
            move |tool: &str, args: &serde_json::Value, output: &str, is_error: bool| {
                if let Some(h) = hub_w.as_ref().and_then(std::sync::Weak::upgrade) {
                    h.crew
                        .tape
                        .record_finished(&task_id, tool, args, output, is_error);
                }
            }
        };
        // Where the agent reads what the engine writes for it, and who runs the process: one answer per task.
        let crewmate_dir = self.ctx.runtime_dir(hub, &crewmate.id);
        let exec = self.ctx.exec();
        // What the agent is told about its permissions goes before the prompt: the fragment when it was not told
        // its current mode yet (a first turn, a resumed task, a mode changed since), and what the owner said.
        let prompt = format!(
            "{}{prompt}",
            crate::crew::approval::prelude(hub, &crewmate.id, self.pane_key)
        );
        let turn = engine::run_turn(engine::TurnCtx {
            engine: self.engine_cfg,
            session,
            prompt: &prompt,
            crewmate_dir: &crewmate_dir,
            crewmate_id: &crewmate.id,
            hook_port: self.hook.0,
            hook_token: self.hook.1,
            pane_key: self.pane_key,
            extra_env: &env,
            work_dir: self.work_dir,
            tools: Some(&tools),
            cancel: &cancel,
            exec: &exec,
        })
        .await?;
        if let Err(e) = hub.crew.store.save_session(&crewmate.id, &turn.session) {
            // The next turn would start a new conversation instead of continuing this one.
            // i18n-ignore: log
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
            // i18n-ignore: log
            eprintln!(
                "atlas crew: crewmate {} ({}): cannot record usage: {e}",
                crewmate.id, self.pane_key
            );
        }
        Ok((turn.reply, turn.usage, env))
    }
}

/// Wait for the user's answer to `action` (the task is parked), then run it as the next turn.
async fn answer_action_turn(
    turn: &Turn<'_>,
    task: &mut Task,
    action: &crate::crew::store::TaskAction,
) {
    let (hub, crewmate) = (turn.hub, turn.crewmate);
    let Some(hub_arc) = hub.crew.hub_arc() else {
        return;
    };
    match resolve_action(&hub_arc, &crewmate.id, &task.id, action).await {
        Some(follow_up) => {
            // The action stays on the task: with `answer` it lets the chat show what was asked and what the user
            // picked. The task is running from this moment, for every client (#524).
            answer_accepted(hub, crewmate, task, action, &follow_up);
            // The env is read again: it may include secrets written while waiting. The answer starts an agent
            // again, so it needs a slot again (the wait for the person held none).
            let Some(_slot) = super::slots::take(hub, crewmate, task).await else {
                finish_unanswered(hub, crewmate, task);
                return;
            };
            // A wait for a slot showed "server busy" meanwhile.
            if task.state != TaskState::Running {
                running_again(hub, crewmate, task);
            }
            match turn.run(&follow_up).await {
                Ok((reply2, usage2, env2)) => settle_reply(turn, task, reply2, usage2, &env2).await,
                Err(e) => finish_task(hub, crewmate, task, Err(e), vec![]),
            }
        }
        None => finish_unanswered(hub, crewmate, task),
    }
}

/// The turn's final reply becomes the task's result: guarded, stored as `reply`, shortened when the
/// crew allows it.
async fn settle_reply(
    turn: &Turn<'_>,
    task: &mut Task,
    reply: String,
    usage: Option<crate::crew::store::UsageRecord>,
    env: &BTreeMap<String, String>,
) {
    let (hub, crewmate) = (turn.hub, turn.crewmate);
    task.usage = usage;
    let reply = match hub.crew.hub_arc() {
        Some(hub_arc) => match gate_success(&hub_arc, crewmate, task, reply, turn.work_dir).await {
            Gate::Finish(reply) => reply,
            Gate::Handled => return,
        },
        None => reply,
    };
    // The agent's `spoken:` line is for listening: it is not part of the reply, and survives the summary.
    task.spoken = super::spoken::agent_line(&reply).unwrap_or_default();
    task.reply = Some(super::spoken::without_line(&reply));
    let (final_outcome, reply_full, was_summarized) =
        maybe_summarize(hub, crewmate, Ok(reply), env).await;
    task.reply_full = reply_full;
    task.summarized = was_summarized;
    finish_task(hub, crewmate, task, final_outcome, vec![]);
}

/// A running turn's cancel handle in `CrewHub::turn_cancels`, removed when the turn ends.
struct TurnRegistration<'a> {
    hub: &'a Hub,
    task_id: String,
}

impl<'a> TurnRegistration<'a> {
    fn new(hub: &'a Hub, task_id: &str, cancel: engine::TurnCancel) -> Self {
        hub.crew
            .turn_cancels
            .lock()
            .insert(task_id.to_string(), cancel);
        Self {
            hub,
            task_id: task_id.to_string(),
        }
    }
}

impl Drop for TurnRegistration<'_> {
    fn drop(&mut self) {
        self.hub.crew.turn_cancels.lock().remove(&self.task_id);
    }
}
