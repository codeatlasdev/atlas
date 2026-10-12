//! Claude Code harness: MCP entry + skills + hooks (`atlas hook claude-code …`).
//!
//! Primary docs (checked 2026-10, Claude Code 2.1.292):
//! * MCP scopes — user: `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when that variable is
//!   set, verified with `claude mcp add --scope user`), project: `.mcp.json` at the project root:
//!   <https://code.claude.com/docs/en/mcp#mcp-installation-scopes>
//! * Skills — `~/.claude/skills/<name>/SKILL.md` (Claude Code does NOT read `~/.agents/skills`):
//!   <https://code.claude.com/docs/en/skills#where-skills-live>
//! * Hooks — `~/.claude/settings.json` / `.claude/settings.json`, `SessionStart` context via
//!   `hookSpecificOutput.additionalContext`, `PreToolUse` deny via `permissionDecision`:
//!   <https://code.claude.com/docs/en/hooks>
//! * `CLAUDE_CONFIG_DIR` relocates the config dir: <https://code.claude.com/docs/en/env-vars>

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::hooks_json;
use super::json_merge;
use super::plan::{Plan, SkillsTarget};
use super::shared::{
    hooks_install_item, hooks_uninstall_item, mcp_install_item, mcp_uninstall_item, read_opt,
    skills_install_item, skills_uninstall_item, user_scope_skills_warning,
};
use super::{Env, Scope};
use crate::commands::hook::Harness;

pub struct Paths {
    /// `~/.claude.json` (user) or `<project>/.mcp.json`.
    pub mcp_json: PathBuf,
    /// `<config dir>/settings.json` (user) or `<project>/.claude/settings.json`.
    pub settings: PathBuf,
    pub skills: SkillsTarget,
    /// Parent directories emptied by uninstall are pruned up to (not including) this.
    pub boundary: PathBuf,
    /// `.mcp.json` sits in the project root itself: nothing to prune above it.
    pub mcp_boundary: Option<PathBuf>,
}

impl Paths {
    pub fn resolve(env: &Env, scope: Scope, project_root: Option<&Path>) -> Result<Self> {
        // Skills are user-scope for both scopes (like OMP): the manifest lives next to them,
        // per config dir, so two `CLAUDE_CONFIG_DIR`s never share ownership records.
        let skills = SkillsTarget {
            dir: env.claude_dir.join("skills"),
            manifest: env.claude_dir.join(".atlas-skills.json"),
        };
        Ok(match scope {
            Scope::User => Self {
                mcp_json: env.claude_json.clone(),
                settings: env.claude_dir.join("settings.json"),
                skills,
                boundary: env.claude_dir.clone(),
                mcp_boundary: None,
            },
            Scope::Project => {
                let root = project_root.context("project scope needs an atlas.yaml ancestor")?;
                Self {
                    mcp_json: root.join(".mcp.json"),
                    settings: root.join(".claude").join("settings.json"),
                    skills,
                    boundary: root.to_path_buf(),
                    mcp_boundary: None,
                }
            }
        })
    }
}

pub fn plan_install(paths: &Paths, scope: Scope, force: bool, command: &str) -> Result<Plan> {
    let mut plan = Plan::default();

    let existing = read_opt(&paths.mcp_json)?;
    let merged = json_merge::merge(json_merge::CLAUDE, existing.as_deref(), command, force)
        .with_context(|| {
            format!(
                "{} is not a valid Claude Code MCP config",
                paths.mcp_json.display()
            )
        })?;
    plan.items.push(mcp_install_item(
        &paths.mcp_json,
        merged,
        "mcpServers.atlas",
        command,
        " (other servers and settings kept)",
    ));

    plan.items
        .push(skills_install_item("Skills", &paths.skills)?);
    if scope == Scope::Project {
        user_scope_skills_warning(
            &mut plan,
            &paths.skills.dir.display().to_string(),
            "Claude Code",
        );
    }

    let existing = read_opt(&paths.settings)?;
    let merged = hooks_json::merge(existing.as_deref(), Harness::ClaudeCode, command)
        .with_context(|| {
            format!(
                "{} is not valid Claude Code settings",
                paths.settings.display()
            )
        })?;
    plan.items.push(hooks_install_item(
        &paths.settings,
        merged,
        "+ hooks.SessionStart (status injection), hooks.PreToolUse[Bash] (`atlas dev` guard)",
        "~ atlas hooks in hooks.SessionStart / hooks.PreToolUse (your other hooks and settings kept)",
    ));
    Ok(plan)
}

