//! How long a task takes and whether it would be a cache hit right now, from recorded data:
//! the task cache's real runs and the heavy queue's run history. Nothing here runs the task.

use serde::Serialize;

use crate::heavy::Dir;
use crate::taskcache::{Cache, Spec};

/// A sample of run durations, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Durations(Vec<u64>);

impl Durations {
    pub fn from_ms(mut ms: Vec<u64>) -> Self {
        ms.sort_unstable();
        Self(ms)
    }

    pub fn runs(&self) -> usize {
        self.0.len()
    }

    /// The middle value; the mean of the two middle ones for an even count.
    pub fn median_ms(&self) -> Option<u64> {
        let n = self.0.len();
        match n {
            0 => None,
            _ if n % 2 == 1 => Some(self.0[n / 2]),
            _ => Some((self.0[n / 2 - 1] + self.0[n / 2]) / 2),
        }
    }

    /// Nearest-rank 90th percentile: the smallest value with at least 90% of the sample at or
    /// below it.
    pub fn p90_ms(&self) -> Option<u64> {
        let n = self.0.len();
        (n > 0).then(|| self.0[(n * 9).div_ceil(10) - 1])
    }
}

/// The answer of `atlas_estimate` for one task (the queue's load is added by the caller, it is
/// about the machine, not the task).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Estimate {
    /// Runs the numbers come from (0: no history, no guess is made).
    pub runs: usize,
    pub median_ms: Option<u64>,
    pub p90_ms: Option<u64>,
    /// Where the samples came from: `task cache`, else `heavy history`.
    pub sources: Vec<&'static str>,
    /// An identical run (same argv, same input bytes as on disk now, same env and tools) is
    /// stored: `atlas run --cache` would return it without running anything.
    pub would_hit: bool,
    /// What that hit would save (how long the stored run took).
    pub saved_ms: Option<u64>,
    /// The key the run has now (64 hex), `None` when it could not be computed.
    pub key: Option<String>,
    /// Why the key could not be computed (a bad glob, a tool that has no `--version`).
    pub key_error: Option<String>,
}

/// Estimate the task called `label` / running `command`, of `project`. `spec` says what its
/// result depends on (its key is computed, the command is not run); `None` for a task that is not
/// cached, which can only have a duration. Durations come from runs that really happened: the
/// cache's recorded runs, else the heavy queue's history of the same label.
pub fn estimate(
    label: &str,
    command: &str,
    project: Option<&str>,
    spec: Option<&Spec>,
    cache: &Cache,
    heavy: &Dir,
) -> Estimate {
    let mut ms = cache.durations_ms(label, project);
    if ms.is_empty() && label != command {
        ms = cache.durations_ms(command, project);
    }
    let mut sources = Vec::new();
    if !ms.is_empty() {
        sources.push("task cache");
    } else {
        // The queue's history is per label, in whole seconds, and not per project. It is only the
        // fallback: a cached `--heavy` action is recorded by BOTH stores for the same real run.
        let secs = heavy
            .runs()
            .into_iter()
            .find(|r| r.label == label)
            .map(|r| r.secs)
            .unwrap_or_default();
        if !secs.is_empty() {
            sources.push("heavy history");
            ms = secs.into_iter().map(|s| s * 1000).collect();
        }
    }
    let d = Durations::from_ms(ms);

    let (key, key_error) = match spec.map(Spec::key) {
        None => (None, None),
        Some(Ok(k)) => (Some(k), None),
        Some(Err(e)) => (None, Some(e.to_string())),
    };
    let stored = key.as_ref().and_then(|k| cache.peek(k));
    Estimate {
        runs: d.runs(),
        median_ms: d.median_ms(),
        p90_ms: d.p90_ms(),
        sources,
        would_hit: stored.is_some(),
        saved_ms: stored.map(|e| e.duration_ms),
        key: key.map(|k| k.to_string()),
        key_error,
    }
}
