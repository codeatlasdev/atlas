use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

pub mod install;
mod list;
mod status;

#[derive(Debug, Subcommand)]
pub enum SkillsCommands {
    /// List the skills bundled with this atlas binary
    List,

    /// Install one or more skills (installs all if no names given)
    Install {
        /// Skill names to install (e.g. atlas atlas-desktop)
        names: Vec<String>,
        /// Install locally to .agents/ instead of ~/.agents/skills
        #[arg(long)]
        local: bool,
    },

    /// Rewrite installed skills whose content differs from this binary's bundled copy
    Update {
        /// Update local .agents/ skills instead of ~/.agents/skills
        #[arg(long)]
        local: bool,
    },

    /// Show installed skills vs the bundled copy
    Status {
        /// Show local .agents/ skills instead of ~/.agents/skills
        #[arg(long)]
        local: bool,
    },
}

pub fn handle(cmd: SkillsCommands) -> Result<()> {
    match cmd {
        SkillsCommands::List => list::handle(),
        SkillsCommands::Install { names, local } => install::handle(names, !local),
        // Skills are embedded in the binary, so update == install (rewrites only on diff).
        SkillsCommands::Update { local } => install::handle(Vec::new(), !local),
        SkillsCommands::Status { local } => status::handle(!local),
    }
}

/// Resolve the skills install directory.
/// global=true  → ~/.agents/skills  (OMP-universal, discovered by omp/claude/codex)
/// global=false → .agents/ in current project dir
pub fn skills_dir(global: bool) -> PathBuf {
    let home = atlas_core::paths::home_dir();
    if global {
        home.join(".agents").join("skills")
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".agents")
    }
}

/// Resolve the manifest file path.
/// global=true  → ~/.atlas/.atlas-skills.json  (atlas private tracking)
/// global=false → .agents/.atlas-skills.json   (project local)
pub fn manifest_path(global: bool) -> PathBuf {
    if global {
        atlas_core::paths::atlas_home().join(".atlas-skills.json")
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".agents")
            .join(".atlas-skills.json")
    }
}

impl super::AtlasCommand for SkillsCommands {
    async fn run(self) -> Result<()> {
        handle(self)
    }
}
