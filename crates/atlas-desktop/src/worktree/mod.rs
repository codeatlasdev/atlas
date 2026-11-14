//! Git worktrees as registry entries (`parent` = main checkout's id).
//!
//! Every git/gh/sh call goes through hostd `exec` (same path a remote host
//! will use). Creation is async: `worktree_create` returns a `creation_id` and
//! reports `worktree://progress {creation_id, step, message, worktree?, setup?}`
//! with step `fetch|add|shared|done|error`. The frontend opens the "Setup"
//! terminal from the `done` payload (it owns tabs).
//!
//! `exec` runs commands, `create` builds a worktree and `manage` removes, imports and inspects existing
//! ones. Names/offsets, the registry and the "nothing would be lost" rule are shared with hostd
//! (`atlas_host::worktrees`).

mod create;
mod exec;
mod manage;

pub use create::*;
pub use manage::*;

pub(crate) use exec::{exec, git};
