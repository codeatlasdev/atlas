//! The values that cross the cache's API: a finished run going in, the verdicts coming out.

use super::log::Chunk;
use super::stats::Why;

/// A command that has run, as the cache is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Project name (the folder its root resolves to), for the counters.
    pub project: String,
    /// Task or action label; what `last_result` is asked by.
    pub label: String,
    /// The command line, for humans and for `last_result`.
    pub command: String,
    pub exit: i32,
    pub duration_ms: u64,
    /// What it printed, in order, by stream.
    pub log: Vec<Chunk>,
}

/// What [`Cache::record`](super::Cache::record) did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Stored,
    NotStored(Why),
}

/// What [`Cache::restore`](super::Cache::restore) put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restored {
    pub files: usize,
    pub bytes: u64,
}
