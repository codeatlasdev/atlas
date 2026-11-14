//! What the gate learned about one task: scenario -> pass or fail and the captures it made.
//! Persisted in the task's captures dir (`ui-gate.json`), so `rerun_failed` after a restart still
//! knows what failed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::scenario::{Scenario, Why};

pub const FILE: &str = "ui-gate.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
}

/// How a scenario's verdict was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The capture ran now.
    Captured,
    /// The task cache already had a pass for the same scenario over the same inputs: nothing ran.
    Cached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub scenario: Scenario,
    pub why: Why,
    pub verdict: Verdict,
    pub source: Source,
    /// File names under the task's captures dir, one per cell.
    #[serde(default)]
    pub captures: Vec<String>,
    /// Why it failed: the capture error and every finding.
    #[serde(default)]
    pub problems: Vec<String>,
}

/// The task's results, by scenario id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskResults {
    pub scenarios: BTreeMap<String, Outcome>,
}

impl TaskResults {
    pub fn path(captures_dir: &Path) -> PathBuf {
        captures_dir.join(FILE)
    }

    /// Missing file: nothing run yet. A file that does not parse is an error, not an empty
    /// record: `rerun_failed` on an empty record would claim there is nothing to rerun.
    pub fn load(captures_dir: &Path) -> Result<Self, String> {
        let p = Self::path(captures_dir);
        match std::fs::read_to_string(&p) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| {
                format!(
                    "ui gate: {} is damaged ({e}); run `ui_gate run` again",
                    p.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("ui gate: cannot read {}: {e}", p.display())),
        }
    }

    pub fn save(&self, captures_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(captures_dir)
            .map_err(|e| format!("ui gate: cannot create {}: {e}", captures_dir.display()))?;
        let p = Self::path(captures_dir);
        let tmp = p.with_extension("json.tmp");
        let body = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body)
            .and_then(|()| std::fs::rename(&tmp, &p))
            .map_err(|e| format!("ui gate: cannot write {}: {e}", p.display()))
    }

    /// Ids of the scenarios whose last verdict is a failure, in id order.
    pub fn failed(&self) -> Vec<String> {
        self.scenarios
            .iter()
            .filter(|(_, o)| o.verdict == Verdict::Fail)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The gate passes when something ran and nothing failed.
    pub fn passed(&self) -> bool {
        !self.scenarios.is_empty() && self.failed().is_empty()
    }
}
