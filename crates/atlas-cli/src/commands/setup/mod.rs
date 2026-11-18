//! `atlas setup` — wire atlas into an AI agent harness.
//!
//! Each harness is one module that turns an [`Env`] into a [`plan::Plan`]
//! (dry-run output == apply behaviour) and applies it. Only OMP exists today;
//! adding a harness means a sibling module + a `Harness` variant + a clap
//! subcommand, with the plan/apply/rendering machinery shared.

mod claude_code;
mod codex;
mod codex_toml;
mod hooks_json;
mod json_merge;
mod omp;
mod opencode;
mod plan;
mod shared;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atlas_i18n::t;
use clap::{Args, Subcommand, ValueEnum};

use super::AtlasCommand;
use plan::{Plan, SkillsTarget};

/// Bold, for a command inside a message: the styling stays in code, the catalog holds words.
fn bold(s: &str) -> String {
    format!("\x1b[1m{s}\x1b[0m")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    /// The agent's user config (~/.omp/agent, ~/.claude, ~/.codex) — every project
    User,
    /// <project>/ — only the project containing atlas.yaml
    Project,
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    #[command(subcommand)]
    pub harness: Option<Harness>,
}

#[derive(Debug, Subcommand)]
pub enum Harness {
    /// Set up OMP (oh-my-pi): one plugin with the MCP server, hook, rules, subagent and skills
    Omp(HarnessArgs),
    /// Set up Claude Code: MCP server entry, skills, and hooks
    #[command(name = "claude-code")]
    ClaudeCode(HarnessArgs),
    /// Set up Codex: MCP server entry and hooks (skills are shared with OMP)
    Codex(HarnessArgs),
    /// Set up OpenCode: a global plugin for status events and remote permission approval
    #[command(name = "opencode")]
    OpenCode(HarnessArgs),
}

#[derive(Debug, Args)]
pub struct HarnessArgs {
    /// Where to write the MCP entry and hooks
    #[arg(long, value_enum, default_value = "user")]
    pub scope: Scope,
    /// Perform the changes (default is a dry-run that only prints the plan)
    #[arg(long)]
    pub apply: bool,
    /// Replace an `atlas` MCP entry / hook file that atlas did not create
    #[arg(long)]
    pub force: bool,
    /// Remove only atlas-owned artifacts
    #[arg(long)]
    pub uninstall: bool,
    /// Exit 0 when atlas is set up for this agent, 1 when not, 2 when its hooks are installed
    /// but differ from this atlas's (another atlas version wrote them; `atlas update` refreshes
    /// them); writes and prints nothing (Atlas Desktop and Mobile ask this)
    #[arg(long, conflicts_with_all = ["uninstall", "apply", "refresh"])]
    pub check: bool,
    /// Used by `atlas update`: re-apply quietly, only if atlas was already set up
    #[arg(long, hide = true, conflicts_with_all = ["uninstall", "apply"])]
    pub refresh: bool,
}

/// The agents `atlas setup` knows, in detection order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Omp,
    ClaudeCode,
    Codex,
    OpenCode,
}

impl Agent {
    const ALL: [Agent; 4] = [Agent::Omp, Agent::ClaudeCode, Agent::Codex, Agent::OpenCode];

