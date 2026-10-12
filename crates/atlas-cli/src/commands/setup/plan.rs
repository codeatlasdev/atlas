//! Harness-agnostic plan model: what a setup run would do to each artifact.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use atlas_skills::embedded;

/// What a plan item will do (or did) to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    UpToDate,
    /// A foreign artifact occupies the slot; refuse unless `--force`.
    Conflict,
    Remove,
    /// Uninstall: nothing atlas-owned to remove.
    Absent,
    /// A prerequisite is missing (e.g. `omp` not on PATH); the item is reported but neither
    /// changes anything nor blocks the rest of the plan.
    Skipped,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Create => "create",
            Action::Update => "update",
            Action::UpToDate => "up to date",
            Action::Conflict => "CONFLICT",
            Action::Remove => "remove",
            Action::Absent => "not installed",
            Action::Skipped => "skipped",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Action::Create => "\x1b[32m",
            Action::Update => "\x1b[33m",
            Action::UpToDate | Action::Absent | Action::Skipped => "\x1b[2m",
            Action::Conflict => "\x1b[31m",
            Action::Remove => "\x1b[35m",
        }
    }

    /// Whether applying this item changes anything on disk.
    pub fn is_change(self) -> bool {
        matches!(self, Action::Create | Action::Update | Action::Remove)
    }
}

/// The concrete write an item performs on apply. Computed at plan time so the
/// dry-run output and `--apply` can never disagree.
#[derive(Debug, Clone)]
pub enum Op {
    /// Nothing to do.
    None,
    /// Atomically write `content` to the item's path.
    WriteFile(String),
    /// Delete the item's path, then remove parent directories that became empty,
    /// walking up but never removing `boundary` itself (`None` = leave parents alone).
    RemoveFile { boundary: Option<PathBuf> },
    /// Install the embedded skills (names empty = all).
    InstallSkills,
    /// Remove the embedded skills recorded in the manifest.
    UninstallSkills,
    /// Delete the item's path recursively (atlas-owned directories only).
    RemoveDir,
    /// Run an external program (inherits stdio); a non-zero exit fails the apply.
    Exec { program: String, args: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: &'static str,
    pub path: PathBuf,
    pub action: Action,
    /// Short human diff summary, e.g. `+ mcpServers.atlas`.
    pub detail: String,
    pub op: Op,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    pub warnings: Vec<String>,
}

impl Plan {
    pub fn has_conflict(&self) -> bool {
        self.items.iter().any(|i| i.action == Action::Conflict)
    }

    pub fn has_changes(&self) -> bool {
        self.items.iter().any(|i| i.action.is_change())
    }

    /// Render the plan. `applied` switches tenses (`would create` → `created`).
    pub fn render(&self, applied: bool) -> String {
        let mut out = String::new();
        for w in &self.warnings {
            out.push_str(&format!("  \x1b[33m!\x1b[0m {w}\n"));
        }
        if !self.warnings.is_empty() {
            out.push('\n');
        }
        for i in &self.items {
            let verb = if applied {
                match i.action {
                    Action::Create => "created",
                    Action::Update => "updated",
                    Action::Remove => "removed",
                    a => a.label(),
                }
            } else {
                i.action.label()
            };
            out.push_str(&format!(
                "  {}{:<13}\x1b[0m \x1b[1m{}\x1b[0m  {}\n",
                i.action.color(),
                verb,
                i.label,
                i.path.display()
            ));
            if !i.detail.is_empty() {
                for line in i.detail.lines() {
                    out.push_str(&format!("                {line}\n"));
                }
            }
        }
        out
    }
}

/// Where a harness keeps the bundled skills and the manifest that records ownership.
#[derive(Debug, Clone)]
pub struct SkillsTarget {
    pub dir: PathBuf,
    pub manifest: PathBuf,
}

/// Execute every changing item of a plan.
pub fn apply(plan: &Plan, skills: &SkillsTarget) -> Result<()> {
    for item in &plan.items {
        if !item.action.is_change() {
            continue;
        }
        match &item.op {
            Op::None => {}
            Op::WriteFile(content) => embedded::write_atomic(&item.path, content)
                .with_context(|| format!("writing {}", item.path.display()))?,
            Op::RemoveFile { boundary } => {
                std::fs::remove_file(&item.path)
                    .with_context(|| format!("removing {}", item.path.display()))?;
                if let Some(b) = boundary {
                    prune_empty_parents(&item.path, b);
                }
            }
            Op::InstallSkills => {
                embedded::install(&skills.dir, &skills.manifest, &[])?;
            }
            Op::UninstallSkills => {
                embedded::uninstall(&skills.dir, &skills.manifest)?;
            }
            Op::RemoveDir => std::fs::remove_dir_all(&item.path)
                .with_context(|| format!("removing {}", item.path.display()))?,
            Op::Exec { program, args } => {
                let status = std::process::Command::new(program)
                    .args(args)
                    .status()
                    .with_context(|| format!("running {program}"))?;
                if !status.success() {
                    return Err(anyhow!("{program} {args:?} exited {status}"));
                }
            }
        }
    }
    Ok(())
}

/// Remove directories left empty by deleting `removed`, from its parent up to (not
/// including) `boundary`. Stops at the first non-empty directory.
fn prune_empty_parents(removed: &Path, boundary: &Path) {
    let mut dir = removed.parent();
    while let Some(d) = dir {
        if d == boundary || !d.starts_with(boundary) || std::fs::remove_dir(d).is_err() {
            return;
        }
        dir = d.parent();
    }
}

/// Outcome of merging the atlas MCP entry into a config file (JSON or TOML).
#[derive(Debug, PartialEq)]
pub enum MergeOutcome {
    Create(String),
    Update(String),
    UpToDate,
    /// Foreign `atlas` entry present; carries a description of it.
    Conflict(String),
}

/// Outcome of removing the atlas MCP entry from a config file.
#[derive(Debug, PartialEq)]
pub enum RemoveOutcome {
    /// No `atlas` entry (or no file).
    Absent,
    /// Entry exists but is not atlas-owned.
    Foreign,
    /// Owned entry removed; new document text, or `None` when the file only
    /// ever held atlas-created content and should be deleted.
    Removed(Option<String>),
}
