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
//! | `pause`       | `crew_pause` / `crew_resume`: the kill switch, its persistence, its event |
//! | `untrusted`   | text from GitHub/webhooks: author filter, quoting, no bypass flags       |

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
mod limits;
#[cfg(unix)]
mod pause;
#[cfg(unix)]
mod projects;
#[cfg(unix)]
mod tasks;
#[cfg(unix)]
mod telegram;
#[cfg(unix)]
mod triggers;
#[cfg(unix)]
mod untrusted;
