use anyhow::Result;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct VersionArgs;

impl AtlasCommand for VersionArgs {
    async fn run(self) -> Result<()> {
        println!(
            "atlas {}\n  os:   {}\n  arch: {}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        Ok(())
    }
}
