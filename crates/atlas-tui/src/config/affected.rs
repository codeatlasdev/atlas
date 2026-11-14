//! `affected:` — the project's declared graph of what a change affects, read by the MCP tool
//! `atlas_affected`.
//!
//! ```yaml
//! affected:
//!   command: bun scripts/affected.ts --json   # changed files on stdin, {tasks,tests,routes,builds} on stdout
//!   rules:                                    # or declare it here: files matching → what they affect
//!     - match: ['crates/atlas-core/**']
//!       tasks: [Run checks]                   # action labels
//!       tests: ['cargo test -p atlas-core']
//!       routes: [/docs]
//!       builds: [site]
//! ```
//!
//! `command` wins over `rules`; without either, `atlas_affected` falls back to the actions whose
//! `cache.inputs` match the changed files.

use schemars::JsonSchema;
use serde::Deserialize;

/// The `affected:` block.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
#[serde(deny_unknown_fields)]
pub struct AffectedConfig {
    /// Shell command run in the project root. It receives the changed files on stdin (one per
    /// line, relative to the project root) and prints JSON `{tasks, tests, routes, builds}` (every
    /// key optional, each a list of strings). Wins over `rules`.
    #[serde(default)]
    pub command: Option<String>,
    /// Declared graph: when a changed file matches `match`, the rule's tasks, tests, routes and
    /// builds are affected. Used when there is no `command`.
    #[serde(default)]
    pub rules: Vec<AffectedRule>,
}

/// One line of the declared graph.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
#[serde(deny_unknown_fields)]
pub struct AffectedRule {
    /// Globs relative to the project root (`**` crosses folders). Any changed file matching one
    /// selects the rule.
    #[serde(rename = "match", default)]
    pub matches: Vec<String>,
    /// Action labels to run (see `actions:`).
    #[serde(default)]
    pub tasks: Vec<String>,
    /// Test commands or names.
    #[serde(default)]
    pub tests: Vec<String>,
    /// Routes (pages, endpoints) whose output can change.
    #[serde(default)]
    pub routes: Vec<String>,
    /// Builds (packages, sites, artifacts) to rebuild.
    #[serde(default)]
    pub builds: Vec<String>,
}

impl AffectedConfig {
    /// The core view of this block.
    pub fn declared(&self) -> atlas_core::affected::Declared {
        atlas_core::affected::Declared {
            command: self.command.clone(),
            rules: self
                .rules
                .iter()
                .map(|r| atlas_core::affected::Rule {
                    matches: r.matches.clone(),
                    tasks: r.tasks.clone(),
                    tests: r.tests.clone(),
                    routes: r.routes.clone(),
                    builds: r.builds.clone(),
                })
                .collect(),
        }
    }
}
