//! The sources of an [`Impact`] and the order they are tried in.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Impact;
use super::glob::Globs;
use super::plugin::from_command;

/// One line of the declared graph: files matching `matches` affect these tasks, tests, routes
/// and builds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Globs relative to the project root.
    pub matches: Vec<String>,
    pub tasks: Vec<String>,
    pub tests: Vec<String>,
    pub routes: Vec<String>,
    pub builds: Vec<String>,
}

/// What atlas.yaml's `affected:` declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    /// A shell command: changed files on stdin (one per line), an [`Impact`] as JSON on stdout.
    pub command: Option<String>,
    pub rules: Vec<Rule>,
}

/// An atlas.yaml action with a `cache:` block: the globs its result depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInputs {
    pub label: String,
    /// Empty: every file git would not ignore (any change affects it).
    pub inputs: Vec<String>,
}

/// Where an [`Impact`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The project's plugin command.
    Command,
    /// The `rules:` declared in atlas.yaml.
    Rules,
    /// The fallback: actions whose `cache.inputs` match.
    CacheInputs,
}

/// Union in first-seen order, without duplicates.
fn push_all(into: &mut Vec<String>, from: &[String]) {
    for s in from {
        if !into.contains(s) {
            into.push(s.clone());
        }
    }
}

/// `Err` names the first glob of `globs` that does not parse (for config validation).
pub fn check_globs(globs: &[String]) -> Result<(), String> {
    Globs::new(globs).map(drop)
}

/// The declared graph applied to `files`.
pub fn from_rules(rules: &[Rule], files: &[String]) -> Result<Impact, String> {
    let mut out = Impact::default();
    for r in rules {
        let globs = Globs::new(&r.matches)?;
        if files.iter().any(|f| globs.is_match(f)) {
            push_all(&mut out.tasks, &r.tasks);
            push_all(&mut out.tests, &r.tests);
            push_all(&mut out.routes, &r.routes);
            push_all(&mut out.builds, &r.builds);
        }
    }
    Ok(out)
}

/// The fallback: the actions a change can invalidate (their `cache.inputs` match a changed file).
pub fn from_action_inputs(actions: &[ActionInputs], files: &[String]) -> Result<Impact, String> {
    let mut out = Impact::default();
    for a in actions {
        let hit = if a.inputs.is_empty() {
            !files.is_empty()
        } else {
            let globs = Globs::new(&a.inputs)?;
            files.iter().any(|f| globs.is_match(f))
        };
        if hit {
            push_all(&mut out.tasks, std::slice::from_ref(&a.label));
        }
    }
    Ok(out)
}

/// Impact of `files`, from the first source that exists: the plugin command, then the declared
/// rules, then the actions' cache inputs. A declared source that fails is an error, never a
/// silent fallback (an answer from a different graph than the project declared would mislead).
pub fn resolve(
    declared: Option<&Declared>,
    actions: &[ActionInputs],
    root: &Path,
    files: &[String],
) -> Result<(Source, Impact), String> {
    if let Some(d) = declared {
        if let Some(cmd) = d.command.as_deref().filter(|c| !c.trim().is_empty()) {
            return from_command(cmd, root, files).map(|i| (Source::Command, i));
        }
        if !d.rules.is_empty() {
            return from_rules(&d.rules, files).map(|i| (Source::Rules, i));
        }
    }
    from_action_inputs(actions, files).map(|i| (Source::CacheInputs, i))
}
