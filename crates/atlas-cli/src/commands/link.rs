use anyhow::{Context, Result, bail};
use atlas_core::util::expand_tilde;
use clap::Args;
use std::path::{Path, PathBuf};

use super::AtlasCommand;
const INSTALL_DIR: &str = ".atlas/bin";

#[derive(Debug, Args)]
pub struct LinkArgs {
    /// Path to the atlas workspace root (default: auto-detected from cwd)
    #[arg(long)]
    pub dir: Option<String>,

    /// Skip `cargo build` before linking (use the existing debug binary as-is)
    #[arg(long = "no-build", action = clap::ArgAction::SetTrue)]
    pub no_build: bool,

    /// Also link Atlas Desktop: build a debug `Atlas.app` and point /Applications/Atlas.app at it
    #[arg(long)]
    pub desktop: bool,
}

#[derive(Debug, Args)]
pub struct UnlinkArgs {}

impl AtlasCommand for LinkArgs {
    async fn run(self) -> Result<()> {
        link(self.dir, !self.no_build, self.desktop).await
    }
}

impl AtlasCommand for UnlinkArgs {
    async fn run(self) -> Result<()> {
        unlink().await
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn install_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home).join(INSTALL_DIR).join("atlas"))
}

/// Walk up from `cwd` until we find a `Cargo.toml` that contains `[workspace]`.
pub(crate) fn find_workspace_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("cannot read cwd")?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
            if content.contains("[workspace]") {
                return Ok(dir);
            }
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => bail!(
                "No Cargo workspace found. Run from inside the atlas repo, \
                 or pass --dir <workspace-root>."
            ),
        }
    }
}

fn debug_binary(workspace: &Path) -> PathBuf {
    workspace.join("target").join("debug").join("atlas")
}

// ── link ──────────────────────────────────────────────────────────────────────

pub async fn link(dir: Option<String>, build: bool, desktop: bool) -> Result<()> {
    let workspace = match dir {
        Some(d) => PathBuf::from(expand_tilde(&d)),
        None => find_workspace_root()?,
    };

    println!("\x1b[1;34m◆\x1b[0m atlas link  —  dev mode");
    println!("  Workspace: {}", workspace.display());

    // Optionally build first so the binary is fresh.
    if build {
        println!("\n\x1b[2m$ cargo build -p atlas-cli\x1b[0m");
        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "atlas-cli"])
            .current_dir(&workspace)
            .status()
            .context("failed to run cargo")?;
        if !status.success() {
            bail!("cargo build failed");
        }
    }

    let debug_bin = debug_binary(&workspace);
    if !debug_bin.exists() {
        bail!(
            "Debug binary not found at {}.\n  Run `cargo build -p atlas-cli` first, \
             or use --build (default).",
            debug_bin.display()
        );
    }

    let installed = install_path()?;

    // Back up the current binary so `atlas unlink` can restore it.
    let backup = installed.with_extension("backup");
    if installed.exists() {
        // Don't overwrite an existing real backup with another symlink.
        let is_symlink = installed
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if !is_symlink {
            std::fs::copy(&installed, &backup)
                .with_context(|| format!("cannot back up {}", installed.display()))?;
            println!("\x1b[32m✓\x1b[0m Backed up  → {}", backup.display());
        }
        std::fs::remove_file(&installed)
            .with_context(|| format!("cannot remove {}", installed.display()))?;
    }

    // Create symlink: ~/.atlas/bin/atlas → <workspace>/target/debug/atlas
    std::os::unix::fs::symlink(&debug_bin, &installed)
        .with_context(|| format!("cannot create symlink at {}", installed.display()))?;

    println!(
        "\x1b[32m✓\x1b[0m Linked     {} → {}",
        installed.display(),
        debug_bin.display()
    );
    println!();
    println!("  \x1b[2mThe installed `atlas` now runs your local debug build.\x1b[0m");
    println!(
        "  Rebuild anytime with \x1b[1mcargo build -p atlas-cli\x1b[0m — no reinstall needed."
    );
    println!("  Restore the release binary with \x1b[1matlas unlink\x1b[0m.");

    if desktop {
        link_desktop(&workspace, build)?;
    }
    Ok(())
}

/// The debug bundle `cargo tauri build --debug` produces.
fn debug_app(workspace: &Path) -> PathBuf {
    workspace.join("target/debug/bundle/macos/Atlas.app")
}

