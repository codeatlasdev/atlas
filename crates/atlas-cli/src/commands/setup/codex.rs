//! Codex harness: MCP entry (`config.toml`) + hooks (`hooks.json`); skills are shared.
//!
//! Primary docs (checked 2026-10, codex-cli 0.158.0):
//! * MCP: `[mcp_servers.<name>]` in `$CODEX_HOME/config.toml` (default `~/.codex`) or, for trusted
//!   projects, `<project>/.codex/config.toml`: <https://developers.openai.com/codex/mcp>
//! * Skills: Codex reads `$HOME/.agents/skills` (and `.agents/skills` in repos), the same
//!   directory OMP uses, so the skills are *shared*, not copied:
//!   <https://developers.openai.com/codex/skills#where-codex-loads-local-skills>
//! * Hooks: `hooks.json` next to a config layer (`~/.codex/hooks.json`, `<repo>/.codex/hooks.json`);
//!   hooks must be reviewed/trusted once in `/hooks` before they run:
//!   <https://developers.openai.com/codex/hooks>
//! * `CODEX_HOME`: <https://developers.openai.com/codex/config-advanced>

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::codex_toml;
use super::hooks_json;
use super::plan::{Plan, SkillsTarget};
use super::shared::{
    hooks_install_item, hooks_uninstall_item, mcp_install_item, mcp_uninstall_item, read_opt,
    shared_skills_target, skills_install_item, skills_uninstall_item, user_scope_skills_warning,
};
use super::{Env, Scope};
use crate::commands::hook::Harness;

pub struct Paths {
    /// `<codex home>/config.toml` or `<project>/.codex/config.toml`.
    pub config: PathBuf,
    /// `hooks.json` next to `config`.
    pub hooks: PathBuf,
    pub skills: SkillsTarget,
    /// Parent directories emptied by uninstall are pruned up to (not including) this.
    pub boundary: PathBuf,
}

impl Paths {
    pub fn resolve(env: &Env, scope: Scope, project_root: Option<&Path>) -> Result<Self> {
        let (base, boundary) = match scope {
            Scope::User => (env.codex_home.clone(), env.codex_home.clone()),
            Scope::Project => {
                let root = project_root.context("project scope needs an atlas.yaml ancestor")?;
                (root.join(".codex"), root.to_path_buf())
            }
        };
        Ok(Self {
            config: base.join("config.toml"),
            hooks: base.join("hooks.json"),
            skills: shared_skills_target(env),
            boundary,
        })
    }
}

pub fn plan_install(paths: &Paths, scope: Scope, force: bool, command: &str) -> Result<Plan> {
    let mut plan = Plan::default();

    let existing = read_opt(&paths.config)?;
    let merged = codex_toml::merge(existing.as_deref(), command, force)
        .with_context(|| format!("{} is not a valid Codex config", paths.config.display()))?;
    plan.items.push(mcp_install_item(
        &paths.config,
        merged,
        "[mcp_servers.atlas]",
        command,
        " (comments and other settings kept)",
    ));

    // Codex reads ~/.agents/skills, which OMP shares: same install, same manifest.
    plan.items
        .push(skills_install_item("Skills (shared)", &paths.skills)?);
    if scope == Scope::Project {
        user_scope_skills_warning(&mut plan, "~/.agents/skills", "Codex");
    }

    let existing = read_opt(&paths.hooks)?;
    let merged = hooks_json::merge(existing.as_deref(), Harness::Codex, command)
        .with_context(|| format!("{} is not a valid Codex hooks file", paths.hooks.display()))?;
    plan.items.push(hooks_install_item(
        &paths.hooks,
        merged,
        "+ SessionStart (status injection), PreToolUse[Bash] (`atlas dev` guard); trust once in `/hooks`",
        "~ atlas hooks (your other hooks kept); re-trust the changed hook in `/hooks`",
    ));
    Ok(plan)
}

