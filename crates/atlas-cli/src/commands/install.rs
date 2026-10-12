use anyhow::{Context, Result};
use atlas_core::util::expand_tilde;
use clap::Args;
use std::path::{Path, PathBuf};

use super::AtlasCommand;
use crate::commands::skills;

const INSTALL_DIR: &str = ".atlas/bin";

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(long)]
    pub dir: Option<String>,
}

impl AtlasCommand for InstallArgs {
    async fn run(self) -> Result<()> {
        handle(self.dir).await
    }
}

pub async fn handle(dir: Option<String>) -> Result<()> {
    let install_dir = match dir {
        Some(d) => PathBuf::from(expand_tilde(&d)),
        None => {
            let home = std::env::var("HOME").context("$HOME not set")?;
            PathBuf::from(home).join(INSTALL_DIR)
        }
    };

    println!("\x1b[1;34m◆\x1b[0m Atlas installer");
    println!("  Version:  v{}", env!("CARGO_PKG_VERSION"));
    println!("  Target:   {}", install_dir.display());
    println!();

    let current_exe =
        std::env::current_exe().context("Could not determine current executable path")?;

    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("Could not create {}", install_dir.display()))?;

    let dest = install_dir.join("atlas");
    let tmp = install_dir.join(".atlas-install-tmp");
    std::fs::copy(&current_exe, &tmp)
        .with_context(|| format!("Could not copy binary to {}", tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }

    std::fs::rename(&tmp, &dest)
        .with_context(|| format!("Could not install to {}", dest.display()))?;

    println!("\x1b[32m✓\x1b[0m Installed → {}", dest.display());

    let already_in_path = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|p| Path::new(&p) == install_dir.as_path());

    if already_in_path {
        println!("\x1b[32m✓\x1b[0m Already in PATH");
    } else {
        let written = setup_path_all_shells(&install_dir);
        if written.is_empty() {
            println!(
                "\x1b[33m◆\x1b[0m Add to PATH manually:\n  export PATH=\"{}:$PATH\"",
                install_dir.display()
            );
        } else {
            for shell in &written {
                println!("\x1b[32m✓\x1b[0m Added to PATH  [{shell}]");
            }
        }
    }

    println!("\x1b[1;32m✓\x1b[0m Done! Restart your shell or run:");
    println!("  export PATH=\"{}:$PATH\"", install_dir.display());
    println!();

    // Install the skills bundled in this binary into ~/.agents/skills
    println!("\x1b[1;34m◆\x1b[0m Installing skills...");
    if let Err(e) = skills::install::handle(vec![], true) {
        eprintln!("\x1b[33m◆\x1b[0m Skills install failed (non-fatal): {e}");
    }

    println!();
    println!("  \x1b[2mGet started:\x1b[0m");
    println!("  atlas dev              start the TUI");
    println!("  atlas update           keep atlas up to date");
    println!(
        "  atlas setup            wire atlas into omp, Claude Code or Codex (MCP + skills + hooks)"
    );

    Ok(())
}

fn setup_path_all_shells(install_dir: &Path) -> Vec<String> {
    let mut written = Vec::new();
    if shell_exists("fish")
        && let Ok(label) = setup_fish(install_dir)
    {
        written.push(label);
    }
    if shell_exists("zsh")
        && let Ok(label) = setup_zsh(install_dir)
    {
        written.push(label);
    }
    if shell_exists("bash")
        && let Ok(label) = setup_bash(install_dir)
    {
        written.push(label);
    }
    written
}

fn shell_exists(name: &str) -> bool {
    let shell_var = std::env::var("SHELL").unwrap_or_default();
    if std::path::Path::new(&shell_var)
        .file_name()
        .map(|n| n.to_string_lossy())
        .as_deref()
        == Some(name)
    {
        return true;
    }
    which_exists(name)
}

fn which_exists(cmd: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join(cmd).is_file())
}

fn setup_fish(install_dir: &Path) -> Result<String> {
    let conf_d = fish_conf_d()?;
    std::fs::create_dir_all(&conf_d)
        .with_context(|| format!("Could not create {}", conf_d.display()))?;
    let rc = conf_d.join("atlas.fish");
    let line = format!("fish_add_path {}", install_dir.display());
    if rc.exists() {
        let existing = std::fs::read_to_string(&rc).unwrap_or_default();
        if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
            return Ok("fish  (already set)".to_string());
        }
    }
    std::fs::write(&rc, format!("# Atlas CLI\n{line}\n"))
        .with_context(|| format!("Could not write {}", rc.display()))?;
    Ok(format!("fish  {}", rc.display()))
}

fn fish_conf_d() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    Ok(base.join("fish").join("conf.d"))
}

fn setup_zsh(install_dir: &Path) -> Result<String> {
    let rc = zsh_env_file()?;
    let line = format!("export PATH=\"{}:$PATH\"", install_dir.display());
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
        return Ok("zsh   (already set)".to_string());
    }
    append_line(&rc, &line)?;
    Ok(format!("zsh   {}", rc.display()))
}

fn zsh_env_file() -> Result<PathBuf> {
    let zdotdir = std::env::var("ZDOTDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home)
        });
    Ok(zdotdir.join(".zshenv"))
}

fn setup_bash(install_dir: &Path) -> Result<String> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    let home = PathBuf::from(home);
    let candidates = [".bash_profile", ".bash_login", ".bashrc"];
    let rc = candidates
        .iter()
        .map(|f| home.join(f))
        .find(|p| p.is_file());
    let rc = match rc {
        Some(r) => r,
        None => return Ok("bash  (no rc file found)".to_string()),
    };
    let line = format!("export PATH=\"{}:$PATH\"", install_dir.display());
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if existing.contains(install_dir.to_string_lossy().into_owned().as_str()) {
        return Ok("bash  (already set)".to_string());
    }
    append_line(&rc, &line)?;
    Ok(format!("bash  {}", rc.display()))
}

fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Could not open {}", path.display()))?;
    writeln!(f, "\n# Atlas CLI\n{line}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_dir_default() {
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let home = std::env::var("HOME").unwrap();
        let dir = PathBuf::from(home).join(INSTALL_DIR);
        assert_eq!(dir, PathBuf::from("/home/dev/.atlas/bin"));
    }

    #[test]
    fn test_shell_exists_via_path() {
        assert!(which_exists("sh"));
    }

    #[test]
    fn test_fish_conf_d_default() {
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let dir = fish_conf_d().unwrap();
        assert_eq!(dir, PathBuf::from("/home/dev/.config/fish/conf.d"));
    }

    #[test]
    fn test_zsh_env_file_default() {
        unsafe { std::env::remove_var("ZDOTDIR") };
        unsafe { std::env::set_var("HOME", "/home/dev") };
        let f = zsh_env_file().unwrap();
        assert_eq!(f, PathBuf::from("/home/dev/.zshenv"));
    }
}
