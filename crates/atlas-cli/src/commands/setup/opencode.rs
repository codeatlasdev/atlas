//! OpenCode harness: one global plugin file (`~/.config/opencode/plugins/atlas-status.ts`).
//!
//! Primary docs (checked 2026-10, OpenCode v2.0.15):
//! * Plugin dir  — `~/.config/opencode/plugins/` (global); `.opencode/plugins/` (project):
//!   <https://opencode.ai/docs/plugins>
//! * Events      — `session.idle`, `session.status`, `tool.execute.after`, `message.updated`,
//!   `permission.updated` (SDK EventPermissionUpdated), `permission.asked` (plugin-docs alias):
//!   <https://opencode.ai/docs/plugins#events>
//! * Permission reply — `POST /session/{id}/permissions/{permissionID}` with
//!   `{ response: "once" | "always" | "reject" }`:
//!   @opencode-ai/sdk v1.19.3 `PostSessionIdPermissionsPermissionIdData`
//! * Config dir env — `$XDG_CONFIG_HOME/opencode` or `~/.config/opencode`.
//! * OpenCode --prompt flag confirmed by `opencode --help` v2.0.15.
//!
//! Note: OpenCode only has a user-scope plugin dir (global to the user's machine).
//! Project scope is not supported here; `plan_install` returns an error for it.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::plan::{Action, Item, Op, Plan, SkillsTarget};
use super::shared::read_opt;
use super::{Env, Scope};

/// Plugin template; `__ATLAS_VERSION__` is substituted at render time.
const PLUGIN_TEMPLATE: &str = include_str!("opencode_plugin.ts");

/// Marker every atlas-written plugin carries on its first line.
pub const PLUGIN_MARKER: &str = "managed by atlas setup";

pub fn render_plugin() -> String {
    PLUGIN_TEMPLATE.replace("__ATLAS_VERSION__", env!("CARGO_PKG_VERSION"))
}

/// True when `content` is an atlas-owned plugin (marker on line 1 comment).
pub fn plugin_is_owned(content: &str) -> bool {
    content
        .lines()
        .next()
        .map(|l| {
            l.trim_start_matches('/')
                .trim_start()
                .starts_with(PLUGIN_MARKER)
        })
        .unwrap_or(false)
}

pub struct Paths {
    /// `~/.config/opencode/plugins/atlas-status.ts` — the one artifact for install.
    pub plugin_file: PathBuf,
    /// Parent directories emptied by uninstall are pruned up to (not including) this.
    /// Set to `~/.config/opencode/` so we never remove the user's whole config dir.
    pub boundary: PathBuf,
    /// Dummy: opencode setup writes no skills, but the shared plan machinery needs this.
    pub skills: SkillsTarget,
}

impl Paths {
    pub fn resolve(env: &Env, scope: Scope, _project_root: Option<&Path>) -> Result<Self> {
        if scope == Scope::Project {
            bail!(
                "atlas setup opencode only supports user scope; OpenCode plugins are global to the user"
            );
        }
        let plugins_dir = env.opencode_config_dir.join("plugins");
        Ok(Self {
            plugin_file: plugins_dir.join("atlas-status.ts"),
            boundary: env.opencode_config_dir.clone(),
            // No skills for OpenCode; supply a SkillsTarget that resolves to
            // a path that never exists so plan items are no-ops.
            skills: SkillsTarget {
                dir: env.home.join(".atlas/opencode-skills-unused"),
                manifest: env.home.join(".atlas/opencode-skills-manifest-unused"),
            },
        })
    }
}

pub fn plan_install(paths: &Paths, _scope: Scope, _force: bool, _command: &str) -> Result<Plan> {
    let mut plan = Plan::default();
    let desired = render_plugin();
    let existing = read_opt(&paths.plugin_file)?;
    let action = match existing.as_deref() {
        None => Action::Create,
        Some(c) if c == desired => Action::UpToDate,
        Some(c) if plugin_is_owned(c) => Action::Update,
        Some(_) => Action::Conflict,
    };
    plan.items.push(Item {
        label: "Plugin",
        path: paths.plugin_file.clone(),
        action,
        detail: match action {
            Action::Create => "+ atlas-status.ts (status events + permission proxy)".into(),
            Action::Update => "~ atlas-status.ts (refreshed)".into(),
            Action::UpToDate => "atlas-status.ts up to date".into(),
            Action::Conflict => "atlas-status.ts exists but was not written by atlas".into(),
            _ => String::new(),
        },
        op: match action {
            Action::Create | Action::Update => Op::WriteFile(desired),
            Action::Conflict => Op::None, // refused unless --force
            _ => Op::None,
        },
    });
    Ok(plan)
}

pub fn plan_install_force(paths: &Paths) -> Result<Plan> {
    let mut plan = Plan::default();
    let desired = render_plugin();
    plan.items.push(Item {
        label: "Plugin",
        path: paths.plugin_file.clone(),
        action: Action::Update,
        detail: "~ atlas-status.ts (force-replaced)".into(),
        op: Op::WriteFile(desired),
    });
    Ok(plan)
}