    /// The built-in agent's id in `data/agents.json`: its name, binary, label and hooks come from there.
    fn id(self) -> &'static str {
        match self {
            Agent::Omp => "omp",
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
            Agent::OpenCode => "opencode",
        }
    }

    fn def(self) -> &'static atlas_core::agents::AgentDef {
        atlas_core::agents::get(self.id()).expect("every setup agent is in data/agents.json")
    }

    /// Subcommand name (`atlas setup <name>`).
    fn name(self) -> &'static str {
        &self.def().setup
    }

    /// Executable looked up on `PATH`.
    fn binary(self) -> &'static str {
        &self.def().bin
    }

    fn title(self) -> &'static str {
        &self.def().label
    }

    /// Where this agent's user-scope config lives, for the scope banner.
    fn user_dir(self, env: &Env) -> &Path {
        match self {
            Agent::Omp => &env.agent_dir,
            Agent::ClaudeCode => &env.claude_dir,
            Agent::Codex => &env.codex_home,
            Agent::OpenCode => &env.opencode_config_dir,
        }
    }

    fn project_dir_name(self) -> String {
        match self {
            Agent::Omp => ".omp/".into(),
            Agent::ClaudeCode => ".mcp.json + .claude/".into(),
            Agent::Codex => ".codex/".into(),
            Agent::OpenCode => t!("cmd_setup_project_user_only"),
        }
    }

    fn next_steps(self) -> Vec<String> {
        let status_hint = t!("cmd_setup_steps_status_hint");
        match self {
            Agent::Omp => vec![
                t!(
                    "cmd_setup_steps_omp_restart",
                    command = bold("/reload-plugins")
                ),
                t!(
                    "cmd_setup_steps_omp_verify",
                    mcp = bold("/mcp list"),
                    plugins = bold("omp plugin list")
                ),
                status_hint,
            ],
            Agent::ClaudeCode => vec![
                t!("cmd_setup_steps_claude_restart"),
                t!(
                    "cmd_setup_steps_claude_verify",
                    command = bold("claude mcp get atlas"),
                    slash = bold("/mcp")
                ),
                status_hint,
            ],
            Agent::Codex => vec![
                t!("cmd_setup_steps_codex_restart", command = bold("/hooks")),
                t!(
                    "cmd_setup_steps_codex_verify",
                    command = bold("codex mcp get atlas"),
                    slash = bold("/mcp")
                ),
                status_hint,
            ],
            Agent::OpenCode => vec![
                t!("cmd_setup_steps_opencode_restart"),
                t!("cmd_setup_steps_opencode_verify"),
                t!("cmd_setup_steps_opencode_crew"),
            ],
        }
    }

    fn removal_hint(self) -> String {
        t!("cmd_setup_removal_restart", label = self.title())
    }

    fn resolve(self, env: &Env, scope: Scope, root: Option<&Path>) -> Result<AgentPaths> {
        Ok(match self {
            Agent::Omp => AgentPaths::Omp(omp::Paths::resolve(env, scope, root)?),
            Agent::ClaudeCode => {
                AgentPaths::ClaudeCode(claude_code::Paths::resolve(env, scope, root)?)
            }
            Agent::Codex => AgentPaths::Codex(codex::Paths::resolve(env, scope, root)?),
            Agent::OpenCode => AgentPaths::OpenCode(opencode::Paths::resolve(env, scope, root)?),
        })
    }
}

/// Resolved artifact locations for one agent and scope.
enum AgentPaths {
    Omp(omp::Paths),
    ClaudeCode(claude_code::Paths),
    Codex(codex::Paths),
    OpenCode(opencode::Paths),
}

impl AgentPaths {
    fn skills(&self) -> &SkillsTarget {
        match self {
            AgentPaths::Omp(p) => &p.skills,
            AgentPaths::ClaudeCode(p) => &p.skills,
            AgentPaths::Codex(p) => &p.skills,
            AgentPaths::OpenCode(p) => &p.skills,
        }
    }

    fn plan_install(&self, scope: Scope, force: bool, command: &str) -> Result<Plan> {
        match self {
            AgentPaths::Omp(p) => omp::plan_install(p, scope, force, command),
            AgentPaths::ClaudeCode(p) => claude_code::plan_install(p, scope, force, command),
            AgentPaths::Codex(p) => codex::plan_install(p, scope, force, command),
            AgentPaths::OpenCode(p) => {
                if force {
                    opencode::plan_install_force(p)
                } else {
                    opencode::plan_install(p, scope, force, command)
                }
            }
        }
    }

    fn plan_uninstall(&self, env: &Env, force: bool) -> Result<Plan> {
        match self {
            AgentPaths::Omp(p) => omp::plan_uninstall(env, p, force),
            AgentPaths::ClaudeCode(p) => claude_code::plan_uninstall(p, force),
            AgentPaths::Codex(p) => codex::plan_uninstall(env, p, force),
            AgentPaths::OpenCode(p) => opencode::plan_uninstall(p, force),
        }
    }

    /// Has a previous `atlas setup <agent> --apply` put hooks here? (`atlas update` gate.)
    fn hooks_installed(&self) -> bool {
        match self {
            AgentPaths::Omp(p) => omp::installed(p),
            AgentPaths::ClaudeCode(p) => claude_code::hooks_installed(p),
            AgentPaths::Codex(p) => codex::hooks_installed(p),
            AgentPaths::OpenCode(p) => opencode::installed(p),
        }
    }

