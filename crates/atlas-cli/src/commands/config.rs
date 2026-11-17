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
    /// Show or set the display language (en, pt-BR); `ATLAS_LANG` overrides it per command
    Lang {
        /// Language to save in ~/.atlas/language (en, pt-BR); omit to show the current one
        #[arg(value_name = "TAG", conflicts_with = "unset")]
        tag: Option<String>,
        /// Remove the saved language: follow the system language
        #[arg(long)]
        unset: bool,
        /// Print JSON (language, source, preference, supported); machine output is never translated
        #[arg(long, conflicts_with_all = ["tag", "unset"])]
        json: bool,
    },
}

/// `atlas config lang`: show, set or unset the display language.
fn lang(tag: Option<String>, unset: bool, json: bool) -> Result<()> {
    use atlas_core::lang;
    use atlas_i18n::t;
    if unset {
        lang::set_preference(None)?;
        println!("{}", t!("config_lang_unset"));
        return Ok(());
    }
    if let Some(tag) = tag {
        let Some(saved) = lang::normalize(&tag) else {
            anyhow::bail!(
                "{}",
                t!(
                    "config_lang_unsupported",
                    tag = tag.as_str(),
                    langs = lang::SUPPORTED.join(", ")
                )
            );
        };
        lang::set_preference(Some(saved))?;
        println!("{}", t!("config_lang_set", lang = saved));
        if let Some(env) = std::env::var(lang::ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            eprintln!(
                "{}",
                t!("config_lang_set_env_warning", value = env.as_str())
            );
        }
        return Ok(());
    }
    let now = lang::resolve_source();
    let saved = lang::preference();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "language": now.tag,
                "source": now.source.as_str(),
                "preference": saved,
                "supported": lang::SUPPORTED,
            })
        );
        return Ok(());
    }
    println!("{}", t!("config_lang_current", lang = now.tag));
    println!("{}", t!("config_lang_source", source = now.source.as_str()));
    match saved {
        Some(saved) => println!("{}", t!("config_lang_saved", lang = saved)),
        None => println!("{}", t!("config_lang_saved_none")),
    }
    println!(
        "{}",
        t!(
            "config_lang_available",
            count = lang::SUPPORTED.len(),
            langs = lang::SUPPORTED.join(", ")
        )
    );
    Ok(())
}

impl ConfigCommands {
    pub fn run(self) -> Result<()> {
        match self {
            Self::Lang { tag, unset, json } => lang(tag, unset, json),
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
