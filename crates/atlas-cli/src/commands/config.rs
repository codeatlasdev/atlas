use std::path::PathBuf;

use anyhow::Result;
use atlas_core::util::expand_tilde;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Validate atlas.yaml (+ atlas.local.yaml); prints OK or every error, exit 1 on errors
    Check {
        #[arg(short, long, default_value = ".")]
        dir: String,
    },
    /// Print the JSON Schema for atlas.yaml to stdout
    Schema,
}

impl ConfigCommands {
    pub fn run(self) -> Result<()> {
        match self {
            Self::Schema => {
                println!("{}", atlas_tui::config::json_schema());
                Ok(())
            }
            Self::Check { dir } => {
                let root = PathBuf::from(expand_tilde(&dir));
                let (config, errors) = atlas_tui::config::load_checked(&root)?;
                if errors.is_empty() {
                    for w in atlas_tui::config::warnings(&config) {
                        eprintln!("warning: {w}");
                    }
                    println!("OK: {} is valid", root.join("atlas.yaml").display());
                    if config.port_offset > 0 {
                        let mut ports: Vec<_> = config
                            .services
                            .iter()
                            .filter_map(|(n, s)| Some((n, s.port?)))
                            .collect();
                        ports.sort();
                        println!("ports (+{}):", config.port_offset);
                        for (name, port) in ports {
                            println!("  {name}: {port}");
                        }
                    }
                    return Ok(());
                }
                eprintln!(
                    "{}",
                    atlas_tui::config::render_errors(&errors, "atlas.yaml")
                );
                std::process::exit(1);
            }
        }
    }
}
