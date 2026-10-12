//! OMP (oh-my-pi) harness: one managed plugin (`~/.atlas/omp-plugin`: MCP server, hook, rules,
//! subagent, skills) linked with `omp plugin link`, plus the shared `~/.agents/skills` copy.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlas_skills::embedded;

use super::json_merge;
use super::plan::{Action, Item, Op, Plan, SkillsTarget};
use super::shared::{
    file_mentions_owned_atlas_mcp_json, mcp_uninstall_item, read_opt, shared_skills_target,
    skills_install_item, skills_uninstall_item,
};
use super::{Env, Scope};

/// Hook template; `__ATLAS_VERSION__` is substituted at render time.
const HOOK_TEMPLATE: &str = include_str!("atlas_hook.ts");
/// Posts lifecycle events to Atlas Desktop (inert outside its terminals).
const STATUS_EXTENSION: &str = include_str!("atlas_status.ts");
/// Marker every atlas-written hook carries on its first line.
pub const HOOK_MARKER: &str = "managed by atlas setup";
/// Name of the plugin as recorded in `omp plugin list`.
pub const PLUGIN_NAME: &str = "atlas-omp";

const RULE_NO_DEV_TUI: &str = "---
description: Never start the interactive atlas dev stack from a tool; use the atlas MCP tools.
condition: '(?m)^\\s*(?:\\S*/)?atlas\\s+dev(?!\\s+--(?:status|ci|headless|help)\\b)'
scope: ['tool:bash']
interruptMode: always
---
`atlas dev` is an interactive TUI the user starts. Do not run it. Use atlas_status / atlas_logs / atlas_restart_service / atlas_wait_ready (MCP), or `atlas dev --status`. If the stack is down, ask the user to start it.
";

const AGENT_ATLAS_OPS: &str = "---
name: atlas-ops
description: Diagnose and fix a failing or unhealthy service in an Atlas project (atlas.yaml, .logs). Reads status and logs through the atlas MCP tools, finds the root cause in code, restarts the service and waits for readiness.
autoloadSkills: [atlas]
---
You operate inside a running `atlas dev` stack that the user owns. Workflow: atlas_status → atlas_logs(service, level=\"error\") → locate the cause in the repository → fix → atlas_restart_service → atlas_wait_ready(service). Never start or stop the whole stack, never run `atlas dev`, never run an action that has a `confirm` message. Report: cause, change made, readiness result.
";

pub fn render_hook() -> String {
    HOOK_TEMPLATE.replace("__ATLAS_VERSION__", env!("CARGO_PKG_VERSION"))
}

/// Ownership: the first line is a comment carrying the marker.
pub fn hook_is_owned(content: &str) -> bool {
    content
        .lines()
        .next()
        .is_some_and(|l| l.starts_with("//") && l.contains(HOOK_MARKER))
}

pub struct Paths {
    /// Legacy (pre-plugin) `mcp.json`; only read for migration and uninstall.
    pub mcp_json: PathBuf,
    /// Legacy (pre-plugin) hook file; only read for migration and uninstall.
    pub hook: PathBuf,
    /// The atlas-owned omp plugin directory (`~/.atlas/omp-plugin`).
    pub plugin_dir: PathBuf,
    /// The `omp` binary, when found on `PATH`.
    pub omp_bin: Option<PathBuf>,
    pub skills: SkillsTarget,
    /// Legacy parent directories emptied by uninstall are pruned up to (not including) this.
    pub boundary: PathBuf,
}

impl Paths {
    /// `project_root` is required for `Scope::Project`.
    pub fn resolve(env: &Env, scope: Scope, project_root: Option<&Path>) -> Result<Self> {
        let (base, boundary) = match scope {
            Scope::User => (env.agent_dir.clone(), env.agent_dir.clone()),
            Scope::Project => {
                let root = project_root.context("project scope needs an atlas.yaml ancestor")?;
                (root.join(".omp"), root.to_path_buf())
            }
        };
        Ok(Self {
            mcp_json: base.join("mcp.json"),
            hook: base.join("hooks").join("pre").join("atlas.ts"),
            plugin_dir: env.home.join(".atlas").join("omp-plugin"),
            omp_bin: env.omp_bin.clone(),
            skills: shared_skills_target(env),
            boundary,
        })
    }
}

/// State of the atlas plugin in `omp plugin list --json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Linked {
    No,
    Disabled,
    Enabled,
}

