//! Plan-item builders shared by every harness: MCP entry, bundled skills, command resolution.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atlas_skills::{SkillState, SkillsManifest, embedded};

use super::Env;
use super::plan::{Action, Item, MergeOutcome, Op, Plan, RemoveOutcome, SkillsTarget};

pub fn read_opt(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Resolve the atlas command to put in agent configs: bare `atlas` when it resolves on
/// PATH, otherwise the absolute path of the running binary (with a warning).
pub fn resolve_command(env: &Env, plan_warnings: &mut Vec<String>) -> Result<String> {
    if env.atlas_on_path {
        return Ok("atlas".into());
    }
    let exe = std::env::current_exe().context("cannot locate the atlas binary")?;
    let exe = exe.to_string_lossy().to_string();
    if exe.is_empty() {
        bail!("cannot locate the atlas binary");
    }
    plan_warnings.push(format!(
        "`atlas` is not on PATH — using the absolute path {exe}. Add ~/.atlas/bin to PATH, or re-run after `atlas install`."
    ));
    Ok(exe)
}

/// `dir` where the harness-agnostic skills live (`~/.agents/skills`, read by OMP and Codex).
pub fn shared_skills_target(env: &Env) -> SkillsTarget {
    SkillsTarget {
        dir: env.home.join(".agents").join("skills"),
        manifest: env.home.join(".atlas").join(".atlas-skills.json"),
    }
}

/// MCP-entry plan item. `key` is what the entry is called in the file, for display.
pub fn mcp_install_item(
    path: &Path,
    merged: MergeOutcome,
    key: &str,
    command: &str,
    boundary_note: &str,
) -> Item {
    let summary = format!("{key} = {{stdio: {command} mcp}}");
    let item = |action, detail: String, op| Item {
        label: "MCP server",
        path: path.to_path_buf(),
        action,
        detail,
        op,
    };
    match merged {
        MergeOutcome::Create(t) => item(Action::Create, format!("+ {summary}"), Op::WriteFile(t)),
        MergeOutcome::Update(t) => item(
            Action::Update,
            format!("~ {summary}{boundary_note}"),
            Op::WriteFile(t),
        ),
        MergeOutcome::UpToDate => item(Action::UpToDate, String::new(), Op::None),
        MergeOutcome::Conflict(cur) => item(
            Action::Conflict,
            format!(
                "an `atlas` entry not created by atlas exists: {cur}\nre-run with --force to replace it"
            ),
            Op::None,
        ),
    }
}

/// MCP-entry uninstall item. `delete_boundary` is passed to `Op::RemoveFile` when the
/// file held nothing but the atlas entry.
pub fn mcp_uninstall_item(
    path: &Path,
    removed: RemoveOutcome,
    key: &str,
    delete_boundary: Option<PathBuf>,
) -> Item {
    let item = |action, detail: String, op| Item {
        label: "MCP server",
        path: path.to_path_buf(),
        action,
        detail,
        op,
    };
    match removed {
        RemoveOutcome::Absent => item(Action::Absent, String::new(), Op::None),
        RemoveOutcome::Foreign => item(
            Action::Conflict,
            "`atlas` entry was not created by atlas — left alone (use --force to remove)".into(),
            Op::None,
        ),
        RemoveOutcome::Removed(Some(t)) => item(
            Action::Remove,
            format!("- {key} (other servers kept)"),
            Op::WriteFile(t),
        ),
        RemoveOutcome::Removed(None) => item(
            Action::Remove,
            format!("- {key} (file held nothing else → deleted)"),
            Op::RemoveFile {
                boundary: delete_boundary,
            },
        ),
    }
}

/// Hooks-file install item (Claude Code `settings.json`, Codex `hooks.json`).
pub fn hooks_install_item(
    path: &Path,
    merged: MergeOutcome,
    create_detail: &str,
    update_detail: &str,
) -> Item {
    let item = |action, detail: &str, op| Item {
        label: "Hooks",
        path: path.to_path_buf(),
        action,
        detail: detail.to_string(),
        op,
    };
    match merged {
        MergeOutcome::Create(t) => item(Action::Create, create_detail, Op::WriteFile(t)),
        MergeOutcome::Update(t) => item(Action::Update, update_detail, Op::WriteFile(t)),
        MergeOutcome::UpToDate => item(Action::UpToDate, "", Op::None),
        // Hooks have no foreign-slot conflicts (ownership is the exact command).
        MergeOutcome::Conflict(_) => unreachable!("hooks_json::merge never reports a conflict"),
    }
}

/// Hooks-file uninstall item. `others_kept` names what survives in the rewritten file.
pub fn hooks_uninstall_item(
    path: &Path,
    removed: RemoveOutcome,
    others_kept: &str,
    boundary: &Path,
) -> Item {
    let item = |action, detail: String, op| Item {
        label: "Hooks",
        path: path.to_path_buf(),
        action,
        detail,
        op,
    };
    match removed {
        RemoveOutcome::Absent | RemoveOutcome::Foreign => {
            item(Action::Absent, String::new(), Op::None)
        }
        RemoveOutcome::Removed(Some(t)) => item(
            Action::Remove,
            format!("- atlas hooks ({others_kept} kept)"),
            Op::WriteFile(t),
        ),
        RemoveOutcome::Removed(None) => item(
            Action::Remove,
            "- atlas hooks (file held nothing else → deleted)".into(),
            Op::RemoveFile {
                boundary: Some(boundary.to_path_buf()),
            },
        ),
    }
}

/// Skills install item for `target`; `label` distinguishes "Skills" from "Skills (shared)".
pub fn skills_install_item(label: &'static str, target: &SkillsTarget) -> Result<Item> {
    let skills = embedded::all();
    let missing = skills
        .iter()
        .filter(|s| s.state_in(&target.dir) == SkillState::Missing)
        .count();
    let outdated = skills
        .iter()
        .filter(|s| s.state_in(&target.dir) == SkillState::Outdated)
        .count();
    let tracked = SkillsManifest::load_from(&target.manifest)?;
    let all_tracked = skills.iter().all(|s| tracked.get(&s.name).is_some());
    if missing == 0 && outdated == 0 && all_tracked {
        return Ok(Item {
            label,
            path: target.dir.clone(),
            action: Action::UpToDate,
            detail: String::new(),
            op: Op::None,
        });
    }
    let names = skills
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Item {
        label,
        path: target.dir.clone(),
        action: if missing == skills.len() {
            Action::Create
        } else {
            Action::Update
        },
        detail: format!(
            "{names} (v{}: {missing} new, {outdated} changed)",
            env!("CARGO_PKG_VERSION")
        ),
        op: Op::InstallSkills,
    })
}

