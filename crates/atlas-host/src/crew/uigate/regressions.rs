//! `.atlas/ui-regressions.json` in the project: every UI bug a crewmate fixed stays a scenario
//! that the gate replays on every change. The crewmate commits the file with its fix, so the
//! registry travels with the code and every other crewmate and worktree replays the same set.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::scenario::Scenario;
use super::tree::Tree;

/// Project-relative path of the registry.
pub const FILE: &str = ".atlas/ui-regressions.json";

/// One scenario that once failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regression {
    #[serde(flatten)]
    pub scenario: Scenario,
    /// What went wrong, for the next person who wonders why it is here.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    #[serde(default)]
    regressions: Vec<Regression>,
}

/// The registry; a missing file is an empty one. A file that is not valid, or has an entry the
/// capture would refuse, is an error naming the file and what is wrong: replaying a half-read
/// registry would drop regressions silently.
pub fn load(tree: &Tree, root: &Path) -> Result<Vec<Regression>, String> {
    let Some(text) = tree.read_text(root, FILE)? else {
        return Ok(vec![]);
    };
    let doc: Doc = serde_json::from_str(&text).map_err(|e| {
        format!(
            "ui gate: {FILE} is not a valid regression registry ({e}); \
             expected {{\"regressions\": [{{\"url\": \"http://…\", \"note\": \"…\"}}]}}. Fix or delete the file"
        )
    })?;
    for (i, r) in doc.regressions.iter().enumerate() {
        r.scenario
            .spec(false)
            .map_err(|e| format!("ui gate: {FILE}, entry {}: {e}", i + 1))?;
    }
    Ok(doc.regressions)
}

/// Register `scenario`; the same scenario again only updates its note. Returns the registry.
pub fn add(
    tree: &Tree,
    root: &Path,
    scenario: Scenario,
    note: &str,
) -> Result<Vec<Regression>, String> {
    scenario.spec(false)?;
    let mut all = load(tree, root)?;
    let id = scenario.id()?;
    match all
        .iter_mut()
        .find(|r| r.scenario.id().ok().as_deref() == Some(id.as_str()))
    {
        Some(r) => r.note = note.to_string(),
        None => all.push(Regression {
            scenario,
            note: note.to_string(),
        }),
    }
    let body = serde_json::to_string_pretty(&Doc {
        regressions: all.clone(),
    })
    .map_err(|e| e.to_string())?;
    tree.write_text(root, FILE, &(body + "\n"))?;
    Ok(all)
}