    /// Where the refresh message points.
    fn hook_path(&self) -> &Path {
        match self {
            AgentPaths::Omp(p) => &p.plugin_dir,
            AgentPaths::ClaudeCode(p) => &p.settings,
            AgentPaths::Codex(p) => &p.hooks,
            AgentPaths::OpenCode(p) => &p.plugin_file,
        }
    }
}

/// Everything the harness code reads from the machine. Injectable for tests.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    /// OMP agent dir: `$PI_CODING_AGENT_DIR`, else `~/.omp/agent`.
    pub agent_dir: PathBuf,
    /// Claude Code config dir: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
    pub claude_dir: PathBuf,
    /// Claude Code user-scope MCP file: `$CLAUDE_CONFIG_DIR/.claude.json` when that variable is
    /// set, else `~/.claude.json` (note: *not* inside `~/.claude`).
    pub claude_json: PathBuf,
    /// Codex home: `$CODEX_HOME`, else `~/.codex`.
    pub codex_home: PathBuf,
    /// OpenCode config dir: `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`.
    pub opencode_config_dir: PathBuf,
    pub cwd: PathBuf,
    pub atlas_on_path: bool,
    /// The `omp` binary on `PATH`, when present.
    pub omp_bin: Option<PathBuf>,
}

fn dir_from_env(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(atlas_core::util::expand_tilde(&d.to_string_lossy())))
}

