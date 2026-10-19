use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

impl AtlasCommand for UninstallArgs {
    async fn run(self) -> Result<()> {
        handle(self.yes).await
    }
}

pub async fn handle(yes: bool) -> Result<()> {
    let home = dirs::home_dir().context("no home directory")?;
    let atlas_dir = home.join(".atlas");
    let bin = atlas_dir.join("bin").join(atlas_core::paths::exe("atlas"));

    println!("\x1b[1;31m◆\x1b[0m Atlas uninstaller");
    println!();

    if !atlas_dir.exists() && !bin.exists() {
        println!("  Nothing to remove — atlas is not installed.");
        return Ok(());
    }

    println!("  This will remove:");
    if atlas_dir.exists() {
        println!("    {}", atlas_dir.display());
    }

    // Detect shell configs that reference atlas
    let shell_files = shell_config_files(&home);
    #[cfg(windows)]
    println!("    {} (user PATH entry)", atlas_dir.join("bin").display());
    let dirty: Vec<&PathBuf> = shell_files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .contains(".atlas")
        })
        .collect();

    for f in &dirty {
        println!("    {} (PATH entry)", f.display());
    }

    println!();

    if !yes {
        eprint!("  Continue? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("  Aborted.");
            return Ok(());
        }
        println!();
    }

    // Windows cannot delete a running executable (this one, or `atlas hostd`), but it can move
    // it: out of the way to the temp dir, where nothing refers to it.
    #[cfg(windows)]
    if bin.exists() {
        let aside =
            std::env::temp_dir().join(format!("atlas-uninstalled-{}.exe", std::process::id()));
        let _ = std::fs::rename(&bin, aside);
    }

    // Remove ~/.atlas
    if atlas_dir.exists() {
        std::fs::remove_dir_all(&atlas_dir).with_context(|| {
            format!(
                "Could not remove {} (close Atlas Desktop and running `atlas` commands, then retry)",
                atlas_dir.display()
            )
        })?;
        println!("\x1b[32m✓\x1b[0m Removed  {}", atlas_dir.display());
    }

    #[cfg(windows)]
    match super::install::remove_user_path(&atlas_dir.join("bin")) {
        Ok(true) => println!("\x1b[32m✓\x1b[0m Removed  user PATH entry"),
        Ok(false) => {}
        Err(e) => eprintln!("\x1b[33m◆\x1b[0m Could not edit the user PATH: {e:#}"),
    }

    // Strip PATH lines from shell configs
    for path in &dirty {
        strip_atlas_lines(path)?;
        println!("\x1b[32m✓\x1b[0m Cleaned  {}", path.display());
    }

    // Remove fish conf.d file
    let fish_conf = PathBuf::from(&home)
        .join(".config")
        .join("fish")
        .join("conf.d")
        .join("atlas.fish");
    if fish_conf.exists() {
        std::fs::remove_file(&fish_conf)?;
        println!("\x1b[32m✓\x1b[0m Removed  {}", fish_conf.display());
    }

    // XDG fish conf.d
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let xdg_fish = PathBuf::from(xdg)
            .join("fish")
            .join("conf.d")
            .join("atlas.fish");
        if xdg_fish.exists() && xdg_fish != fish_conf {
            std::fs::remove_file(&xdg_fish)?;
            println!("\x1b[32m✓\x1b[0m Removed  {}", xdg_fish.display());
        }
    }

    println!();
    println!("  Atlas has been removed. Restart your shell.");

    Ok(())
}

fn shell_config_files(home: &std::path::Path) -> Vec<PathBuf> {
    [
        ".zshenv",
        ".zshrc",
        ".bash_profile",
        ".bash_login",
        ".bashrc",
        ".profile",
    ]
    .iter()
    .map(|f| home.join(f))
    .filter(|p| p.exists())
    .collect()
}

/// Remove all lines containing ".atlas" from a shell config file.
fn strip_atlas_lines(path: &PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;

    let filtered: String = content
        .lines()
        .filter(|line| !line.contains(".atlas") && !line.contains("# Atlas CLI"))
        .map(|line| format!("{line}\n"))
        .collect();

    // Trim trailing blank lines added by the filter
    let trimmed = filtered.trim_end().to_string() + "\n";

    std::fs::write(path, trimmed).with_context(|| format!("Could not write {}", path.display()))?;

    Ok(())
}
