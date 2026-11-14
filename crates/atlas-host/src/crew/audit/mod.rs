//! The append-only audit of what crewmates do and who decided what (issue #201): every tool call
//! the agent reports (full name and input), every approval asked and how it ended, task lifecycle,
//! configuration changes, pause/resume, crewmate create/delete.
//!
//! One writer ([`Audit::record`]) behind one lock, one JSON object per line:
//! `{seq, ts, kind, crewmate_id?, ...fields}`.
//!
//! - A crewmate's entries go to `<crew dir>/<id>/audit.jsonl`; crew-wide ones (create/delete,
//!   pause/resume) to `<base>/crew-audit.jsonl`, which survives a crewmate's deletion.
//! - Redaction runs on every write: the values of the crewmate's secrets and the hook token (both read
//!   at write time, so a secret set a minute ago is covered) become `[secret:NAME]`; a string over
//!   [`MAX_STRING`] is cut with a marker saying how much was dropped.
//! - Bounded: a file over `max_bytes` rotates to `audit.jsonl.1` …, keeping `keep` files in all.
//! - Owner-only (0600): it holds tool inputs.
//!
//! The recorders live where the facts are: [`observe`] watches the agent hub for tool calls and
//! approvals, the runner records task lifecycle, the handlers record changes.

mod observe;
mod redact;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde_json::{Map, Value};

pub(crate) use observe::observe;

use super::store::{CrewStore, now_s};

/// File name of a crewmate's log (and of the rotated `audit.jsonl.1` …).
const FILE: &str = "audit.jsonl";
/// The crew-wide log, next to the crewmate directories (never inside `crew/`, whose children are crewmates).
const CREW_FILE: &str = "crew-audit.jsonl";
/// A file rotates once it holds this much.
const MAX_BYTES: u64 = 1024 * 1024;
/// Files kept per log (the live one and the rotated ones): about 5 MiB at most.
const KEEP: usize = 5;
/// The longest string stored (bytes); a longer one is cut and says so.
pub(crate) const MAX_STRING: usize = 8 * 1024;
/// How many entries `crewmate_audit` returns when the client gives no limit.
pub(crate) fn default_limit() -> usize {
    200
}

/// Which entries to read (`crewmate_audit` / `crew_audit`).
#[derive(Debug, Clone)]
pub(crate) struct Query {
    /// Only entries at or after this time (unix seconds).
    pub since: Option<u64>,
    /// Newest first, at most this many.
    pub limit: usize,
    /// Only this `kind`.
    pub kind: Option<String>,
}

pub(crate) struct Audit {
    store: CrewStore,
    base: PathBuf,
    max_bytes: u64,
    keep: usize,
    /// One writer: the lock covers the sequence number, the rotation and the append, per log path.
    seq: Mutex<HashMap<PathBuf, u64>>,
}

impl Audit {
    pub(crate) fn new(base: &Path) -> Self {
        Self::with_limits(base, MAX_BYTES, KEEP)
    }

    pub(crate) fn with_limits(base: &Path, max_bytes: u64, keep: usize) -> Self {
        Self {
            store: CrewStore::new(base),
            base: base.to_path_buf(),
            max_bytes,
            keep: keep.max(1),
            seq: Mutex::new(HashMap::new()),
        }
    }

    fn path(&self, crewmate: Option<&str>) -> PathBuf {
        match crewmate {
            Some(id) => self.store.crewmate_dir(id).join(FILE),
            None => self.base.join(CREW_FILE),
        }
    }

    /// Append one entry. `fields` is a JSON object merged into the entry; anything else is ignored.
    /// A failure is logged (`atlas crew logs`), never raised: the audit must not stop the work it
    /// records, but it must not fail silently either.
    pub(crate) fn record(&self, crewmate: Option<&str>, kind: &str, fields: Value) {
        if let Err(e) = self.try_record(crewmate, kind, fields) {
            eprintln!(
                "atlas crew: audit: cannot record {kind} for {}: {e}",
                crewmate.unwrap_or("the crew")
            );
        }
    }