/// Run `omp plugin list --json` (5 s timeout) and find the atlas plugin.
fn plugin_state(omp: &Path) -> Result<Linked> {
    let mut child = Command::new(omp)
        .args(["plugin", "list", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("running {} plugin list", omp.display()))?;
    let mut stdout = child.stdout.take().context("omp plugin list: no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("`omp plugin list --json` timed out after 5s");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let text = reader.join().unwrap_or_default();
    if !status.success() {
        anyhow::bail!("`omp plugin list --json` exited {status}");
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).context("`omp plugin list --json` printed invalid JSON")?;
    // Linked plugins are listed under `npm`; match by name across every array to stay robust.
    let found = v
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter_map(|a| a.as_array())
        .flatten()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(PLUGIN_NAME));
    Ok(match found {
        None => Linked::No,
        Some(p) if p.get("enabled").and_then(|e| e.as_bool()) == Some(false) => Linked::Disabled,
        Some(_) => Linked::Enabled,
    })
}

/// Every file of the plugin, relative to the plugin dir, with its desired content.
fn plugin_files(command: &str) -> Vec<(String, String)> {
    let package = serde_json::json!({
        "name": PLUGIN_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "type": "module",
        "description": "Atlas dev-environment tools for omp: MCP server, hook, skills, rules, subagent",
        "omp": { "extensions": ["./hooks/pre/atlas.ts", "./extensions/atlas-status.ts"] },
    });
    let mcp = serde_json::json!({
        "mcpServers": { json_merge::SERVER_NAME: { "type": "stdio", "command": command, "args": ["mcp"] } },
    });
    let json = |v: &serde_json::Value| {
        let mut s = serde_json::to_string_pretty(v).expect("static JSON serializes");
        s.push('\n');
        s
    };
    let mut files = vec![
        ("package.json".to_string(), json(&package)),
        ("hooks/pre/atlas.ts".to_string(), render_hook()),
        (
            "extensions/atlas-status.ts".to_string(),
            STATUS_EXTENSION.replace("__ATLAS_VERSION__", env!("CARGO_PKG_VERSION")),
        ),
        (".mcp.json".to_string(), json(&mcp)),
        (
            "rules/atlas-no-dev-tui.md".to_string(),
            RULE_NO_DEV_TUI.to_string(),
        ),
        (
            "agents/atlas-ops.md".to_string(),
            AGENT_ATLAS_OPS.to_string(),
        ),
    ];
    for skill in embedded::all() {
        for f in &skill.files {
            files.push((
                format!("skills/{}/{}", skill.name, f.path),
                f.content.to_string(),
            ));
        }
    }
    files
}

fn plugin_item(path: PathBuf, desired: String) -> Result<Item> {
    let item = |action, detail: &str, op| Item {
        label: "Plugin",
        path: path.clone(),
        action,
        detail: detail.to_string(),
        op,
    };
    Ok(match read_opt(&path)? {
        None => item(Action::Create, "", Op::WriteFile(desired)),
        Some(c) if c == desired => item(Action::UpToDate, "", Op::None),
        // The plugin dir is atlas-owned: never a conflict.
        Some(_) => item(Action::Update, "", Op::WriteFile(desired)),
    })
}

/// Build the install plan.
pub fn plan_install(paths: &Paths, scope: Scope, _force: bool, command: &str) -> Result<Plan> {
    let mut plan = Plan::default();
    if scope == Scope::Project {
        plan.warnings.push(
            "omp plugins are user-level; project scope only removes legacy project files".into(),
        );
    }

    // 1. Plugin files
    for (rel, content) in plugin_files(command) {
        plan.items
            .push(plugin_item(paths.plugin_dir.join(rel), content)?);
    }

    // 2. Link the plugin into omp
    let link = |action, detail: String, op| Item {
        label: "Plugin link",
        path: paths.plugin_dir.clone(),
        action,
        detail,
        op,
    };
    let exec = |args: &[&str], omp: &Path| Op::Exec {
        program: omp.to_string_lossy().into_owned(),
        args: args.iter().map(|s| s.to_string()).collect(),
    };
    let dir = paths.plugin_dir.to_string_lossy().into_owned();
    plan.items.push(match &paths.omp_bin {
        None => link(
            Action::Skipped,
            format!("omp not found on PATH; run: omp plugin link {dir}"),
            Op::None,
        ),
        Some(omp) => match plugin_state(omp)? {
            Linked::No => link(
                Action::Create,
                format!("omp plugin link {dir}"),
                exec(&["plugin", "link", &dir], omp),
            ),
            Linked::Disabled => link(
                Action::Update,
                format!("omp plugin enable {PLUGIN_NAME}"),
                exec(&["plugin", "enable", PLUGIN_NAME], omp),
            ),
            Linked::Enabled => link(Action::UpToDate, String::new(), Op::None),
        },
    });
    plan.warnings.push(
        "restart omp sessions to load the new hook (/reload-plugins refreshes skills and MCP only)"
            .into(),
    );

    // 3. Shared skills for Claude Code / Codex (omp prefers the plugin copy)
    plan.items
        .push(skills_install_item("Skills", &paths.skills)?);

    // 4. Legacy migration: the same hook/MCP entry must not load twice.
    legacy_removals(paths, &mut plan, Legacy::MigrateOwned)?;

    Ok(plan)
}

/// How [`legacy_removals`] treats pre-plugin artifacts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Legacy {
    /// Install: remove only atlas-owned files; foreign or unreadable ones are not our business.
    MigrateOwned,
    /// Uninstall: also report foreign files as conflicts.
    Uninstall { force: bool },
}

