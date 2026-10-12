use std::time::Duration;

use anyhow::Result;
use atlas_core::util::expand_tilde;
use atlas_tui::runner::DownOutcome;
use clap::Args;

use super::AtlasCommand;

/// How long `atlas down` waits for the services to stop.
const WAIT: Duration = Duration::from_secs(15);

#[derive(Debug, Args)]
pub struct DownArgs {
    #[arg(short, long, default_value = ".")]
    pub dir: String,
}

impl AtlasCommand for DownArgs {
    async fn run(self) -> Result<()> {
        let root = std::path::PathBuf::from(expand_tilde(&self.dir));
        match atlas_tui::runner::down(&root, WAIT)
            .await
            .map_err(|e| anyhow::anyhow!(e))?
        {
            DownOutcome::NotRunning => println!("atlas is not running"),
            DownOutcome::Stopped { pid } => println!("atlas stopped (pid {pid})"),
            DownOutcome::Timeout { pid } => {
                eprintln!(
                    "atlas (pid {pid}) is still stopping after {}s; check: atlas dev --status",
                    WAIT.as_secs()
                );
                std::process::exit(1);
            }
        }
        Ok(())
    }
}
