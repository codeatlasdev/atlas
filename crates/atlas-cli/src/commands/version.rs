use anyhow::Result;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct VersionArgs;

impl AtlasCommand for VersionArgs {
    async fn run(self) -> Result<()> {
        println!(
            "{}",
            atlas_core::buildinfo::render(
                atlas_core::buildinfo::current(),
                std::env::consts::OS,
                std::env::consts::ARCH,
            )
        );
        Ok(())
    }
}
