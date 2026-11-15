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
//! | `attachments` | images on a task: caps, type check, storage, the agent's copy             |
//! | `store`     | on-disk definitions, task logs, secrets, usage (`<hostd dir>/crew/<id>/`)   |
//! | `triggers`  | webhook, GitHub, file, service, crewmate-chain and schedule triggers        |
//! | `capture`   | the `capture` tool: Playwright screenshots of a URL, widths x themes x engines |
//! | `locate`    | the `locate` tool: which route, component and selector draw a region of a screenshot |
//! | `uigate`    | the `ui_gate` tool: plan per change, rerun only failures, regression replay |
//! | `schedule`  | cron routines                                                               |
//! | `events`    | the push events broadcast to clients                                        |
//! | `pane`      | the `crew:<crewmate>:<task>` pane key                                       |
//! | `project`   | Git/path project checkout                                                   |
//! | `spawn`     | the root helper protocol that starts each crewmate as its own Linux user    |
//! | `telegram`, `push` | delivery channels                                                    |
//! | `mcp`       | the MCP server an engine's agent talks to (`atlas mcp --crewmate <id>`)     |
//! | `relay`     | the Mac's relay to crew servers, for Atlas Mobile (runs on the Mac)         |

pub(crate) mod attachments;
pub(crate) mod audit;
pub mod capture;
pub(crate) mod engine;
pub(crate) mod engine_login;
pub(crate) mod events;
pub(crate) mod github;
mod handlers;
mod hub;
pub mod locate;
pub mod mcp;
pub(crate) mod pane;
pub(crate) mod proc;
pub(crate) mod project;
pub(crate) mod push;
pub mod relay;
pub(crate) mod runner;
pub(crate) mod schedule;
pub mod spawn;
pub(crate) mod store;
pub(crate) mod telegram;
#[cfg(test)]
mod tests;
pub(crate) mod triggers;
pub mod uigate;

use serde_json::{Value, json};

use crate::protocol::HostCmd;
use crate::server::Hub;

pub use store::archive::{self, ImportError, ImportReport, Manifest};

/// `atlas hostd --crew-export`: the archive of this hostd dir's crewmates to `out`. Reads the files
/// directly, so it works with the daemon stopped (an uninstall exports before it removes anything).
pub fn export_archive(
    dir: &std::path::Path,
    ids: &[String],
    with_secrets: bool,
    out: &mut impl std::io::Write,
) -> anyhow::Result<Manifest> {
    archive::export(&store::CrewStore::new(dir), ids, with_secrets, out)
}

/// `atlas hostd --crew-import`: the other direction. A running daemon picks the new crewmates up on its
/// next list; nothing here needs it.
pub fn import_archive(
    dir: &std::path::Path,
    input: impl std::io::Read,
    rename: bool,
) -> Result<ImportReport, ImportError> {
    archive::import(&store::CrewStore::new(dir), input, rename)
}

pub(crate) use hub::CrewHub;
pub(crate) use push::PushToken;
pub(crate) use store::{Crewmate, Task, now_s};

/// Route a crew `HostCmd`. Called from `server.rs handle()`.
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd) -> Result<Value, String> {
    use handlers::{audit as a, crewmates as c, settings as s, tasks as t};
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
        HostCmd::CrewmateDelete { id } => c::delete(hub, id).await,

        HostCmd::CrewmateTask {
            id,
            prompt,
            source,
            tz,
            attachments,
            ui,
        } => t::send_task(hub, id, prompt, source, tz, attachments, ui),
        HostCmd::CrewmateAttachmentPut {
            id,
            upload,
            seq,
            data_b64,
            last,
        } => t::attachment_put(hub, id, upload, seq, data_b64, last),
        HostCmd::CrewmateAttachment {
            id,
            task,
            name,
            offset,
            limit,
        } => t::attachment(hub, id, task, name, offset, limit),
        HostCmd::CrewmateTasks { id, limit } => t::tasks(hub, id, limit),
        HostCmd::CrewmateCancel { id, task } => t::cancel(hub, id, task),
        HostCmd::CrewmateAnswer {
            id,
            ask_id,
            key,
            text,
        } => t::answer(hub, id, ask_id, key, text),
        HostCmd::CrewmateActionAnswer {
            id,
            task,
            value,
            attachments,
        } => t::action_answer(hub, id, task, value, attachments),
        HostCmd::CrewmateUsage { id, since } => t::usage(hub, id, since),
        HostCmd::CrewmateAudit {
            id,
            since,
            limit,
            kind,
        } => a::read(hub, Some(id), since, limit, kind),
        HostCmd::CrewAudit { since, limit, kind } => a::read(hub, None, since, limit, kind),

        HostCmd::CrewmateMemory { id } => s::memory(hub, id),
        HostCmd::CrewmateMemorySet { id, text } => s::set_memory(hub, id, text),
        HostCmd::CrewmateRoutineSet { id, routines } => s::set_routines(hub, id, routines),
        HostCmd::CrewmateTriggersSet { id, triggers } => s::set_triggers(hub, id, triggers),
        HostCmd::CrewmateSecretSet { id, key, value } => s::set_secret(hub, id, key, value),
        HostCmd::CrewTelegramSet { token } => s::telegram_set(hub, token),
        HostCmd::CrewTelegramLink {} => s::telegram_link(hub),
        HostCmd::CrewTelegramStatus {} => s::telegram_status(hub),
        HostCmd::CrewSettingsGet {} => s::get(hub),
        HostCmd::CrewSettingsSet {
            short_replies,
            short_replies_model,
            max_agents,
        } => s::set(hub, short_replies, short_replies_model, max_agents),
        HostCmd::CrewPause {} => hub.crew.set_paused(hub, true).map(|()| json!({})),
        HostCmd::CrewResume {} => hub.crew.set_paused(hub, false).map(|()| json!({})),
        HostCmd::EngineLimits {} => s::engine_limits(),
        HostCmd::CrewEngineStatus { refresh, crewmate } => {
            engine_login::status(hub, refresh, crewmate.as_deref()).await
        }

        HostCmd::CrewGithubStatus {} => github::status().await,
        HostCmd::CrewGithubLoginStart {} => github::login_start(hub).await,
        HostCmd::CrewGithubLoginPoll {} => github::login_poll(hub).await,
        HostCmd::CrewGithubLogout {} => github::logout(hub).await,
        HostCmd::CrewGithubConnectResume {} => github::connect_resume(hub),

        _ => Err("not a crew command".into()),
    }
}
