use anyhow::{Result, bail};
use atlas_core::util::expand_tilde;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct McpArgs {
    /// Project directory containing atlas.yaml. Default: the nearest ancestor of
    /// the current directory that has one (falls back to the current directory).
    #[arg(short, long)]
    pub dir: Option<String>,
}

impl AtlasCommand for McpArgs {
    /// Serve MCP over stdio. stdout is reserved for protocol messages.
    async fn run(self) -> Result<()> {
        let root = match &self.dir {
            Some(d) => std::path::PathBuf::from(expand_tilde(d)),
            None => {
                let cwd = std::env::current_dir()?;
                atlas_tui::config::find_root(&cwd).unwrap_or(cwd)
            }
        };
        let root = root.canonicalize().unwrap_or(root);
        if !root.is_dir() {
            bail!("not a directory: {}", root.display());
        }
        eprintln!("atlas mcp: serving {} on stdio", root.display());
        atlas_tui::mcp::serve(&root).await
    }
}
