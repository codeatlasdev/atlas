use std::io::IsTerminal;

use anyhow::Result;
use atlas_core::util::expand_tilde;
use clap::Args;

use super::{AtlasCommand, update_prompt};

#[derive(Debug, Args)]
pub struct DevArgs {
    /// Start only these services plus the ones they `depends_on` (default: all).
    #[arg(value_name = "SERVICE")]
    pub services: Vec<String>,
    /// Start the services of a `profiles:` entry from atlas.yaml (repeatable, union).
    #[arg(long = "profile", value_name = "NAME")]
    pub profiles: Vec<String>,
    /// Do not pull in `depends_on` dependencies of the selected services.
    #[arg(long)]
    pub no_deps: bool,
    #[arg(short, long, default_value = ".")]
    pub dir: String,
    /// Run without TUI — stream logs to stdout, stop on Ctrl-C.
    #[arg(long)]
    pub headless: bool,
    /// Start the services in a background runner and exit; `atlas dev` attaches, `atlas down` stops.
    #[arg(short = 'D', long)]
    pub detach: bool,
    /// The daemonized runner process itself (spawned by --detach; not for direct use).
    #[arg(long, hide = true)]
    pub runner: bool,
    /// CI mode: start all services, wait for health, print JSON result, exit 0/1.
    #[arg(long)]
    pub ci: bool,
    /// Seconds to wait for all services to become healthy in --ci mode.
    #[arg(long, default_value = "60")]
    pub timeout: u64,
    /// Print service status from a running atlas and exit (0 = running, 3 = not running).
    #[arg(long)]
    pub status: bool,
}

impl AtlasCommand for DevArgs {
    async fn run(self) -> Result<()> {
        let root = std::path::PathBuf::from(expand_tilde(&self.dir));
        if self.status {
            match atlas_tui::status::live_status(&root) {
                Ok(table) => {
                    print!("{table}");
                    return Ok(());
                }
                Err(reason) => {
                    println!("atlas dev not running ({reason})");
                    std::process::exit(3);
                }
            }
        }
        let select = atlas_tui::SelectionRequest {
            services: self.services,
            profiles: self.profiles,
            no_deps: self.no_deps,
        };
        if self.detach {
            match atlas_tui::detach(&root, &select).await? {
                atlas_tui::runner::Detached::Started { pid } => println!(
                    "atlas runner started (pid {pid}) · attach: atlas dev · stop: atlas down"
                ),
                atlas_tui::runner::Detached::AlreadyRunning { pid } => println!(
                    "atlas runner already running (pid {pid}) · attach: atlas dev · stop: atlas down"
                ),
            }
            Ok(())
        } else if self.runner {
            atlas_tui::run_runner(&root, &select).await
        } else if self.ci {
            atlas_tui::run_ci(&root, self.timeout, &select).await
        } else if self.headless {
            atlas_tui::run_headless(&root, &select).await
        } else {
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                eprintln!(
                    "atlas dev needs an interactive terminal. Agents/scripts: use 'atlas dev --status' or 'atlas dev --ci'."
                );
                std::process::exit(2);
            }
            // Offer a pending update before the TUI takes the terminal.
            if update_prompt::preflight(true).await == update_prompt::Flow::Abort {
                return Ok(());
            }
            atlas_tui::run(&root, &select).await
        }
    }
}
