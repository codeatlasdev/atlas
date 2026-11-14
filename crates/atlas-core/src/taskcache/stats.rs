//! Counters and recent runs of the task cache.
//!
//! [`Stats`] (`stats.json`) is the small file the desktop Machine drawer reads: hits, misses and
//! the time hits saved (the original duration of the run, on every hit), in total, per project
//! and per key. `recent.json` keeps the last runs with a log tail: what the MCP tool
//! `last_result` answers from, failures included.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Runs kept for `last_result`.
pub const RECENT_KEEP: usize = 64;
/// Per-key counters kept (the least recently used go first).
pub const KEYS_KEEP: usize = 512;

/// Why a finished run was not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// Exit code other than 0 (and failures are not being kept).
    Failed,
    /// The entry alone is larger than the cache's cap.
    TooBig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub hits: u64,
    pub misses: u64,
    /// Milliseconds hits did not have to run: the original duration, once per hit.
    pub saved_ms: u64,
}

impl Totals {
    pub(super) fn add_hit(&mut self, saved_ms: u64) {
        self.hits += 1;
        self.saved_ms += saved_ms;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStats {
    pub label: String,
    pub project: String,
    pub hits: u64,
    pub misses: u64,
    pub saved_ms: u64,
    /// Unix milliseconds of the last hit or miss.
    pub last_at: u64,
}

/// `stats.json`: the contract with the desktop (fixture `task_cache_stats.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub total: Totals,
    /// Project name (the folder its root resolves to) to its counters.
    pub projects: BTreeMap<String, Totals>,
    /// Key (64 hex) to its counters.
    pub keys: BTreeMap<String, KeyStats>,
}

impl Stats {
    pub(super) fn touch_key(
        &mut self,
        key: &str,
        label: &str,
        project: &str,
        now: u64,
    ) -> &mut KeyStats {
        let ks = self.keys.entry(key.to_string()).or_default();
        ks.label = label.to_string();
        ks.project = project.to_string();
        ks.last_at = now;
        // Bound the map: drop the least recently touched keys.
        if self.keys.len() > KEYS_KEEP {
            let mut by_age: Vec<(u64, String)> = self
                .keys
                .iter()
                .map(|(k, v)| (v.last_at, k.clone()))
                .collect();
            by_age.sort();
            for (_, k) in by_age.into_iter().take(self.keys.len() - KEYS_KEEP) {
                self.keys.remove(&k);
            }
        }
        self.keys.get_mut(key).expect("just inserted")
    }
}

/// One run seen by the cache, newest last in `recent.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    pub label: String,
    pub command: String,
    pub project: String,
    pub key: String,
    pub hit: bool,
    pub exit: i32,
    pub duration_ms: u64,
    /// Unix milliseconds.
    pub at: u64,
    pub log_tail: String,
}

/// What the MCP tool `last_result` returns: the last outcome of a task, action or command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastResult {
    /// The task/action label (or the command line) it was found by.
    pub name: String,
    pub command: String,
    pub project: String,
    pub key: String,
    /// True when the result came from the cache; false when the command actually ran.
    pub hit: bool,
    pub exit: i32,
    /// How long the command took when it ran for real.
    pub duration_ms: u64,
    /// Unix milliseconds of this outcome (the hit, or the run).
    pub at: u64,
    /// The last ~2 KiB the run printed (both streams).
    pub log_tail: String,
    /// Time this hit saved (`hit` only).
    pub saved_ms: Option<u64>,
}

impl Recent {
    pub(super) fn matches(&self, name: &str, project: Option<&str>) -> bool {
        (self.label == name || self.command == name) && project.is_none_or(|p| self.project == p)
    }

    pub(super) fn into_last(self, name: &str) -> LastResult {
        LastResult {
            name: name.to_string(),
            saved_ms: self.hit.then_some(self.duration_ms),
            command: self.command,
            project: self.project,
            key: self.key,
            hit: self.hit,
            exit: self.exit,
            duration_ms: self.duration_ms,
            at: self.at,
            log_tail: self.log_tail,
        }
    }
}