    fn try_record(&self, crewmate: Option<&str>, kind: &str, fields: Value) -> std::io::Result<()> {
        let path = self.path(crewmate);
        let mut entry = Map::new();
        entry.insert("ts".into(), now_s().into());
        entry.insert("kind".into(), kind.into());
        if let Some(id) = crewmate {
            entry.insert("crewmate_id".into(), id.into());
        }
        if let Value::Object(f) = fields {
            entry.extend(f);
        }
        let mut entry = Value::Object(entry);
        let secrets = crewmate
            .map(|id| redact::secrets_of(&self.store, id))
            .unwrap_or_default();
        redact::scrub(&mut entry, &secrets, &redact::hook_token(&self.base));

        let mut seqs = self.seq.lock();
        let seq = match seqs.get(&path) {
            Some(n) => n + 1,
            None => last_seq(&path, self.keep) + 1,
        };
        seqs.insert(path.clone(), seq);
        entry["seq"] = seq.into();
        let mut line = entry.to_string();
        line.push('\n');

        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        if std::fs::metadata(&path).is_ok_and(|m| m.len() + line.len() as u64 > self.max_bytes) {
            self.rotate(&path)?;
        }
        atlas_core::sys::owner_only(std::fs::OpenOptions::new().create(true).append(true))
            .open(&path)?
            .write_all(line.as_bytes())
    }

    /// `audit.jsonl` → `.1` → `.2` …, the oldest dropped so at most `keep` files remain.
    fn rotate(&self, path: &Path) -> std::io::Result<()> {
        let nth = |n: usize| {
            let mut s = path.as_os_str().to_owned();
            s.push(format!(".{n}"));
            PathBuf::from(s)
        };
        let _ = std::fs::remove_file(nth(self.keep - 1));
        for n in (1..self.keep - 1).rev() {
            if nth(n).exists() {
                std::fs::rename(nth(n), nth(n + 1))?;
            }
        }
        if self.keep > 1 {
            std::fs::rename(path, nth(1))
        } else {
            std::fs::remove_file(path)
        }
    }

    /// Entries matching `q`, newest first. A line that does not parse (a torn write) is skipped.
    pub(crate) fn read(&self, crewmate: Option<&str>, q: &Query) -> Vec<Value> {
        let path = self.path(crewmate);
        let _writer = self.seq.lock(); // not mid-rotation
        let mut out: Vec<Value> = vec![];
        for p in files_newest_first(&path, self.keep) {
            let Ok(f) = std::fs::File::open(&p) else {
                continue;
            };
            let mut file_entries: Vec<Value> = std::io::BufReader::new(f)
                .lines()
                .map_while(Result::ok)
                .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
                .filter(|e| entry_matches(e, q))
                .collect();
            file_entries.reverse();
            out.extend(file_entries);
            if out.len() >= q.limit {
                break;
            }
        }
        out.truncate(q.limit);
        out
    }
}

fn entry_matches(e: &Value, q: &Query) -> bool {
    q.since.is_none_or(|s| e["ts"].as_u64().unwrap_or(0) >= s)
        && q.kind.as_deref().is_none_or(|k| e["kind"] == k)
}

/// The live file first, then `.1`, `.2` … (newest to oldest).
fn files_newest_first(path: &Path, keep: usize) -> Vec<PathBuf> {
    let mut v = vec![path.to_path_buf()];
    for n in 1..keep {
        let mut s = path.as_os_str().to_owned();
        s.push(format!(".{n}"));
        v.push(PathBuf::from(s));
    }
    v
}

/// The highest `seq` on disk (0 when there is none): numbering continues after a restart.
fn last_seq(path: &Path, keep: usize) -> u64 {
    for p in files_newest_first(path, keep) {
        let Ok(f) = std::fs::File::open(&p) else {
            continue;
        };
        let last = std::io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str::<Value>(&l).ok()?["seq"].as_u64())
            .last();
        if let Some(n) = last {
            return n;
        }
    }
    0
}