pub fn plan_uninstall(env: &Env, paths: &Paths, force: bool) -> Result<Plan> {
    let mut plan = Plan::default();

    let existing = read_opt(&paths.config)?;
    let removed = codex_toml::remove(existing.as_deref(), force)
        .with_context(|| format!("{} is not a valid Codex config", paths.config.display()))?;
    plan.items.push(mcp_uninstall_item(
        &paths.config,
        removed,
        "[mcp_servers.atlas]",
        Some(paths.boundary.clone()),
    ));

    // Shared with OMP: only remove when OMP is not still set up.
    let omp_uses_them = super::omp::user_mcp_is_owned(env);
    plan.items.push(skills_uninstall_item(
        "Skills (shared)",
        &paths.skills,
        omp_uses_them.then_some("OMP is still set up and reads ~/.agents/skills"),
    )?);

    let existing = read_opt(&paths.hooks)?;
    let removed = hooks_json::remove(existing.as_deref(), Harness::Codex)
        .with_context(|| format!("{} is not a valid Codex hooks file", paths.hooks.display()))?;
    plan.items.push(hooks_uninstall_item(
        &paths.hooks,
        removed,
        "your other hooks",
        &paths.boundary,
    ));
    Ok(plan)
}

/// `atlas update` refresh gate: this scope has atlas-owned hooks.
pub fn hooks_installed(paths: &Paths) -> bool {
    hooks_json::has_owned(
        read_opt(&paths.hooks).ok().flatten().as_deref(),
        Harness::Codex,
    )
}

