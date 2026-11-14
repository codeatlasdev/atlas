//! Integration tests of the crew subsystem, by area. Each one runs a real hostd (crew server mode)
//! on a temp dir with a fake agent binary (a `/bin/sh` script, so unix only); `fixture` has the
//! shared helpers.
//!
//! | file          | what it covers                                                           |
//! |---------------|--------------------------------------------------------------------------|
//! | `tasks`       | a task from queue to done/failed/cancelled, steps, usage, JSON contracts |
//! | `crewmates`   | create/update, the look, caps, secrets, the `crewmate_list` contract     |
//! | `triggers`    | routines, webhooks, crewmate-chain triggers, trigger validation          |
//! | `automations` | triggers in `crewmate_list`, the `add_trigger` action                    |
//! | `projects`    | Git/path projects, checkout before the first task                        |
//! | `engines`     | conversational engines: sessions, secrets, approvals, omp extension      |
//! | `actions`     | `ATLAS_ACTION` parsing, `waiting_action`, GitHub login                   |
//! | `approvals`   | approval prompts of a crewmate's pane                                    |
//! | `telegram`    | setting Telegram up from a client, the status payload                     |
//! | `payloads`    | payloads that are pure values: summarized task, the push event (Windows too) |
//! | `limits`      | name/role/prompt limits (characters, not bytes)                          |
//! | `push`        | `push_register`: the token and its relay grant, the phone's wire shape   |
//! | `pause`       | `crew_pause` / `crew_resume`: the kill switch, its persistence, its event |
//! | `untrusted`   | text from GitHub/webhooks: author filter, quoting, no bypass flags       |
//! | `github_in`   | GitHub events to tasks: mentions, assignment, review comments, CI logs   |
//! | `github_out`  | the progress comment: created, edited (throttled), finished, failed      |
//! | `github_fake` | the `GithubApi` fake both of those run against                           |
//! | `github_e2e`  | a real hostd polling the fake: mention -> task -> reply -> PR link       |

mod github_fake;
mod github_in;
mod github_out;
mod payloads;

#[cfg(unix)]
mod actions;
#[cfg(unix)]
mod approvals;
#[cfg(unix)]
mod automations;
#[cfg(unix)]
mod crewmates;
#[cfg(unix)]
mod engines;
#[cfg(unix)]
mod fixture;
#[cfg(unix)]
mod github_e2e;
#[cfg(unix)]
mod limits;
#[cfg(unix)]
mod pause;
#[cfg(unix)]
mod projects;
#[cfg(unix)]
mod push;
#[cfg(unix)]
mod tasks;
#[cfg(unix)]
mod telegram;
#[cfg(unix)]
mod triggers;
#[cfg(unix)]
mod untrusted;
