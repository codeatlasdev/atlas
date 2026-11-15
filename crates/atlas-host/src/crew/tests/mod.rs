//! Integration tests of the crew subsystem, by area. Each one runs a real hostd (crew server mode)
//! on a temp dir with a fake agent binary (a `/bin/sh` script, so unix only); `fixture` has the
//! shared helpers.
//!
//! | file          | what it covers                                                           |
//! |---------------|--------------------------------------------------------------------------|
//! | `tasks`       | a task from queue to done/failed/cancelled, steps, usage, JSON contracts |
//! | `capture`     | the `capture` tool through a real hostd with a fake runner                |
//! | `locate`      | the `locate` tool through a real hostd with a fake runner                 |
//! | `uigate`      | the `ui_gate` tool through a real hostd: plan, cache hit, regression replay |
//! | `crewmates`   | create/update, the look, caps, secrets, the `crewmate_list` contract     |
//! | `triggers`    | routines, webhooks, crewmate-chain triggers, trigger validation          |
//! | `automations` | triggers in `crewmate_list`, the `add_trigger` action                    |
//! | `projects`    | Git/path projects, checkout before the first task                        |
//! | `engines`     | conversational engines: sessions, secrets, approvals, omp extension      |
//! | `engine_stop` | pause, cancel and the task timeout end an engine turn and its child process |
//! | `actions`     | `ATLAS_ACTION` parsing, `waiting_action`, GitHub login                   |
//! | `approvals`   | approval prompts of a crewmate's pane                                    |
//! | `audit`       | the append-only audit: tool calls, approvals, lifecycle, config, restart  |
//! | `telegram`    | setting Telegram up from a client, the status payload                     |
//! | `payloads`    | payloads that are pure values: summarized task, the push event (Windows too) |
//! | `guards`      | the guards against the agent: refusal parks the task, override and retry |
//! | `hygiene`     | chain loops, GitHub dedupe and baseline, a delete with a turn still running |
//! | `limits`      | name/role/prompt limits (characters, not bytes)                          |
//! | `mobile_payloads` | the audit, usage, engine-limits and settings replies the phone's crew screens read |
//! | `push`        | `push_register`: the token and its relay grant, the phone's wire shape   |
//! | `pause`       | `crew_pause` / `crew_resume`: the kill switch, its persistence, its event |
//! | `restart`     | tasks in flight when hostd restarts: resumed, capped, answered, held while paused |
//! | `untrusted`   | text from GitHub/webhooks: author filter, quoting, no bypass flags       |
//! | `github_in`   | GitHub events to tasks: mentions, assignment, review comments, CI logs   |
//! | `github_out`  | the progress comment: created, edited (throttled), finished, failed      |
//! | `github_fake` | the `GithubApi` fake both of those run against                           |
//! | `github_e2e`  | a real hostd polling the fake: mention -> task -> reply -> PR link       |
//! | `attachments` | images on tasks: stored, staged, refused, fetched, from Telegram, scopes |
//! | `prove_ui`    | a `ui` task needs two agent captures to be done: the card, override/retry, Telegram |

pub(crate) mod github_fake;
mod github_in;
mod github_out;
mod payloads;

#[cfg(unix)]
mod actions;
#[cfg(unix)]
mod approvals;
#[cfg(unix)]
mod attachments;
#[cfg(unix)]
mod audit;
#[cfg(unix)]
mod automations;
#[cfg(unix)]
mod capture;
#[cfg(unix)]
mod crewmates;
#[cfg(unix)]
mod engine_stop;
#[cfg(unix)]
mod engines;
#[cfg(unix)]
mod fixture;
#[cfg(unix)]
mod github_e2e;
#[cfg(unix)]
mod guards;
#[cfg(unix)]
mod hygiene;
#[cfg(unix)]
mod limits;
#[cfg(unix)]
mod locate;
#[cfg(unix)]
mod mobile_payloads;
#[cfg(unix)]
mod pause;
#[cfg(unix)]
mod prepare;
#[cfg(unix)]
mod projects;
#[cfg(unix)]
mod prove_ui;
#[cfg(unix)]
mod push;
#[cfg(unix)]
mod restart;
#[cfg(unix)]
mod tasks;
#[cfg(unix)]
mod telegram;
#[cfg(unix)]
mod triggers;
#[cfg(unix)]
mod uigate;
#[cfg(unix)]
mod untrusted;