pub fn plan_uninstall(paths: &Paths, force: bool) -> Result<Plan> {
    let mut plan = Plan::default();
    let existing = read_opt(&paths.plugin_file)?;
    let action = match existing.as_deref() {
        None => Action::Absent,
        Some(c) if plugin_is_owned(c) => Action::Remove,
        Some(_) if force => Action::Remove,
        Some(_) => Action::Conflict,
    };
    plan.items.push(Item {
        label: "Plugin",
        path: paths.plugin_file.clone(),
        action,
        detail: match action {
            Action::Remove => "- atlas-status.ts".into(),
            Action::Absent => "atlas-status.ts not installed".into(),
            Action::Conflict => {
                "atlas-status.ts exists but was not written by atlas (pass --force)".into()
            }
            _ => String::new(),
        },
        op: match action {
            Action::Remove => Op::RemoveFile {
                boundary: Some(paths.boundary.clone()),
            },
            _ => Op::None,
        },
    });
    Ok(plan)
}

/// `atlas update` refresh gate: the plugin file exists and is atlas-owned.
pub fn installed(paths: &Paths) -> bool {
    read_opt(&paths.plugin_file)
        .ok()
        .flatten()
        .as_deref()
        .map(plugin_is_owned)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::plan::apply;

    fn env_with_tmp(tmp: &Path) -> Env {
        let mut env = Env::for_test(tmp);
        // Point opencode_config_dir at tmp/config/opencode (not ~/.config which may exist)
        env.opencode_config_dir = tmp.join("config/opencode");
        env
    }

    #[test]
    fn plugin_marker_and_version() {
        let rendered = render_plugin();
        assert!(plugin_is_owned(&rendered));
        assert!(
            rendered
                .lines()
                .next()
                .unwrap()
                .contains(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn ownership_requires_marker_on_first_line_comment() {
        assert!(!plugin_is_owned("export const Foo = () => ({});\n"));
        assert!(!plugin_is_owned(
            "// other header\n// managed by atlas setup\n"
        ));
        assert!(!plugin_is_owned("const x = 'managed by atlas setup';\n"));
    }

    #[test]
    fn paths_resolve_under_opencode_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env_with_tmp(tmp.path());
        let p = Paths::resolve(&env, Scope::User, None).unwrap();
        assert_eq!(
            p.plugin_file,
            env.opencode_config_dir.join("plugins/atlas-status.ts")
        );
        assert_eq!(p.boundary, env.opencode_config_dir);
    }

    #[test]
    fn project_scope_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env_with_tmp(tmp.path());
        let root = tmp.path().join("proj");
        assert!(Paths::resolve(&env, Scope::Project, Some(&root)).is_err());
        assert!(Paths::resolve(&env, Scope::Project, None).is_err());
    }

    #[test]
    fn install_apply_idempotent_then_uninstall() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env_with_tmp(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();

        // Fresh install
        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_changes() && !plan.has_conflict());
        assert_eq!(plan.items[0].action, Action::Create);
        apply(&plan, &paths.skills).unwrap();
        assert!(paths.plugin_file.is_file());
        let written = std::fs::read_to_string(&paths.plugin_file).unwrap();
        assert!(plugin_is_owned(&written));
        assert!(written.contains("ATLAS_HOOK_ENDPOINT"));
        assert!(written.contains("permission.updated"));
        assert!(written.contains("permission.asked"));
        assert!(written.contains("postSessionIdPermissionsPermissionId"));

        // Idempotent: second plan is a no-op
        let again = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(!again.has_changes(), "{:?}", again.items);
        assert_eq!(again.items[0].action, Action::UpToDate);

        // installed() gate
        assert!(installed(&paths));

        // Uninstall removes the plugin and prunes the empty plugins/ dir
        let un = plan_uninstall(&paths, false).unwrap();
        assert_eq!(un.items[0].action, Action::Remove);
        apply(&un, &paths.skills).unwrap();
        assert!(!paths.plugin_file.exists());
        // plugins/ dir should be pruned (was empty after removal)
        assert!(!paths.plugin_file.parent().unwrap().exists());

        // Second uninstall is a no-op
        let after = plan_uninstall(&paths, false).unwrap();
        assert_eq!(after.items[0].action, Action::Absent);
        assert!(!installed(&paths));
    }

    #[test]
    fn foreign_plugin_conflicts_without_force_and_unblocks_with_force() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env_with_tmp(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(paths.plugin_file.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.plugin_file,
            "// someone else's plugin\nexport const X = () => ({});",
        )
        .unwrap();

        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_conflict());
        assert_eq!(plan.items[0].action, Action::Conflict);

        let un = plan_uninstall(&paths, false).unwrap();
        assert_eq!(un.items[0].action, Action::Conflict);
        // Foreign file survives apply without force
        apply(&un, &paths.skills).unwrap();
        assert!(paths.plugin_file.exists());

        // With force: replace on install
        let forced = plan_install_force(&paths).unwrap();
        apply(&forced, &paths.skills).unwrap();
        assert!(plugin_is_owned(
            &std::fs::read_to_string(&paths.plugin_file).unwrap()
        ));
    }

    #[test]
    fn render_contains_required_event_types_and_decision_endpoint() {
        let src = render_plugin();
        // Must handle idle
        assert!(src.contains("session.idle"));
        // Must handle permissions (both SDK and plugin-docs names)
        assert!(src.contains("permission.updated"));
        assert!(src.contains("permission.asked"));
        // Must call the decision endpoint
        assert!(src.contains("/hook/opencode/decision"));
        // Must call the OpenCode client permission reply API
        assert!(src.contains("postSessionIdPermissionsPermissionId"));
        // Must route to /hook/opencode
        assert!(src.contains("/hook/opencode"));
        // Must read ATLAS_HOOK_ENDPOINT
        assert!(src.contains("ATLAS_HOOK_ENDPOINT"));
        // Must respect ATLAS_HOOK_DECISION_SECS for bot wait time
        assert!(src.contains("ATLAS_HOOK_DECISION_SECS"));
    }
}