/// Point the installed `Atlas.app` at the workspace's debug bundle, moving a real
/// install aside (to `~/.atlas/desktop/Atlas.app.backup`) for `atlas unlink`.
fn link_desktop(workspace: &Path, build: bool) -> Result<()> {
    use super::desktop;
    if build {
        let setup = workspace.join("scripts/setup.sh");
        let st = std::process::Command::new(&setup)
            .arg("--quiet")
            .current_dir(workspace)
            .status()
            .with_context(|| format!("cannot run {}", setup.display()))?;
        if !st.success() {
            bail!("setup failed");
        }
        println!("\n\x1b[2m$ cargo tauri build --debug --bundles app\x1b[0m");
        let st = std::process::Command::new("cargo")
            .args(["tauri", "build", "--debug", "--bundles", "app"])
            .current_dir(workspace.join("crates/atlas-desktop"))
            .status()
            .context("failed to run cargo tauri (install: cargo install tauri-cli)")?;
        if !st.success() {
            bail!("desktop build failed");
        }
    }
    let app = debug_app(workspace);
    if !app.is_dir() {
        bail!("{} not found; drop --no-build", app.display());
    }

    let target = desktop::installed_app().unwrap_or_else(desktop::default_location);
    if desktop::is_symlink(&target) {
        std::fs::remove_file(&target)?;
    } else if target.exists() {
        let backup = desktop::backup_path();
        if let Some(dir) = backup.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let _ = std::fs::remove_dir_all(&backup);
        // Different volume than /Applications is possible: ditto, then delete.
        let st = std::process::Command::new("/usr/bin/ditto")
            .arg(&target)
            .arg(&backup)
            .status()?;
        if !st.success() {
            bail!("cannot back up {}", target.display());
        }
        std::fs::remove_dir_all(&target)?;
        println!("\x1b[32m✓\x1b[0m Backed up  → {}", backup.display());
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::os::unix::fs::symlink(&app, &target)
        .with_context(|| format!("cannot create symlink at {}", target.display()))?;
    println!(
        "\x1b[32m✓\x1b[0m Linked     {} → {}",
        target.display(),
        app.display()
    );
    println!(
        "  \x1b[2mRebuild with\x1b[0m \x1b[1matlas link --desktop\x1b[0m\x1b[2m, or iterate live with\x1b[0m \x1b[1matlas desktop --dev\x1b[0m\x1b[2m.\x1b[0m"
    );
    Ok(())
}

/// Undo `link_desktop`; no-op when the app is not linked.
fn unlink_desktop() -> Result<()> {
    use super::desktop;
    let Some(target) = desktop::installed_app().filter(|p| desktop::is_symlink(p)) else {
        return Ok(());
    };
    std::fs::remove_file(&target)?;
    let backup = desktop::backup_path();
    if backup.is_dir() {
        let st = std::process::Command::new("/usr/bin/ditto")
            .arg(&backup)
            .arg(&target)
            .status()?;
        if !st.success() {
            bail!("cannot restore {}", backup.display());
        }
        std::fs::remove_dir_all(&backup)?;
        println!("\x1b[32m✓\x1b[0m Unlinked — restored {}", target.display());
    } else {
        println!(
            "\x1b[33m◆\x1b[0m Desktop link removed; no backup to restore. Install it with \x1b[1matlas desktop\x1b[0m."
        );
    }
    Ok(())
}

// ── unlink ────────────────────────────────────────────────────────────────────

pub async fn unlink() -> Result<()> {
    unlink_desktop()?;
    let installed = install_path()?;
    let backup = installed.with_extension("backup");

    let is_symlink = installed
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    if !is_symlink {
        println!("\x1b[33m◆\x1b[0m atlas is not currently linked to a dev build — nothing to do.");
        return Ok(());
    }

    std::fs::remove_file(&installed)
        .with_context(|| format!("cannot remove symlink {}", installed.display()))?;

    if backup.exists() {
        std::fs::rename(&backup, &installed)
            .with_context(|| format!("cannot restore {}", backup.display()))?;
        println!(
            "\x1b[32m✓\x1b[0m Unlinked — restored release binary at {}",
            installed.display()
        );
    } else {
        println!(
            "\x1b[33m◆\x1b[0m Symlink removed but no backup found at {}.",
            backup.display()
        );
        println!("  Re-run \x1b[1matlas install\x1b[0m to reinstall from the latest release.");
    }

    Ok(())
}