/// Items removing atlas-owned pre-plugin artifacts (hook file, `mcpServers.atlas`).
fn legacy_removals(paths: &Paths, plan: &mut Plan, mode: Legacy) -> Result<()> {
    let force = matches!(mode, Legacy::Uninstall { force: true });
    let hook_item = |action, detail: String, op| Item {
        label: "Legacy hook",
        path: paths.hook.clone(),
        action,
        detail,
        op,
    };
    match read_opt(&paths.hook)? {
        Some(c) if hook_is_owned(&c) || force => plan.items.push(hook_item(
            Action::Remove,
            "replaced by the plugin hook".into(),
            Op::RemoveFile {
                boundary: Some(paths.boundary.clone()),
            },
        )),
        Some(_) if mode != Legacy::MigrateOwned => plan.items.push(hook_item(
            Action::Conflict,
            format!(
                "not managed by atlas (no `{HOOK_MARKER}` header) — left alone (use --force to remove)"
            ),
            Op::None,
        )),
        _ => {}
    }

    let existing = read_opt(&paths.mcp_json)?;
    let removed = match json_merge::remove(existing.as_deref(), force) {
        Ok(r) => r,
        Err(_) if mode == Legacy::MigrateOwned => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("{} is not a valid MCP config", paths.mcp_json.display())
            });
        }
    };
    let item = mcp_uninstall_item(
        &paths.mcp_json,
        removed,
        "mcpServers.atlas",
        Some(paths.boundary.clone()),
    );
    let keep = match item.action {
        Action::Absent => false,
        Action::Conflict => mode != Legacy::MigrateOwned,
        _ => true,
    };
    if keep {
        plan.items.push(item);
    }
    Ok(())
}

/// Build the uninstall plan: only atlas-owned artifacts are touched.
pub fn plan_uninstall(env: &Env, paths: &Paths, force: bool) -> Result<Plan> {
    let mut plan = Plan::default();

    let link = |action, op| Item {
        label: "Plugin link",
        path: paths.plugin_dir.clone(),
        action,
        detail: String::new(),
        op,
    };
    plan.items.push(match &paths.omp_bin {
        Some(omp) if plugin_state(omp)? != Linked::No => link(
            Action::Remove,
            Op::Exec {
                program: omp.to_string_lossy().into_owned(),
                args: vec!["plugin".into(), "uninstall".into(), PLUGIN_NAME.into()],
            },
        ),
        _ => link(Action::Absent, Op::None),
    });

    plan.items.push(Item {
        label: "Plugin",
        path: paths.plugin_dir.clone(),
        action: if paths.plugin_dir.exists() {
            Action::Remove
        } else {
            Action::Absent
        },
        detail: String::new(),
        op: if paths.plugin_dir.exists() {
            Op::RemoveDir
        } else {
            Op::None
        },
    });

    // The skills dir is shared with Codex: keep it while Codex is still set up.
    let codex_uses_them = super::codex::user_mcp_is_owned(env);
    plan.items.push(skills_uninstall_item(
        "Skills",
        &paths.skills,
        codex_uses_them.then_some("Codex is still set up and reads ~/.agents/skills"),
    )?);

    // Pre-plugin artifacts: listed only when present.
    legacy_removals(paths, &mut plan, Legacy::Uninstall { force })?;
    Ok(plan)
}

