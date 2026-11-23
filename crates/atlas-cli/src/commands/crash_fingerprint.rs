//! `atlas crash-fingerprint <crash.toml> [--resolved <file>|-]`: the fingerprint of a crash report (#320), for the
//! maintainer who triages one and wants to know whether an issue for it exists. What it hashes, with and without
//! symbols, is in `atlas_core::crash_fingerprint`; `--resolved` takes the output of `scripts/symbols.sh resolve`.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use atlas_i18n::t;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct CrashFingerprintArgs {
    /// A `crash-*.toml` or `crash-native-*.toml` report
    pub report: PathBuf,
    /// Output of `scripts/symbols.sh resolve` for the report (`-` for stdin): keys a native crash by the crashing
    /// function and file instead of the module and offset
    #[arg(long, value_name = "FILE")]
    pub resolved: Option<String>,
}

impl AtlasCommand for CrashFingerprintArgs {
    async fn run(self) -> Result<()> {
        let read = |p: &str| {
            std::fs::read_to_string(p).map_err(|e| {
                anyhow!(t!(
                    "cmd_misc_crash_fingerprint_unreadable",
                    path = p.to_string(),
                    error = e.to_string()
                ))
            })
        };
        let report = self.report.to_string_lossy();
        let text = read(&report)?;
        let resolved = match self.resolved.as_deref() {
            None => None,
            Some("-") => {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                Some(s)
            }
            Some(p) => Some(read(p)?),
        };
        match atlas_core::crash_fingerprint::of_report(&text, resolved.as_deref()) {
            Some(f) => println!("{f}"),
            None => {
                return Err(anyhow!(t!(
                    "cmd_misc_crash_fingerprint_none",
                    path = report.to_string()
                )));
            }
        }
        Ok(())
    }
}
