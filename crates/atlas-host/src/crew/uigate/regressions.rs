//! `.atlas/ui-regressions.json` in the project: every UI bug a crewmate fixed stays a scenario
//! that the gate replays on every change. The crewmate commits the file with its fix, so the
//! registry travels with the code and every other crewmate and worktree replays the same set.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::scenario::Scenario;

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

fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// The registry; a missing file is an empty one. A file that is not valid, or has an entry the
/// capture would refuse, is an error naming the file and what is wrong: replaying a half-read
/// registry would drop regressions silently.
pub fn load(root: &Path) -> Result<Vec<Regression>, String> {
    let p = path(root);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("ui gate: cannot read {}: {e}", p.display())),
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
pub fn add(root: &Path, scenario: Scenario, note: &str) -> Result<Vec<Regression>, String> {
    scenario.spec(false)?;
    let mut all = load(root)?;
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
    let p = path(root);
    std::fs::create_dir_all(p.parent().unwrap_or(root)).map_err(|e| {
        format!(
            "ui gate: cannot create {}: {e}",
            root.join(".atlas").display()
        )
    })?;
    let body = serde_json::to_string_pretty(&Doc {
        regressions: all.clone(),
    })
    .map_err(|e| e.to_string())?;
    // Atomic: a crash mid-write must not leave a file `load` refuses.
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, body + "\n")
        .and_then(|()| std::fs::rename(&tmp, &p))
        .map_err(|e| format!("ui gate: cannot write {}: {e}", p.display()))?;
    Ok(all)
}
