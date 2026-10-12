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
mod plan;
mod shared;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};

use super::AtlasCommand;
use plan::{Plan, SkillsTarget};

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
}

impl Agent {
    const ALL: [Agent; 3] = [Agent::Omp, Agent::ClaudeCode, Agent::Codex];

    /// Subcommand name (`atlas setup <name>`).
    fn name(self) -> &'static str {
        match self {
            Agent::Omp => "omp",
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
        }
    }

    /// Executable looked up on `PATH`.
    fn binary(self) -> &'static str {
        match self {
            Agent::Omp => "omp",
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Agent::Omp => "OMP",
            Agent::ClaudeCode => "Claude Code",
            Agent::Codex => "Codex",
        }
    }

    /// Where this agent's user-scope config lives, for the scope banner.
    fn user_dir(self, env: &Env) -> &Path {
        match self {
            Agent::Omp => &env.agent_dir,
            Agent::ClaudeCode => &env.claude_dir,
            Agent::Codex => &env.codex_home,
        }
    }

    fn project_dir_name(self) -> &'static str {
        match self {
            Agent::Omp => ".omp/",
            Agent::ClaudeCode => ".mcp.json + .claude/",
            Agent::Codex => ".codex/",
        }
    }

    fn next_steps(self) -> [&'static str; 3] {
        match self {
            Agent::Omp => [
                "Restart omp (the hook loads at session start); \x1b[1m/reload-plugins\x1b[0m refreshes skills and MCP in a running session",
                "Verify: \x1b[1m/mcp list\x1b[0m should show `atlas`, \x1b[1momp plugin list\x1b[0m should show `atlas-omp`",
                "In an atlas project, the agent now gets `atlas dev --status` at session start",
            ],
            Agent::ClaudeCode => [
                "Restart Claude Code (project `.mcp.json` servers ask for approval on first use)",
                "Verify: \x1b[1mclaude mcp get atlas\x1b[0m should say Connected, or run \x1b[1m/mcp\x1b[0m",
                "In an atlas project, the agent now gets `atlas dev --status` at session start",
            ],
            Agent::Codex => [
                "Restart Codex, then run \x1b[1m/hooks\x1b[0m once and trust the atlas hooks (Codex skips unreviewed hooks)",
                "Verify: \x1b[1mcodex mcp get atlas\x1b[0m, or \x1b[1m/mcp\x1b[0m in the TUI",
                "In an atlas project, the agent now gets `atlas dev --status` at session start",
            ],
        }
    }

    fn removal_hint(self) -> &'static str {
        match self {
            Agent::Omp => "Restart omp for the removal to take effect.",
            Agent::ClaudeCode => "Restart Claude Code for the removal to take effect.",
            Agent::Codex => "Restart Codex for the removal to take effect.",
        }
    }

    fn resolve(self, env: &Env, scope: Scope, root: Option<&Path>) -> Result<AgentPaths> {
        Ok(match self {
            Agent::Omp => AgentPaths::Omp(omp::Paths::resolve(env, scope, root)?),
            Agent::ClaudeCode => {
                AgentPaths::ClaudeCode(claude_code::Paths::resolve(env, scope, root)?)
            }
            Agent::Codex => AgentPaths::Codex(codex::Paths::resolve(env, scope, root)?),
        })
    }
}

/// Resolved artifact locations for one agent and scope.
enum AgentPaths {
    Omp(omp::Paths),
    ClaudeCode(claude_code::Paths),
    Codex(codex::Paths),
}

impl AgentPaths {
    fn skills(&self) -> &SkillsTarget {
        match self {
            AgentPaths::Omp(p) => &p.skills,
            AgentPaths::ClaudeCode(p) => &p.skills,
            AgentPaths::Codex(p) => &p.skills,
        }
    }

    fn plan_install(&self, scope: Scope, force: bool, command: &str) -> Result<Plan> {
        match self {
            AgentPaths::Omp(p) => omp::plan_install(p, scope, force, command),
            AgentPaths::ClaudeCode(p) => claude_code::plan_install(p, scope, force, command),
            AgentPaths::Codex(p) => codex::plan_install(p, scope, force, command),
        }
    }

    fn plan_uninstall(&self, env: &Env, force: bool) -> Result<Plan> {
        match self {
            AgentPaths::Omp(p) => omp::plan_uninstall(env, p, force),
            AgentPaths::ClaudeCode(p) => claude_code::plan_uninstall(p, force),
            AgentPaths::Codex(p) => codex::plan_uninstall(env, p, force),
        }
    }

    /// Has a previous `atlas setup <agent> --apply` put hooks here? (`atlas update` gate.)
    fn hooks_installed(&self) -> bool {
        match self {
            AgentPaths::Omp(p) => omp::installed(p),
            AgentPaths::ClaudeCode(p) => claude_code::hooks_installed(p),
            AgentPaths::Codex(p) => codex::hooks_installed(p),
        }
    }

