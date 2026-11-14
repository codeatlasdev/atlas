//! Task cache: a machine-wide, content-addressed store of what a command produced, so an agent
//! (or a worktree after a rebase) never reruns identical work.
//!
//! * [`Spec`] says what a run depends on; [`Spec::key`] hashes the argv, the bytes of every input
//!   file as they are on disk now (uncommitted edits included, `.gitignore` respected when a glob
//!   is expanded), the declared env values and the `--version` of the declared tools. The
//!   directory the command runs in is not part of the key: two worktrees with the same bytes
//!   share a result, two with different edits never do.
//! * [`Cache`] is the store (`~/.atlas/cache`, `ATLAS_CACHE_DIR` moves it): blobs by sha256
//!   (outputs and logs, shared across entries and worktrees), entries by key, LRU eviction under
//!   `cache.max_bytes`, atomic writes, one advisory lock for every mutation.
//! * [`stats`] holds the counters (hits, misses, time saved) the desktop reads, and the recent
//!   runs behind the MCP tool `last_result`.
//!
//! Layout of the store:
//! `blobs/<aa>/<sha256>`, `entries/<key>.json`, `stats.json`, `recent.json`, `lock`.

mod key;
mod log;
mod run;
pub mod stats;
mod store;
#[cfg(test)]
mod tests;
pub mod wrap;

pub use key::{Key, Spec};
pub use log::{Chunk, Stream};
pub use run::{Recorded, Restored, Run};
pub use stats::{LastResult, Stats, Why};
pub use store::{Cache, Entry, Freed, OutFile, Status};
