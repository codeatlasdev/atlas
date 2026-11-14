//! Control channel for a running `atlas dev` (TUI or `--headless`).
//!
//! A Unix domain socket at `.logs/atlas.sock` accepts line-delimited JSON:
//! request `{"cmd": "...", "args": {...}}` → response `{"ok": bool, "error"?, "data"?}`.
//! The server only parses and forwards: every request becomes a [`ControlRequest`]
//! on a channel owned by the run loop, which turns it into the same Messages /
//! Effects / manager calls the keyboard uses (so `confirm`, `pause_services` and
//! in-flight guards apply). Clients (the `atlas mcp` server) use [`call`].
//!
//! Beyond service control the socket carries the runner/attach protocol:
//! `info` (who owns this run), `shutdown` (graceful stop, `atlas down`) and
//! `subscribe` (snapshot reply, then one pushed JSON line per event until either
//! side hangs up, see `runner.rs`). A subscription connection is read-only; an
//! attached client sends its requests on separate connections.

pub mod audit;
mod client;
mod guard;
mod protocol;
mod server;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use client::{RunInfo, Subscription, call, call_as, info, subscribe};
pub use guard::Guard;
pub use protocol::{
    Caller, ControlCmd, ControlRequest, ControlResponse, Resolved, find_action, resolve,
};
pub use server::ControlServer;
pub(crate) use server::read_bounded_line;

pub const SOCKET_NAME: &str = "atlas.sock";
/// Longest a client waits for the owning loop to answer one request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// Requests larger than this are rejected (a control line is a few hundred bytes).
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;
/// Events buffered per subscriber before the hub drops it as a stalled reader.
pub(crate) const SUBSCRIBER_BUFFER: usize = 4096;

pub const NOT_RUNNING: &str = "atlas dev is not running; start it in a terminal";

pub fn socket_path(root_dir: &Path) -> PathBuf {
    crate::runtime::log_dir(root_dir).join(SOCKET_NAME)
}