    /// Where the refresh message points.
    fn hook_path(&self) -> &Path {
        match self {
            AgentPaths::Omp(p) => &p.plugin_dir,
            AgentPaths::ClaudeCode(p) => &p.settings,
            AgentPaths::Codex(p) => &p.hooks,
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
        let home = PathBuf::from(std::env::var("HOME").context("$HOME not set")?);
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
            home,
            agent_dir,
            claude_dir,
            claude_json,
            codex_home,
            cwd: std::env::current_dir().context("cannot read current directory")?,
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
        }
    }
}

/// `atlas setup` with no harness: report what is installed and what to run.
fn detect() -> Result<()> {
    let env = Env::from_process()?;
    println!("\x1b[1;34m◆\x1b[0m Atlas agent setup");
    println!();
    let mut found = Vec::new();
    for agent in Agent::ALL {
        let pad = " ".repeat(11 - agent.name().len());
        match which(agent.binary()) {
            Some(p) => {
                println!(
                    "  \x1b[32m✓\x1b[0m {}{pad}found at {}",
                    agent.name(),
                    p.display()
                );
                let state = match agent
                    .resolve(&env, Scope::User, None)
                    .and_then(|paths| paths.plan_install(Scope::User, false, "atlas"))
                {
                    Ok(plan) if plan.has_conflict() => "needs attention".to_string(),
                    Ok(plan) if plan.has_changes() => "not set up (or outdated)".to_string(),
                    Ok(_) => "set up, up to date".to_string(),
                    Err(e) => format!("cannot read config: {e:#}"),
                };
                println!("        status: {state}");
                found.push(agent);
            }
            None => println!("  \x1b[2m·\x1b[0m {}{pad}not found on PATH", agent.name()),
        }
    }
    println!();
    if found.is_empty() {
        println!(
            "  No supported agent found. Install one of omp, claude (Claude Code) or codex, then run \x1b[1matlas setup <agent> --apply\x1b[0m."
        );
    } else {
        for agent in found {
            println!("  {}:", agent.title());
            println!("    Preview:  \x1b[1matlas setup {}\x1b[0m", agent.name());
            println!(
                "    Install:  \x1b[1matlas setup {} --apply\x1b[0m",
                agent.name()
            );
        }
    }
    Ok(())
}

fn run_agent(agent: Agent, args: HarnessArgs) -> Result<()> {
    let env = Env::from_process()?;
    let project_root = atlas_tui::config::find_root(&env.cwd);
    let project_root = project_root.map(|r| r.canonicalize().unwrap_or(r));

    if args.refresh {
        return refresh_agent(agent, &env, project_root.as_deref());
    }

    if args.scope == Scope::Project && project_root.is_none() {
        bail!(
            "--scope project needs an atlas.yaml in {} or a parent directory",
            env.cwd.display()
        );
    }
    let paths = agent.resolve(&env, args.scope, project_root.as_deref())?;

    let mut warnings = Vec::new();
    let (mut plan, title) = if args.uninstall {
        (
            paths.plan_uninstall(&env, args.force)?,
            format!("Uninstall atlas from {}", agent.title()),
        )
    } else {
        let command = shared::resolve_command(&env, &mut warnings)?;
        (
            paths.plan_install(args.scope, args.force, &command)?,
            format!("Set up atlas for {}", agent.title()),
        )
    };
    plan.warnings.splice(0..0, warnings);

    let scope_label = match args.scope {
        Scope::User => format!("user ({})", agent.user_dir(&env).display()),
        Scope::Project => format!(
            "project ({}: {})",
            project_root.as_deref().unwrap_or(&env.cwd).display(),
            agent.project_dir_name()
        ),
    };
    println!("\x1b[1;34m◆\x1b[0m {title}  \x1b[2mscope: {scope_label}\x1b[0m");
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
        bail!(
            "conflicts found — nothing was changed. Re-run with --force to replace foreign entries."
        );
    }
    if !plan.has_changes() {
        print!("{}", plan.render(true));
        println!();
        println!(
            "  \x1b[32m✓\x1b[0m {}",
            if args.uninstall {
                "nothing to remove"
            } else {
                "up to date"
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
        println!("  Next steps:");
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
        println!(
            "  \x1b[31m✗\x1b[0m conflicts found; --apply will refuse unless you pass --force."
        );
    } else if plan.has_changes() {
        println!(
            "  Dry run — nothing written. Run \x1b[1matlas setup {} {flag}\x1b[0m to apply.",
            agent.name()
        );
    } else if uninstall {
        println!("  Nothing to remove.");
    } else {
        println!("  \x1b[32m✓\x1b[0m up to date — nothing to do.");
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
                "  \x1b[33m↑\x1b[0m refreshed {} setup ({})",
                agent.title(),
                paths.hook_path().display()
            );
        }
    }
    Ok(())
}
