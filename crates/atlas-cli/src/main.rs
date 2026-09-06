use clap::{Parser, Subcommand};

mod commands;

#[derive(Debug, Parser)]
#[command(name = "atlas", about = "Atlas — TUI dev environment orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the development environment TUI
    Dev {
        /// Project root directory (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        dir: String,
        /// Run without TUI (headless mode for CI/scripts)
        #[arg(long)]
        headless: bool,
    },

    /// Initialize atlas.yaml for the current project
    Init {
        /// Project root directory (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        dir: String,
    },

    /// Update atlas to the latest version
    SelfUpdate {
        /// Only check for updates without installing
        #[arg(long)]
        check: bool,
        /// Update channel (stable, beta, nightly)
        #[arg(long)]
        channel: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Dev { dir, headless } => {
            let root = std::path::PathBuf::from(shellexpand(&dir));
            if headless {
                atlas_tui::run_headless(&root).await?;
            } else {
                atlas_tui::run(&root).await?;
            }
        }
        Commands::Init { dir } => {
            commands::init::handle(&dir).await?;
        }
        Commands::SelfUpdate { check, channel } => {
            commands::self_update::handle(check, channel).await?;
        }
    }

    Ok(())
}

fn shellexpand(path: &str) -> String {
    if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}{}", home, &path[1..])
    } else {
        path.to_string()
    }
}
