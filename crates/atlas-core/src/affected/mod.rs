//! What a change affects, answered from data without starting a process: the backend of the MCP
//! tools `atlas_affected` and `atlas_estimate`.
//!
//! * [`changed_files`] is the change: the worktree's diff against its merge base with the base
//!   branch, plus staged, unstaged and untracked files (what `scripts/check-changed.ts` uses).
//! * [`resolve`] turns changed files into an [`Impact`] from the first source that exists: a
//!   plugin command ([`from_command`]), the project's declared rules ([`from_rules`]), else the
//!   atlas.yaml actions whose `cache.inputs` match ([`from_action_inputs`]).
//! * [`estimate`](estimate::estimate) says how long a task takes (median and p90 of the
//!   recorded runs) and whether it would be a cache hit right now (the key is computed, never
//!   run).

mod changed;
pub mod estimate;
mod glob;
mod plugin;
mod rules;

#[cfg(test)]
mod estimate_tests;
#[cfg(test)]
mod tests;

pub use changed::{RunGit, changed_files, changed_files_with};
pub use plugin::from_command;
pub use rules::{
    ActionInputs, Declared, Rule, Source, check_globs, from_action_inputs, from_rules, resolve,
};

use serde::{Deserialize, Serialize};

/// What a change affects, each list deduplicated in first-seen order. Every key is optional in a
/// plugin's JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Impact {
    /// atlas.yaml action labels (or project task names) worth running.
    pub tasks: Vec<String>,
    /// Test commands or names.
    pub tests: Vec<String>,
    /// Routes (pages, endpoints) whose output can change.
    pub routes: Vec<String>,
    /// Builds (packages, sites, artifacts) to rebuild.
    pub builds: Vec<String>,
}
