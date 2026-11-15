//! `tasks.jsonl` and its in-memory index.
//!
//! The file is an append-only log: the last full record of an id is the task. Two kinds of line:
//!
//! - a full [`Task`] (every state change);
//! - a step delta `{"step_of":"<id>","step":{..}}` (the runner's live progress): one new [`Step`] for a
//!   task that already has a full record. Writing the whole task (prompt, every step so far) for each of
//!   up to 50 steps made a task cost ~50 copies of itself (#295).
//!
//! Reading is by index: the file is parsed once per process and then only what was appended since, so
//! `crewmate_list` (the latest task) and the 30 s cancel check cost nothing per call. The index is kept
//! up to date by every write, which all go through [`CrewStore`] under one lock ([`LOGS`]).
//!
//! The file is compacted (rewritten with one full record per id, atomically) once it has doubled since
//! the last compaction and holds at least [`COMPACT_MIN`]: history is never dropped, only superseded
//! copies. Files written before deltas existed are plain full records and read unchanged; a file with
//! deltas read by an older hostd just misses the in-flight steps until the task's final full record.

use std::collections::HashMap;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Step, Task, jsonl};
use crate::paths;

/// A task log is not compacted below this size.
pub(super) const COMPACT_MIN: u64 = 4 * 1024 * 1024;

/// Every task log of the process, by file. One lock for all of them: a read-modify-append
/// ([`CrewStore::update_task`], [`CrewStore::append_runner_state`]) is never split by another write,
/// whichever [`CrewStore`] instance (handlers build their own over the same files) makes it.
pub(super) static LOGS: parking_lot::Mutex<Option<HashMap<PathBuf, TaskLog>>> =
    parking_lot::Mutex::new(None);

#[derive(Serialize, Deserialize)]
struct StepDelta {
    step_of: String,
    step: Step,
}

/// What the file holds, parsed.
#[derive(Default)]
pub(super) struct TaskLog {
    /// How much of the file `tasks` reflects (a torn final line is not part of it until it is
    /// terminated by the next append).
    len: u64,
    tasks: HashMap<String, Task>,
    /// Ids in the order their first record appears: the order tasks were created in.
    order: Vec<String>,
    /// Lines that are not a record this version knows (a torn write, a newer hostd's record). Compaction
    /// keeps them as they are instead of dropping what it cannot read.
    unknown: Vec<String>,
    /// The size that triggers the next compaction.
    compact_at: u64,
    /// Bytes read from the file since the index was built (tests check a read does not parse it again).
    #[cfg(test)]
    pub(super) bytes_read: u64,
}

impl TaskLog {
    /// Bring the index up to date with the file: parse what was appended since, or everything when the
    /// file is gone or shorter than what was parsed (replaced).
    pub(super) fn sync(&mut self, path: &Path) {
        let Ok(mut f) = std::fs::File::open(path) else {
            *self = Self::default();
            return;
        };
        let file_len = f.metadata().map_or(0, |m| m.len());
        if file_len < self.len {
            *self = Self::default();
        }
        if file_len == self.len {
            return;
        }
        if f.seek(SeekFrom::Start(self.len)).is_err() {
            return;
        }
        let mut consumed = 0u64;
        for line in jsonl::lines(io::BufReader::new(f)) {
            #[cfg(test)]
            {
                self.bytes_read += line.bytes as u64;
            }
            // A final line with no newline counts only when it is a whole record (a file written by
            // something else); a torn one is left for the next append to terminate.
            if self.apply(&line.text) {
                consumed += line.bytes as u64;
            } else if line.terminated {
                self.unknown.push(line.text);
                consumed += line.bytes as u64;
            } else {
                break;
            }
        }
        self.len += consumed;
    }

    /// Fold one line into the index. `false` when it is not a record this version reads (blank lines count as read).
    fn apply(&mut self, line: &str) -> bool {
        if line.trim().is_empty() {
            return true;
        }
        if line.starts_with("{\"step_of\"") {
            match serde_json::from_str::<StepDelta>(line) {
                Ok(d) => {
                    // A delta needs its task's full record; one without is dropped (nothing to extend).
                    if let Some(t) = self.tasks.get_mut(&d.step_of) {
                        t.steps.push(d.step);
                    }
                    return true;
                }
                Err(_) => return false,
            }
        }
        match serde_json::from_str::<Task>(line) {
            Ok(t) => {
                if self.tasks.insert(t.id.clone(), t.clone()).is_none() {
                    self.order.push(t.id);
                }
                true
            }
            Err(_) => false,
        }
    }

    pub(super) fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Newest-created first, at most `limit`.
    pub(super) fn newest(&self, limit: usize) -> Vec<Task> {
        self.order
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.tasks.get(id).cloned())
            .collect()
    }

    /// Append the full record of `task`.
    pub(super) fn push_task(
        &mut self,
        path: &Path,
        task: &Task,
        compact_min: u64,
    ) -> io::Result<()> {
        let line = serde_json::to_string(task)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.push_line(path, &line, true)?;
        self.compact_if_due(path, compact_min);
        Ok(())
    }

    /// Append one step to the task `id`, which must already have a full record.
    pub(super) fn push_step(
        &mut self,
        path: &Path,
        id: &str,
        step: &Step,
        compact_min: u64,
    ) -> io::Result<()> {
        if !self.tasks.contains_key(id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("task {id} has no record to add a step to"),
            ));
        }
        let line = serde_json::to_string(&StepDelta {
            step_of: id.to_string(),
            step: step.clone(),
        })
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Not flushed: a step lost to a power cut is replaced by the full record of the next state change.
        self.push_line(path, &line, false)?;
        self.compact_if_due(path, compact_min);
        Ok(())
    }

    /// `sync` ran just before, so the index covers the file up to `self.len`.
    fn push_line(&mut self, path: &Path, line: &str, durable: bool) -> io::Result<()> {
        let new_len = jsonl::append_line(path, line, durable)?;
        self.apply(line);
        self.len = new_len;
        Ok(())
    }

    fn compact_if_due(&mut self, path: &Path, min: u64) {
        if self.len < self.compact_at.max(min) {
            return;
        }
        let mut out = Vec::new();
        for l in &self.unknown {
            out.extend_from_slice(l.as_bytes());
            out.push(b'\n');
        }
        for id in &self.order {
            if let Some(t) = self.tasks.get(id)
                && serde_json::to_writer(&mut out, t).is_ok()
            {
                out.push(b'\n');
            }
        }
        match paths::write_private(path, &out) {
            Ok(()) => {
                self.len = out.len() as u64;
                // Records that are all live shrink nothing: wait for the file to double before trying again.
                self.compact_at = self.len * 2;
            }
            Err(e) => {
                eprintln!("atlas crew: cannot compact {}: {e}", path.display());
                self.compact_at = self.len * 2;
            }
        }
    }
}
