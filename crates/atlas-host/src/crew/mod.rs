//! Atlas Crew on a crew server: crewmates (an agent with a role, a look and tasks), their task
//! queues, triggers and schedules, and the channels people reach them on (the apps, Telegram, push).
//!
//! | module      | what it owns                                                                |
//! |-------------|-----------------------------------------------------------------------------|
//! | `hub`       | [`CrewHub`]: shared state, startup, restart recovery, approval forwarding   |
//! | `handlers`  | the `crewmate_*` / `crew_settings_*` commands, by what they act on          |
//! | `github`    | `crew_github_*` commands and the `gh` CLI helpers                           |
//! | `runner`    | one task at a time per crewmate: conversational engines or a terminal agent |
//! | `engine`    | the conversational engines (omp, claude, codex, opencode)                   |
//! | `store`     | on-disk definitions, task logs, secrets, usage (`<hostd dir>/crew/<id>/`)   |
//! | `triggers`  | webhook, GitHub, file, service, crewmate-chain and schedule triggers        |
//! | `schedule`  | cron routines                                                               |
//! | `events`    | the push events broadcast to clients                                        |
//! | `pane`      | the `crew:<crewmate>:<task>` pane key                                       |
//! | `project`   | Git/path project checkout                                                   |
//! | `telegram`, `push` | delivery channels                                                    |
//! | `mcp`       | the MCP server an engine's agent talks to (`atlas mcp --crewmate <id>`)     |
//! | `relay`     | the Mac's relay to crew servers, for Atlas Mobile (runs on the Mac)         |

pub(crate) mod engine;
pub(crate) mod events;
pub(crate) mod github;
mod handlers;
mod hub;
pub mod mcp;
pub(crate) mod pane;
pub(crate) mod project;
pub(crate) mod push;
pub mod relay;
pub(crate) mod runner;
pub(crate) mod schedule;
pub(crate) mod store;
pub(crate) mod telegram;
#[cfg(test)]
mod tests;
pub(crate) mod triggers;

use serde_json::Value;

use crate::protocol::HostCmd;
use crate::server::Hub;

pub(crate) use hub::CrewHub;
pub(crate) use push::PushToken;
pub(crate) use store::{Crewmate, Task, now_s};

/// Route a crew `HostCmd`. Called from `server.rs handle()`.
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd) -> Result<Value, String> {
    use handlers::{crewmates as c, settings as s, tasks as t};
    if !hub.crew_host {
        return Err("this computer is not a crew server (atlas crew install)".into());
    }
    match cmd {
        HostCmd::CrewmateList {} => c::list(hub),
        HostCmd::CrewmateCreate {
            name,
            role,
            agent,
            approval,
            workdir,
            look,
            engine,
            project,
            setup_step,
        } => c::create(
            hub, name, role, agent, approval, workdir, look, engine, project, setup_step,
        ),
        HostCmd::CrewmateUpdate {
            id,
            name,
            role,
            agent,
            approval,
            look,
            engine,
            project,
            setup_step,
            short_replies,
        } => c::update(
            hub,
            id,
            name,
            role,
            agent,
            approval,
            look,
            engine,
            project,
            setup_step,
            short_replies,
        ),
        HostCmd::CrewmateDelete { id } => c::delete(hub, id),

        HostCmd::CrewmateTask {
            id,
            prompt,
            source,
            tz,
        } => t::send_task(hub, id, prompt, source, tz),
        HostCmd::CrewmateTasks { id, limit } => t::tasks(hub, id, limit),
        HostCmd::CrewmateCancel { id, task } => t::cancel(hub, id, task),
        HostCmd::CrewmateAnswer {
            id,
            ask_id,
            key,
            text,
        } => t::answer(hub, id, ask_id, key, text),
        HostCmd::CrewmateActionAnswer { id, task, value } => t::action_answer(hub, id, task, value),
        HostCmd::CrewmateUsage { id, since } => t::usage(hub, id, since),

        HostCmd::CrewmateMemory { id } => s::memory(hub, id),
        HostCmd::CrewmateMemorySet { id, text } => s::set_memory(hub, id, text),
        HostCmd::CrewmateRoutineSet { id, routines } => s::set_routines(hub, id, routines),
        HostCmd::CrewmateTriggersSet { id, triggers } => s::set_triggers(hub, id, triggers),
        HostCmd::CrewmateSecretSet { id, key, value } => s::set_secret(hub, id, key, value),
        HostCmd::CrewTelegramSet { token } => s::telegram_set(hub, token),
        HostCmd::CrewTelegramLink {} => s::telegram_link(hub),
        HostCmd::CrewTelegramStatus {} => s::telegram_status(hub),
        HostCmd::PushRegister { token, platform } => s::push_register(hub, token, platform),
        HostCmd::PushUnregister { token } => s::push_unregister(hub, token),
        HostCmd::CrewSettingsGet {} => s::get(hub),
        HostCmd::CrewSettingsSet {
            short_replies,
            short_replies_model,
        } => s::set(hub, short_replies, short_replies_model),
        HostCmd::EngineLimits {} => s::engine_limits(),

        HostCmd::CrewGithubStatus {} => github::status().await,
        HostCmd::CrewGithubLoginStart {} => github::login_start(hub).await,
        HostCmd::CrewGithubLoginPoll {} => github::login_poll(hub).await,
        HostCmd::CrewGithubLogout {} => github::logout(hub).await,
        HostCmd::CrewGithubConnectResume {} => github::connect_resume(hub),

        _ => Err("not a crew command".into()),
    }
}
