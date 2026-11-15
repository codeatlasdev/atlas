//! `tasks.jsonl` and its in-memory index.
//!
//! The file is an append-only log: the last full record of an id is the task. Two kinds of line:
//!
//! - a full [`Task`] (every state change);
//! - a step delta `{"step_of":"<id>","step":{..}}` (the runner's live progress): one new [`Step`] for a
//!   task that already has a full record. Writing the whole task (prompt, every step so far) for each of
//!   up to 50 steps made a task cost ~50 copies of itself (#295).
//!
//! Reading is by index: the file is parsed once and then only what was appended since, so
//! `crewmate_list` (the latest task) and the 30 s cancel check cost nothing per call. The index is kept
//! up to date by every write, which all go through [`CrewStore`] under one lock ([`LOGS`]).
//!
//! The index is also saved next to the log as `tasks.idx` (the folded tasks, their order and a
//! fingerprint of the bytes they came from), so the first read after a restart parses only what was
//! appended after it was saved instead of the whole file. An order cannot be rebuilt from a tail (an old
//! task can have the newest record), which is why the index is saved rather than the file read backwards.
//! It is written when it is built or caught up and when the log is compacted, not on every append: the
//! next open parses the few lines since. Anything wrong with it (missing, corrupt, another version, a log
//! that is not the one it was made from) means a full parse and a new index, never an error.
//!
//! The file is compacted (rewritten with one full record per id, atomically) once it has doubled since
//! the last compaction and holds at least [`COMPACT_MIN`]: history is never dropped, only superseded
//! copies. Files written before deltas existed are plain full records and read unchanged; a file with
//! deltas read by an older hostd just misses the in-flight steps until the task's final full record.

use std::collections::HashMap;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Step, Task, jsonl};
use crate::paths;

/// A task log is not compacted below this size.
pub(super) const COMPACT_MIN: u64 = 4 * 1024 * 1024;

/// Every task log of the process, by file. One lock for all of them: a read-modify-append
/// ([`CrewStore::update_task`], [`CrewStore::append_runner_state`]) is never split by another write,
/// whichever [`CrewStore`] instance (handlers build their own over the same files) makes it.
pub(super) static LOGS: parking_lot::Mutex<Option<HashMap<PathBuf, TaskLog>>> =
    parking_lot::Mutex::new(None);

/// `tasks.idx` format; an index of another version is not read.
const IDX_VERSION: u32 = 1;

/// How much of the log is hashed at each of three places (start, middle, end of the indexed part).
const FINGERPRINT_WINDOW: u64 = 4096;

/// `<dir>/tasks.jsonl` → `<dir>/tasks.idx`
pub(super) fn idx_path(log: &Path) -> PathBuf {
    log.with_extension("idx")
}

/// The saved form of a [`TaskLog`].
#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    /// Bytes of the log the rest describes.
    len: u64,
    /// [`fingerprint`] of those bytes.
    fingerprint: String,
    tasks: HashMap<String, Task>,
    order: Vec<String>,
    unknown: Vec<String>,
    compact_at: u64,
}

/// [`Saved`] by reference, so saving does not copy the tasks.
#[derive(Serialize)]
struct SavedRef<'a> {
    version: u32,
    len: u64,
    fingerprint: &'a str,
    tasks: &'a HashMap<String, Task>,
    order: &'a [String],
    unknown: &'a [String],
    compact_at: u64,
}

/// Length plus a hash of three windows of the first `len` bytes of `f` (start, middle, end). Enough to tell
/// the log the index was made from from a compacted, restored or truncated one, without reading it all.
fn fingerprint(f: &mut std::fs::File, len: u64) -> io::Result<String> {
    let mut h = Sha256::new();
    h.update(len.to_le_bytes());
    let w = FINGERPRINT_WINDOW.min(len);
    let mut buf = vec![0u8; w as usize];
    for at in [0, (len - w) / 2, len - w] {
        f.seek(SeekFrom::Start(at))?;
        io::Read::read_exact(f, &mut buf)?;
        h.update(&buf);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

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
    /// Whether `tasks.idx` was written or loaded by this process: the first write of a process that never
    /// parsed anything (a new log) still leaves one.
    idx_saved: bool,
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
        // A new process: start from the saved index when it describes this very file.
        let mut from_idx = false;
        if self.len == 0 && file_len > 0 {
            from_idx = self.load(path, &mut f, file_len);
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
        // Built or caught up: the next process starts from here. (Not on every append: see the module docs.)
        if consumed > 0 || from_idx {
            self.save(path);
        }
    }

    /// Take the saved index of the log at `path` when it was made from the first `saved.len` bytes of `f`.
    /// `false` (and `self` untouched) when there is none or it cannot be trusted.
    fn load(&mut self, path: &Path, f: &mut std::fs::File, file_len: u64) -> bool {
        let Ok(raw) = std::fs::read(idx_path(path)) else {
            return false;
        };
        let Ok(saved) = serde_json::from_slice::<Saved>(&raw) else {
            return false;
        };
        let ok = saved.version == IDX_VERSION
            && saved.len > 0
            && saved.len <= file_len
            && fingerprint(f, saved.len).is_ok_and(|fp| fp == saved.fingerprint)
            // The order and the tasks must describe each other (an index edited or from a bug).
            && saved.order.len() == saved.tasks.len()
            && saved.order.iter().all(|id| saved.tasks.contains_key(id));
        if !ok {
            return false;
        }
        self.len = saved.len;
        self.tasks = saved.tasks;
        self.order = saved.order;
        self.unknown = saved.unknown;
        self.compact_at = saved.compact_at;
        self.idx_saved = true;
        true
    }

    /// Write `tasks.idx` for what the index holds now. Best effort: without it the next open parses the log.
    fn save(&mut self, path: &Path) {
        if self.len == 0 {
            return;
        }
        let Ok(mut f) = std::fs::File::open(path) else {
            return;
        };
        let Ok(fp) = fingerprint(&mut f, self.len) else {
            return;
        };
        let saved = serde_json::to_vec(&SavedRef {
            version: IDX_VERSION,
            len: self.len,
            fingerprint: &fp,
            tasks: &self.tasks,
            order: &self.order,
            unknown: &self.unknown,
            compact_at: self.compact_at,
        });
        let written = saved
            .map_err(io::Error::from)
            .and_then(|b| paths::write_private(&idx_path(path), &b));
        self.idx_saved = true;
        if let Err(e) = written {
            eprintln!("atlas crew: cannot save {}: {e}", idx_path(path).display());
        }
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
        if !self.idx_saved {
            self.save(path);
        }
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
                // The old index describes the file that was just replaced.
                self.save(path);
            }
            Err(e) => {
                eprintln!("atlas crew: cannot compact {}: {e}", path.display());
                self.compact_at = self.len * 2;
            }
        }
    }
}
