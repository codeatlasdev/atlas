use anyhow::{Result, bail};
use atlas_core::util::expand_tilde;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct McpArgs {
    /// Project directory. Default: the project root of the current directory (nearest
    /// `atlas.yaml`, else nearest `.git` or package manifest, else the directory itself).
    #[arg(short, long)]
    pub dir: Option<String>,

    /// Serve the bot MCP protocol for the given bot ID instead of the project MCP server.
    #[arg(long)]
    pub bot: Option<String>,
}

impl AtlasCommand for McpArgs {
    /// Serve MCP over stdio. stdout is reserved for protocol messages.
    async fn run(self) -> Result<()> {
        if let Some(bot_id) = self.bot {
            return atlas_host::bots::mcp::serve(&bot_id).await;
        }

        let root = match &self.dir {
            Some(d) => std::path::PathBuf::from(expand_tilde(d)),
            None => atlas_core::project_root::resolve(&std::env::current_dir()?),
        };
        let root = root.canonicalize().unwrap_or(root);
        if !root.is_dir() {
            bail!("not a directory: {}", root.display());
        }
        eprintln!("atlas mcp: serving {} on stdio", root.display());
        atlas_tui::mcp::serve(&root).await
    }
}
