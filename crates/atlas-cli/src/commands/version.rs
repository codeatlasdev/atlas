use anyhow::Result;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct VersionArgs;

/// `atlas version` is read by programs, never translated: `atlas install` runs the installed binary and takes the
/// first line (`atlas <version>`), and bug reports quote the rest. `tests/machine_output.rs` compares it byte for
/// byte in every language.
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