/// True when the user-scope `[mcp_servers.atlas]` is atlas-owned (OMP's uninstall uses this to
/// keep the shared skills while Codex still relies on them).
pub fn user_mcp_is_owned(env: &Env) -> bool {
    codex_toml::has_owned(
        read_opt(&env.codex_home.join("config.toml"))
            .ok()
            .flatten()
            .as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::plan::{Action, apply};

    #[test]
    fn paths_follow_codex_home_and_project_dot_codex() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let p = Paths::resolve(&env, Scope::User, None).unwrap();
        assert_eq!(p.config, env.codex_home.join("config.toml"));
        assert_eq!(p.hooks, env.codex_home.join("hooks.json"));
        assert_eq!(p.skills.dir, env.home.join(".agents/skills"));
        assert!(Paths::resolve(&env, Scope::Project, None).is_err());
        let root = tmp.path().join("proj");
        let p = Paths::resolve(&env, Scope::Project, Some(&root)).unwrap();
        assert_eq!(p.config, root.join(".codex/config.toml"));
        assert_eq!(p.hooks, root.join(".codex/hooks.json"));
    }

    #[test]
    fn merge_idempotent_uninstall_preserves_comments_and_user_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.codex_home).unwrap();
        let cfg =
            "# codex\nmodel = \"gpt-5\" # fast\n\n[projects.\"/p\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&paths.config, cfg).unwrap();
        let hooks = r#"{"description":"mine","hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo stop"}]}]}}"#;
        std::fs::write(&paths.hooks, hooks).unwrap();

        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_changes() && !plan.has_conflict());
        apply(&plan, &paths.skills).unwrap();
        let written = std::fs::read_to_string(&paths.config).unwrap();
        assert!(written.starts_with(cfg));
        assert!(written.contains("[mcp_servers.atlas]"));
        let h: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.hooks).unwrap()).unwrap();
        assert_eq!(h["description"], "mine");
        assert_eq!(h["hooks"]["Stop"][0]["hooks"][0]["command"], "echo stop");
        assert_eq!(h["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert!(paths.skills.dir.join("atlas/SKILL.md").is_file());

        let again = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(
            again.items.iter().all(|i| i.action == Action::UpToDate),
            "{:?}",
            again.items
        );

        let un = plan_uninstall(&env, &paths, false).unwrap();
        apply(&un, &paths.skills).unwrap();
        assert_eq!(
            std::fs::read_to_string(&paths.config).unwrap().trim_end(),
            cfg.trim_end()
        );
        let h: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.hooks).unwrap()).unwrap();
        assert!(h["hooks"].get("PreToolUse").is_none() && h["hooks"].get("SessionStart").is_none());
        assert_eq!(h["hooks"]["Stop"][0]["hooks"][0]["command"], "echo stop");
        assert!(!paths.skills.dir.join("atlas").exists());
        let after = plan_uninstall(&env, &paths, false).unwrap();
        assert!(after.items.iter().all(|i| i.action == Action::Absent));
    }

    #[test]
    fn foreign_server_conflicts_and_survives_uninstall() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.codex_home).unwrap();
        std::fs::write(&paths.config, "[mcp_servers.atlas]\ncommand = \"node\"\n").unwrap();
        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_conflict());
        let un = plan_uninstall(&env, &paths, false).unwrap();
        assert_eq!(un.items[0].action, Action::Conflict);
        apply(&un, &paths.skills).unwrap();
        assert!(
            std::fs::read_to_string(&paths.config)
                .unwrap()
                .contains("node")
        );
        let forced = plan_install(&paths, Scope::User, true, "atlas").unwrap();
        apply(&forced, &paths.skills).unwrap();
        let cfg = std::fs::read_to_string(&paths.config).unwrap();
        assert!(
            cfg.contains("args = [\"mcp\"]") && !cfg.contains("node"),
            "{cfg}"
        );
    }

    #[test]
    fn invalid_config_or_hooks_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(&env.codex_home).unwrap();
        std::fs::write(&paths.config, "[oops").unwrap();
        assert!(plan_install(&paths, Scope::User, true, "atlas").is_err());
        std::fs::write(&paths.config, "").unwrap();
        std::fs::write(&paths.hooks, "{nope").unwrap();
        assert!(plan_install(&paths, Scope::User, true, "atlas").is_err());
    }

    #[test]
    fn shared_skills_survive_codex_uninstall_while_omp_is_set_up_and_vice_versa() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let cpaths = Paths::resolve(&env, Scope::User, None).unwrap();
        let opaths = super::super::omp::Paths::resolve(&env, Scope::User, None).unwrap();
        let o = super::super::omp::plan_install(&opaths, Scope::User, false, "atlas").unwrap();
        apply(&o, &opaths.skills).unwrap();
        let c = plan_install(&cpaths, Scope::User, false, "atlas").unwrap();
        apply(&c, &cpaths.skills).unwrap();
        assert!(env.home.join(".agents/skills/atlas/SKILL.md").is_file());

        // Codex uninstall keeps them: OMP still uses them.
        let un = plan_uninstall(&env, &cpaths, false).unwrap();
        assert!(un.items[1].detail.starts_with("kept:"), "{:?}", un.items[1]);
        apply(&un, &cpaths.skills).unwrap();
        assert!(env.home.join(".agents/skills/atlas/SKILL.md").is_file());

        // OMP uninstall now removes them: Codex no longer owns an entry.
        let un = super::super::omp::plan_uninstall(&env, &opaths, false).unwrap();
        apply(&un, &opaths.skills).unwrap();
        assert!(!env.home.join(".agents/skills/atlas").exists());
    }

    #[test]
    fn project_scope_prunes_dot_codex() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let root = tmp.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        let paths = Paths::resolve(&env, Scope::Project, Some(&root)).unwrap();
        let plan = plan_install(&paths, Scope::Project, false, "atlas").unwrap();
        apply(&plan, &paths.skills).unwrap();
        assert!(
            root.join(".codex/config.toml").is_file() && root.join(".codex/hooks.json").is_file()
        );
        let un = plan_uninstall(&env, &paths, false).unwrap();
        apply(&un, &paths.skills).unwrap();
        assert!(!root.join(".codex").exists());
        assert!(root.exists());
    }
}
