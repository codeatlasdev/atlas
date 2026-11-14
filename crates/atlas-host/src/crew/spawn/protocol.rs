//! The wire types of the crew spawn helper: what hostd (user `atlas-crew`) may ask the root helper
//! for, and what comes back. Plain serde types, no I/O: [`super::validate`] decides which requests
//! are acceptable, [`super::wire`] frames them.
//!
//! One message is one length-prefixed JSON document (see `wire`); a spawn reply carries the
//! pty master (or one end of a socketpair) as `SCM_RIGHTS` ancillary data, never in the JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Largest request or reply body the helper reads. A spawn request is a few hundred bytes; the
/// cap keeps a hostile or buggy peer from making a root process allocate.
pub const MAX_MESSAGE: usize = 64 * 1024;

/// How the child's stdio is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// A pty: the reply carries the master, the child gets the slave as controlling terminal.
    Pty,
    /// One end of a `socketpair` as stdin/stdout, stderr merged: headless engines.
    Pipe,
}

/// Start `argv` as the crewmate's own user. Everything here is data: the helper never runs a shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnReq {
    /// The crewmate the child belongs to (a provisioned `atlas-cm-<slug>` user).
    pub user: String,
    /// `argv[0]` must be one of [`super::validate::ENGINE_BINARIES`], resolved by the helper in
    /// its own pinned directory, never from the request.
    pub argv: Vec<String>,
    /// Environment the child gets, on top of a fixed minimal one.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Working directory; must be inside `/var/lib/atlas-cm/<slug>/`.
    pub cwd: String,
    pub mode: Mode,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_cols() -> u16 {
    120
}
fn default_rows() -> u16 {
    40
}

/// One request to the helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Spawn(SpawnReq),
    /// Signal a session this helper started. `sig` is one of [`super::validate::SIGNALS`].
    Signal {
        session: u64,
        sig: i32,
    },
    /// Exit status of a session: `exited: false` while it runs, the code when it is known.
    Wait {
        session: u64,
    },
    /// Working directory of a session's process (replaces `lsof` across uids).
    Cwd {
        session: u64,
    },
}

/// The helper's answer. A successful `Spawn` carries one fd next to this message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Spawned {
        session: u64,
        pid: u32,
    },
    Done,
    Exit {
        exited: bool,
        code: Option<i32>,
    },
    Cwd {
        path: String,
    },
    /// The request was refused or failed; `reason` is for humans and logs.
    Error {
        reason: String,
    },
}