/// Skills uninstall item: only skills recorded in the manifest are removed.
/// `keep_reason` (e.g. another harness still uses the shared dir) turns it into a no-op note.
pub fn skills_uninstall_item(
    label: &'static str,
    target: &SkillsTarget,
    keep_reason: Option<&str>,
) -> Result<Item> {
    let tracked = SkillsManifest::load_from(&target.manifest)?;
    let owned: Vec<String> = embedded::all()
        .into_iter()
        .filter(|s| tracked.get(&s.name).is_some())
        .map(|s| s.name)
        .collect();
    let (action, detail, op) = match (owned.is_empty(), keep_reason) {
        (true, _) => (Action::Absent, String::new(), Op::None),
        (false, Some(why)) => (Action::Absent, format!("kept: {why}"), Op::None),
        (false, None) => (Action::Remove, owned.join(", "), Op::UninstallSkills),
    };
    Ok(Item {
        label,
        path: target.dir.clone(),
        action,
        detail,
        op,
    })
}

/// A `.json`/`.toml` config with an atlas-owned MCP entry means the harness is set up.
/// Used to decide whether shared skills may be removed on uninstall.
pub fn file_mentions_owned_atlas_mcp_json(path: &Path) -> bool {
    use super::json_merge;
    read_opt(path)
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("mcpServers")?.get(json_merge::SERVER_NAME).cloned())
        .is_some_and(|e| json_merge::is_owned(&e))
}

/// Plan header warning for scopes where skills cannot follow the project.
pub fn user_scope_skills_warning(plan: &mut Plan, dir_display: &str, harness: &str) {
    plan.warnings.push(format!(
        "skills are always installed at user scope ({dir_display}) — {harness} does not read project-local skill copies from atlas"
    ));
}