/// True when OMP is set up: the atlas plugin dir exists or the legacy user-scope MCP entry is
/// atlas-owned (used by Codex's uninstall to decide whether the shared skills must stay).
pub fn user_mcp_is_owned(env: &Env) -> bool {
    env.home.join(".atlas/omp-plugin/package.json").is_file()
        || file_mentions_owned_atlas_mcp_json(&env.agent_dir.join("mcp.json"))
}

/// `atlas update` gate: has a previous `atlas setup omp --apply` put the plugin (or the legacy
/// hook) here?
pub fn installed(paths: &Paths) -> bool {
    paths.plugin_dir.join("package.json").is_file()
        || std::fs::read_to_string(&paths.hook).is_ok_and(|c| hook_is_owned(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::plan::apply;

    #[test]
    fn hook_ownership_requires_marker_on_first_line_comment() {
        assert!(hook_is_owned(&render_hook()));
        assert!(!hook_is_owned("export default function () {}\n"));
        assert!(!hook_is_owned("// my hook\n// managed by atlas setup\n"));
        assert!(!hook_is_owned("const x = 'managed by atlas setup';\n"));
        assert!(
            render_hook()
                .lines()
                .next()
                .unwrap()
                .contains(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn plugin_ships_status_extension_and_lists_both() {
        let files = plugin_files("atlas");
        let get = |p: &str| files.iter().find(|(r, _)| r == p).map(|(_, c)| c.clone());
        let ext = get("extensions/atlas-status.ts").expect("status extension");
        assert!(hook_is_owned(&ext));
        let pkg: serde_json::Value = serde_json::from_str(&get("package.json").unwrap()).unwrap();
        assert_eq!(
            pkg["omp"]["extensions"],
            serde_json::json!(["./hooks/pre/atlas.ts", "./extensions/atlas-status.ts"])
        );
    }

    #[test]
    fn install_apply_idempotent_then_uninstall_removes_only_owned() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        // Unrelated server and foreign hook sibling in omp's own config must survive.
        std::fs::create_dir_all(&env.agent_dir).unwrap();
        std::fs::write(
            &paths.mcp_json,
            r#"{"mcpServers":{"other":{"command":"x"}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(paths.hook.parent().unwrap()).unwrap();
        let sibling = paths.hook.parent().unwrap().join("mine.ts");
        std::fs::write(&sibling, "export default () => {}").unwrap();

        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(plan.has_changes() && !plan.has_conflict());
        // No omp on PATH: the link step is reported, never blocking.
        let link = plan
            .items
            .iter()
            .find(|i| i.label == "Plugin link")
            .unwrap();
        assert_eq!(link.action, Action::Skipped);
        assert!(link.detail.contains("omp plugin link"), "{}", link.detail);
        apply(&plan, &paths.skills).unwrap();

        for rel in [
            "package.json",
            ".mcp.json",
            "hooks/pre/atlas.ts",
            "rules/atlas-no-dev-tui.md",
            "agents/atlas-ops.md",
            "skills/atlas/SKILL.md",
            "skills/atlas/yaml.md",
        ] {
            assert!(paths.plugin_dir.join(rel).is_file(), "{rel}");
        }
        let pkg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(paths.plugin_dir.join("package.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(pkg["name"], PLUGIN_NAME);
        assert_eq!(pkg["omp"]["extensions"][0], "./hooks/pre/atlas.ts");
        let mcp: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(paths.plugin_dir.join(".mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(mcp["mcpServers"]["atlas"]["command"], "atlas");
        assert_eq!(mcp["mcpServers"]["atlas"]["args"][0], "mcp");

        let again = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(!again.has_changes(), "second run must be a no-op");
        assert!(
            again
                .items
                .iter()
                .all(|i| matches!(i.action, Action::UpToDate | Action::Skipped))
        );

        let un = plan_uninstall(&env, &paths, false).unwrap();
        apply(&un, &paths.skills).unwrap();
        assert!(!paths.plugin_dir.exists());
        assert!(!paths.skills.dir.join("atlas").join("SKILL.md").exists());
        // Foreign config is untouched by uninstall.
        let mcp: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.mcp_json).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["other"]["command"], "x");
        assert!(sibling.exists(), "foreign hook sibling must survive");
        let after = plan_uninstall(&env, &paths, false).unwrap();
        assert!(after.items.iter().all(|i| i.action == Action::Absent));
    }

    #[test]
    fn legacy_owned_artifacts_are_migrated_and_foreign_ones_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        let paths = Paths::resolve(&env, Scope::User, None).unwrap();
        std::fs::create_dir_all(paths.hook.parent().unwrap()).unwrap();
        std::fs::write(&paths.hook, render_hook()).unwrap();
        std::fs::write(
            &paths.mcp_json,
            r#"{"mcpServers":{"atlas":{"command":"atlas","args":["mcp"]},"keep":{"command":"k"}}}"#,
        )
        .unwrap();

        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(!plan.has_conflict());
        apply(&plan, &paths.skills).unwrap();
        assert!(!paths.hook.exists(), "owned legacy hook removed");
        let mcp: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.mcp_json).unwrap()).unwrap();
        assert!(mcp["mcpServers"].get("atlas").is_none());
        assert_eq!(mcp["mcpServers"]["keep"]["command"], "k");

        // Foreign legacy files: install ignores them, uninstall reports them, nothing is removed.
        std::fs::create_dir_all(paths.hook.parent().unwrap()).unwrap();
        std::fs::write(&paths.hook, "// user's own atlas hook\n").unwrap();
        std::fs::write(
            &paths.mcp_json,
            r#"{"mcpServers":{"atlas":{"command":"node","args":["x.js"]}}}"#,
        )
        .unwrap();
        let plan = plan_install(&paths, Scope::User, false, "atlas").unwrap();
        assert!(!plan.has_conflict());
        assert!(plan.items.iter().all(|i| !i.label.starts_with("Legacy")));
        apply(&plan, &paths.skills).unwrap();
        assert!(paths.hook.exists());
        assert!(
            std::fs::read_to_string(&paths.mcp_json)
                .unwrap()
                .contains("x.js")
        );
        let un = plan_uninstall(&env, &paths, false).unwrap();
        assert!(un.has_conflict());
        let forced = plan_uninstall(&env, &paths, true).unwrap();
        assert!(!forced.has_conflict());
        apply(&forced, &paths.skills).unwrap();
        assert!(!paths.hook.exists());
    }

    #[test]
    fn project_scope_paths_and_requirement() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env::for_test(tmp.path());
        assert!(Paths::resolve(&env, Scope::Project, None).is_err());
        let root = tmp.path().join("proj");
        let p = Paths::resolve(&env, Scope::Project, Some(&root)).unwrap();
        // Legacy locations follow the scope; the plugin itself is always user-level.
        assert_eq!(p.mcp_json, root.join(".omp/mcp.json"));
        assert_eq!(p.hook, root.join(".omp/hooks/pre/atlas.ts"));
        assert_eq!(p.plugin_dir, env.home.join(".atlas/omp-plugin"));
        assert_eq!(p.skills.dir, env.home.join(".agents/skills"));
        let plan = plan_install(&p, Scope::Project, false, "atlas").unwrap();
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("omp plugins are user-level"))
        );
    }

    #[test]
    fn rule_condition_matches_interactive_atlas_dev_only() {
        // The rule text is a JS regex evaluated by omp; sanity-check the shipped file shape.
        assert!(RULE_NO_DEV_TUI.starts_with("---\n"));
        assert!(RULE_NO_DEV_TUI.contains("scope: ['tool:bash']"));
        assert!(RULE_NO_DEV_TUI.contains("interruptMode: always"));
        assert!(AGENT_ATLAS_OPS.contains("name: atlas-ops"));
    }
}