pub fn plan_uninstall(paths: &Paths, force: bool) -> Result<Plan> {
    let mut plan = Plan::default();

    let existing = read_opt(&paths.mcp_json)?;
    let removed = json_merge::remove(existing.as_deref(), force).with_context(|| {
        format!(
            "{} is not a valid Claude Code MCP config",
            paths.mcp_json.display()
        )
    })?;
    plan.items.push(mcp_uninstall_item(
        &paths.mcp_json,
        removed,
        "mcpServers.atlas",
        paths.mcp_boundary.clone(),
    ));

    plan.items
        .push(skills_uninstall_item("Skills", &paths.skills, None)?);

    let existing = read_opt(&paths.settings)?;
    let removed =
        hooks_json::remove(existing.as_deref(), Harness::ClaudeCode).with_context(|| {
            format!(
                "{} is not valid Claude Code settings",
                paths.settings.display()
            )
        })?;
    plan.items.push(hooks_uninstall_item(
        &paths.settings,
        removed,
        "your other hooks and settings",
        &paths.boundary,
    ));
    Ok(plan)
}

/// `atlas update` refresh gate: this scope has atlas-owned hooks.
pub fn hooks_installed(paths: &Paths) -> bool {
    hooks_json::has_owned(
        read_opt(&paths.settings).ok().flatten().as_deref(),
        Harness::ClaudeCode,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::plan::{Action, apply};
    use serde_json::Value;

    fn read_json(p: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn user_paths_follow_claude_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let p = Paths::resolve(&env, Scope::User, None).unwrap();
        assert_eq!(p.mcp_json, env.claude_json);
        assert_eq!(p.settings, env.claude_dir.join("settings.json"));
        assert_eq!(p.skills.dir, env.claude_dir.join("skills"));
        assert_eq!(p.skills.manifest, env.claude_dir.join(".atlas-skills.json"));
    }

    #[test]
    fn project_paths_and_requirement() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        assert!(Paths::resolve(&env, Scope::Project, None).is_err());
        let root = tmp.path().join("proj");
        let p = Paths::resolve(&env, Scope::Project, Some(&root)).unwrap();
        assert_eq!(p.mcp_json, root.join(".mcp.json"));
        assert_eq!(p.settings, root.join(".claude/settings.json"));
        assert_eq!(
            p.skills.dir,
            env.claude_dir.join("skills"),
            "skills stay user scope"
        );
    }

    #[test]
    fn merge_idempotent_uninstall_preserves_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.claude_dir).unwrap();
        // A realistic ~/.claude.json: lots of unrelated state, ordered, with another server.
        std::fs::write(
            &paths.mcp_json,
            r#"{"numStartups": 12, "mcpServers": {"other": {"type":"stdio","command":"x","args":[],"env":{}}}, "projects": {"/p": {"allowedTools": []}}}"#,
        )
        .unwrap();
        std::fs::write(
            &paths.settings,
            r#"{"model":"opus","hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"echo mine"}]}]}}"#,
        )
        .unwrap();
        let mine = paths.skills.dir.join("my-own-skill/SKILL.md");
        std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
        std::fs::write(&mine, "---\nname: my-own-skill\n---\n").unwrap();

        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_changes() && !plan.has_conflict());
        apply(&plan, &paths.skills).unwrap();

        let claude_json = read_json(&paths.mcp_json);
        assert_eq!(claude_json["numStartups"], 12);
        assert_eq!(claude_json["mcpServers"]["atlas"]["args"][0], "mcp");
        assert_eq!(claude_json["mcpServers"]["other"]["command"], "x");
        let keys: Vec<_> = claude_json.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["numStartups", "mcpServers", "projects"]);
        let settings = read_json(&paths.settings);
        assert_eq!(settings["model"], "opus");
        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "echo mine"
        );
        assert_eq!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "atlas hook claude-code session-start 2>/dev/null || true"
        );
        assert!(paths.skills.dir.join("atlas/SKILL.md").is_file());
        assert!(
            !env.home.join(".agents").exists(),
            "Claude Code does not use the shared ~/.agents/skills"
        );

        let again = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(
            again.items.iter().all(|i| i.action == Action::UpToDate),
            "{:?}",
            again.items
        );

        let un = plan_uninstall(&paths, false).unwrap();
        apply(&un, &paths.skills).unwrap();
        let claude_json = read_json(&paths.mcp_json);
        assert!(claude_json["mcpServers"].get("atlas").is_none());
        assert_eq!(claude_json["mcpServers"]["other"]["command"], "x");
        let settings = read_json(&paths.settings);
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert!(settings["hooks"].get("SessionStart").is_none());
        assert!(!paths.skills.dir.join("atlas").exists());
        assert!(mine.exists(), "user's own skill survives");
        let after = plan_uninstall(&paths, false).unwrap();
        assert!(after.items.iter().all(|i| i.action == Action::Absent));
    }

    #[test]
    fn foreign_atlas_mcp_entry_conflicts_and_is_never_removed_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.claude_dir).unwrap();
        std::fs::write(
            &paths.mcp_json,
            r#"{"mcpServers":{"atlas":{"type":"stdio","command":"node","args":["x.js"],"env":{}}}}"#,
        )
        .unwrap();
        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_conflict());
        assert_eq!(plan.items[0].action, Action::Conflict);
        let un = plan_uninstall(&paths, false).unwrap();
        assert_eq!(un.items[0].action, Action::Conflict);
        apply(&un, &paths.skills).unwrap();
        assert!(
            std::fs::read_to_string(&paths.mcp_json)
                .unwrap()
                .contains("x.js")
        );
        let forced = plan_install(&paths, Scope::User, true, "atlas").unwrap();
        assert!(!forced.has_conflict());
        apply(&forced, &paths.skills).unwrap();
        assert_eq!(
            read_json(&paths.mcp_json)["mcpServers"]["atlas"]["args"][0],
            "mcp"
        );
    }

    #[test]
    fn invalid_settings_or_claude_json_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.claude_dir).unwrap();
        std::fs::write(&paths.settings, "{ not json").unwrap();
        assert!(plan_install(&paths, Scope::User, true, "atlas").is_err());
        std::fs::write(&paths.settings, "{}").unwrap();
        std::fs::write(&paths.mcp_json, "[1]").unwrap();
        assert!(plan_install(&paths, Scope::User, true, "atlas").is_err());
    }

    #[test]
    fn project_scope_files_are_deleted_and_pruned_when_atlas_only() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let root = tmp.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        let paths = Paths::resolve(&env, Scope::Project, Some(&root)).unwrap();
        let plan = plan_install(&paths, Scope::Project, false, "atlas").unwrap();
        assert!(!plan.warnings.is_empty(), "skills user-scope warning");
        apply(&plan, &paths.skills).unwrap();
        assert!(root.join(".mcp.json").is_file() && root.join(".claude/settings.json").is_file());
        let un = plan_uninstall(&paths, false).unwrap();
        apply(&un, &paths.skills).unwrap();
        assert!(!root.join(".mcp.json").exists());
        assert!(!root.join(".claude").exists(), "empty .claude pruned");
        assert!(root.exists(), "project root never removed");
    }
}