impl Env {
    pub fn from_process() -> Result<Self> {
        let home = atlas_core::paths::home_dir();
        let agent_dir =
            dir_from_env("PI_CODING_AGENT_DIR").unwrap_or_else(|| home.join(".omp").join("agent"));
        let claude_cfg = dir_from_env("CLAUDE_CONFIG_DIR");
        let claude_dir = claude_cfg.clone().unwrap_or_else(|| home.join(".claude"));
        let claude_json = match &claude_cfg {
            Some(d) => d.join(".claude.json"),
            None => home.join(".claude.json"),
        };
        let codex_home = dir_from_env("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
        Ok(Self {
            home: home.clone(),
            agent_dir,
            claude_dir,
            claude_json,
            codex_home,
            opencode_config_dir: dir_from_env("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("opencode"),
            cwd: std::env::current_dir().context(t!("cmd_setup_cwd_unreadable"))?,
            atlas_on_path: which("atlas").is_some(),
            omp_bin: which("omp"),
        })
    }
}

#[cfg(test)]
impl Env {
    /// Hermetic environment rooted in `tmp`.
    pub fn for_test(tmp: &Path) -> Self {
        Self {
            home: tmp.join("home"),
            agent_dir: tmp.join("agent"),
            claude_dir: tmp.join("claude"),
            claude_json: tmp.join("claude/.claude.json"),
            codex_home: tmp.join("codex"),
            opencode_config_dir: tmp.join("config/opencode"),
            cwd: tmp.join("proj"),
            atlas_on_path: true,
            omp_bin: None,
        }
    }
}

/// Unwrap the text of a `Create`/`Update` merge outcome.
#[cfg(test)]
pub fn merge_text(o: plan::MergeOutcome) -> String {
    match o {
        plan::MergeOutcome::Create(t) | plan::MergeOutcome::Update(t) => t,
        other => panic!("expected write, got {other:?}"),
    }
}

pub use atlas_core::paths::which;

impl AtlasCommand for SetupArgs {
    async fn run(self) -> Result<()> {
        match self.harness {
            None => detect(),
            Some(Harness::Omp(args)) => run_agent(Agent::Omp, args),
            Some(Harness::ClaudeCode(args)) => run_agent(Agent::ClaudeCode, args),
            Some(Harness::Codex(args)) => run_agent(Agent::Codex, args),
            Some(Harness::OpenCode(args)) => run_agent(Agent::OpenCode, args),
        }
    }
}

/// `atlas setup` with no harness: report what is installed and what to run.
fn detect() -> Result<()> {
    let env = Env::from_process()?;
    println!("\x1b[1;34m◆\x1b[0m {}", t!("cmd_setup_detect_title"));
    println!();
    let mut found = Vec::new();
    for agent in Agent::ALL {
        let pad = " ".repeat(12 - agent.name().len());
        match which(agent.binary()) {
            Some(p) => {
                println!(
                    "  \x1b[32m✓\x1b[0m {}{pad}{}",
                    agent.name(),
                    t!("cmd_setup_detect_found", path = p.display().to_string())
                );
                let state = match agent
                    .resolve(&env, Scope::User, None)
                    .and_then(|paths| paths.plan_install(Scope::User, false, "atlas"))
                {
                    Ok(plan) if plan.has_conflict() => t!("cmd_setup_state_attention"),
                    Ok(plan) if plan.has_changes() => t!("cmd_setup_state_outdated"),
                    Ok(_) => t!("cmd_setup_state_ok"),
                    Err(e) => t!("cmd_setup_state_error", error = format!("{e:#}")),
                };
                println!("        {}", t!("cmd_setup_detect_status", state = state));
                found.push(agent);
            }
            None => println!(
                "  \x1b[2m·\x1b[0m {}{pad}{}",
                agent.name(),
                t!("cmd_setup_detect_not_found")
            ),
        }
    }
    println!();
    if found.is_empty() {
        println!(
            "  {}",
            t!(
                "cmd_setup_detect_none",
                command = bold("atlas setup <agent> --apply")
            )
        );
    } else {
        for agent in found {
            println!("  {}:", agent.title());
            println!(
                "    {}",
                t!(
                    "cmd_setup_detect_preview",
                    command = bold(&format!("atlas setup {}", agent.name()))
                )
            );
            println!(
                "    {}",
                t!(
                    "cmd_setup_detect_install",
                    command = bold(&format!("atlas setup {} --apply", agent.name()))
                )
            );
        }
    }
    Ok(())
}

/// `--check`'s exit code: 0 set up, 1 not set up, 2 set up by a different atlas.
fn check_code(hooks_installed: bool, differs: bool) -> i32 {
    match (differs, hooks_installed) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => 2,
    }
}

fn run_agent(agent: Agent, args: HarnessArgs) -> Result<()> {
    let env = Env::from_process()?;
    let project_root = atlas_tui::config::find_root(&env.cwd);
    let project_root = project_root.map(|r| r.canonicalize().unwrap_or(r));

    if args.refresh {
        return refresh_agent(agent, &env, project_root.as_deref());
    }

    if args.scope == Scope::Project && project_root.is_none() {
        bail!(t!(
            "cmd_setup_scope_project_missing",
            dir = env.cwd.display().to_string()
        ));
    }
    let paths = agent.resolve(&env, args.scope, project_root.as_deref())?;

    let mut warnings = Vec::new();
    let (mut plan, title) = if args.uninstall {
        (
            paths.plan_uninstall(&env, args.force)?,
            t!("cmd_setup_title_uninstall", label = agent.title()),
        )
    } else {
        let command = shared::resolve_command(&env, &mut warnings)?;
        (
            paths.plan_install(args.scope, args.force, &command)?,
            t!("cmd_setup_title_install", label = agent.title()),
        )
    };
    plan.warnings.splice(0..0, warnings);
    if args.check {
        std::process::exit(check_code(
            paths.hooks_installed(),
            plan.has_changes() || plan.has_conflict(),
        ));
    }

    let scope_label = match args.scope {
        Scope::User => t!(
            "cmd_setup_scope_user",
            dir = agent.user_dir(&env).display().to_string()
        ),
        Scope::Project => t!(
            "cmd_setup_scope_project",
            root = project_root
                .as_deref()
                .unwrap_or(&env.cwd)
                .display()
                .to_string(),
            what = agent.project_dir_name()
        ),
    };
    println!(
        "\x1b[1;34m◆\x1b[0m {title}  \x1b[2m{}\x1b[0m",
        t!("cmd_setup_scope_banner", scope = scope_label)
    );
    println!();

    if !args.apply {
        print!("{}", plan.render(false));
        println!();
        finish_dry_run(agent, &plan, args.uninstall);
        return Ok(());
    }

    if plan.has_conflict() {
        print!("{}", plan.render(false));
        println!();
        bail!(t!("cmd_setup_conflicts_refused"));
    }
    if !plan.has_changes() {
        print!("{}", plan.render(true));
        println!();
        println!(
            "  \x1b[32m✓\x1b[0m {}",
            if args.uninstall {
                t!("cmd_setup_applied_nothing_to_remove")
            } else {
                t!("cmd_setup_action_up_to_date")
            }
        );
        return Ok(());
    }
    plan::apply(&plan, paths.skills())?;
    print!("{}", plan.render(true));
    println!();
    if args.uninstall {
        println!("  {}", agent.removal_hint());
    } else {
        println!("  {}", t!("cmd_setup_next_steps"));
        for (i, step) in agent.next_steps().iter().enumerate() {
            println!("    {}. {step}", i + 1);
        }
    }
    Ok(())
}

fn finish_dry_run(agent: Agent, plan: &Plan, uninstall: bool) {
    let flag = if uninstall {
        "--uninstall --apply"
    } else {
        "--apply"
    };
    if plan.has_conflict() {
        println!("  \x1b[31m✗\x1b[0m {}", t!("cmd_setup_dry_conflicts"));
    } else if plan.has_changes() {
        println!(
            "  {}",
            t!(
                "cmd_setup_dry_run",
                command = bold(&format!("atlas setup {} {flag}", agent.name()))
            )
        );
    } else if uninstall {
        println!("  {}", t!("cmd_setup_dry_nothing_to_remove"));
    } else {
        println!("  \x1b[32m✓\x1b[0m {}", t!("cmd_setup_dry_up_to_date"));
    }
}

/// `atlas update` hook: refresh what a previous `atlas setup <agent> --apply` created, at user
/// scope and (when run inside an atlas project) at project scope. Silent when nothing was
/// set up; never touches foreign entries.
fn refresh_agent(agent: Agent, env: &Env, project_root: Option<&Path>) -> Result<()> {
    let mut scopes = vec![(Scope::User, None)];
    if let Some(root) = project_root {
        scopes.push((Scope::Project, Some(root)));
    }
    for (scope, root) in scopes {
        let paths = agent.resolve(env, scope, root)?;
        if !paths.hooks_installed() {
            continue;
        }
        let mut warnings = Vec::new();
        let command = shared::resolve_command(env, &mut warnings)?;
        let mut plan = paths.plan_install(scope, false, &command)?;
        // Refresh only rewrites; never fail the update on foreign entries.
        plan.items.retain(|i| i.action != plan::Action::Conflict);
        if plan.has_changes() {
            plan::apply(&plan, paths.skills())?;
            println!(
                "  \x1b[33m↑\x1b[0m {}",
                t!(
                    "cmd_setup_refreshed",
                    label = agent.title(),
                    path = paths.hook_path().display().to_string()
                )
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        setup: SetupArgs,
    }

    // The registry (`data/agents.json`) says every built-in agent has hooks installed by `atlas setup <setup>`;
    // hostd's integrations check, Atlas Desktop's prompt and the crew install all run that exact command. A
    // built-in added to the registry without a harness here (or a harness without an entry) turns this red.
    #[test]
    fn every_built_in_agent_has_a_setup_command_with_check_and_no_harness_is_unlisted() {
        for def in atlas_core::agents::all() {
            let cli =
                Cli::try_parse_from(["atlas", def.setup.as_str(), "--check"]).unwrap_or_else(|e| {
                    panic!("`atlas setup {} --check` does not parse: {e}", def.setup)
                });
            let ok = match cli.setup.harness {
                Some(Harness::Omp(a)) => (Agent::Omp, a.check),
                Some(Harness::ClaudeCode(a)) => (Agent::ClaudeCode, a.check),
                Some(Harness::Codex(a)) => (Agent::Codex, a.check),
                Some(Harness::OpenCode(a)) => (Agent::OpenCode, a.check),
                None => panic!("{}: no harness", def.setup),
            };
            assert!(ok.1, "{}: --check", def.setup);
            assert_eq!(
                ok.0.id(),
                def.id,
                "{}: harness maps to its registry entry",
                def.setup
            );
        }
        let covered: Vec<&str> = Agent::ALL.iter().map(|a| a.id()).collect();
        let registry: Vec<&str> = atlas_core::agents::all()
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(
            covered, registry,
            "Agent::ALL is the registry, in its order"
        );
    }
}
