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
    let home = dirs::home_dir().context("no home directory")?;
    Ok(home.join(INSTALL_DIR).join(atlas_core::paths::exe("atlas")))
}

/// A file symlink. Windows allows it with Developer Mode on (or as administrator).
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("{e} (Windows needs Developer Mode on to create symlinks)"),
        )
    })
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
    workspace
        .join("target")
        .join("debug")
        .join(atlas_core::paths::exe("atlas"))
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

    // Back up the release binary (so `atlas unlink` can restore it) and swap the link in.
    if let Some(backup) = link_binary(&installed, &debug_bin)? {
        println!("\x1b[32m✓\x1b[0m Backed up  → {}", backup.display());
    }

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
    if cfg!(not(target_os = "macos")) {
        bail!(
            "atlas link --desktop links the macOS app bundle; on this OS run the debug app directly"
        );
    }
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
        println!(
            "\n\x1b[2m$ cargo tauri build --debug --bundles app --config tauri.dev.conf.json\x1b[0m"
        );
        let st = std::process::Command::new("cargo")
            // tauri.dev.conf.json: the orange Atlas Dev icon and id, so a linked build never looks like a release.
            .args([
                "tauri",
                "build",
                "--debug",
                "--bundles",
                "app",
                "--config",
                "tauri.dev.conf.json",
            ])
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
    // Set when a real install was moved to the backup, so a failed link can put it back.
    let mut moved_to_backup = None;
    if desktop::is_symlink(&target) {
        std::fs::remove_file(&target)?;
    } else if target.exists() {
        let backup = desktop::backup_path();
        if let Some(dir) = backup.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // The new backup is made beside the old one and swapped in, so a failed copy keeps the
        // release app an earlier link saved.
        let staged = sibling_tmp(&backup, "new");
        let _ = std::fs::remove_dir_all(&staged);
        // Different volume than /Applications is possible: ditto, then delete.
        let st = std::process::Command::new("/usr/bin/ditto")
            .arg(&target)
            .arg(&staged)
            .status()?;
        if !st.success() {
            let _ = std::fs::remove_dir_all(&staged);
            bail!("cannot back up {}", target.display());
        }
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::rename(&staged, &backup)
            .with_context(|| format!("cannot save {}", backup.display()))?;
        std::fs::remove_dir_all(&target)?;
        println!("\x1b[32m✓\x1b[0m Backed up  → {}", backup.display());
        moved_to_backup = Some(backup);
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if let Err(e) = symlink(&app, &target) {
        // Never leave the machine without an Atlas app because the link could not be made.
        if let Some(backup) = moved_to_backup {
            let _ = std::process::Command::new("/usr/bin/ditto")
                .arg(&backup)
                .arg(&target)
                .status();
        }
        return Err(e).with_context(|| format!("cannot create symlink at {}", target.display()));
    }
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

/// Undo `link_desktop`; no-op when the app is not linked. The backup is restored next to the link
/// first and swapped in by a rename, so a failed restore leaves the link (a working dev app) as it was.
fn unlink_desktop() -> Result<()> {
    use super::desktop;
    let Some(target) = desktop::installed_app().filter(|p| desktop::is_symlink(p)) else {
        return Ok(());
    };
    let backup = desktop::backup_path();
    if backup.is_dir() {
        let staged = sibling_tmp(&target, "restore");
        let _ = std::fs::remove_dir_all(&staged);
        let st = std::process::Command::new("/usr/bin/ditto")
            .arg(&backup)
            .arg(&staged)
            .status()?;
        if !st.success() {
            let _ = std::fs::remove_dir_all(&staged);
            bail!("cannot restore {}", backup.display());
        }
        let swapped = std::fs::remove_file(&target)
            .and_then(|()| std::fs::rename(&staged, &target))
            .with_context(|| format!("cannot replace {}", target.display()));
        if let Err(e) = swapped {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(e);
        }
        std::fs::remove_dir_all(&backup)?;
        println!("\x1b[32m✓\x1b[0m Unlinked — restored {}", target.display());
    } else {
        std::fs::remove_file(&target)?;
        println!(
            "\x1b[33m◆\x1b[0m Desktop link removed; no backup to restore. Install it with \x1b[1matlas desktop\x1b[0m."
        );
    }
    Ok(())
}

// ── unlink ────────────────────────────────────────────────────────────────────

pub async fn unlink() -> Result<()> {
    // The CLI and the app are independent: a desktop failure must not leave the CLI linked.
    let desktop = unlink_desktop();
    let installed = install_path()?;
    match unlink_binary(&installed)? {
        Unlinked::NotLinked => println!(
            "\x1b[33m◆\x1b[0m atlas is not currently linked to a dev build — nothing to do."
        ),
        Unlinked::Restored => println!(
            "\x1b[32m✓\x1b[0m Unlinked — restored release binary at {}",
            installed.display()
        ),
        Unlinked::KeptAsCopy => {
            println!(
                "\x1b[33m◆\x1b[0m Unlinked, but there is no release backup at {}.",
                backup_path(&installed).display()
            );
            println!(
                "  {} is now a plain copy of the dev build, so `atlas` keeps working; run \x1b[1matlas update\x1b[0m to get a release.",
                installed.display()
            );
        }
        Unlinked::Removed => {
            println!(
                "\x1b[33m◆\x1b[0m Symlink removed; it pointed at a build that is gone and there is no backup."
            );
            println!(
                "  Reinstall: \x1b[1mcurl -fsSL https://atlas.codeatlas.com.br/install.sh | sh\x1b[0m"
            );
        }
    }
    desktop
}

/// Where `link` keeps the release binary it replaced: `atlas.backup` (`atlas.exe` → `atlas.backup`).
fn backup_path(installed: &Path) -> PathBuf {
    installed.with_extension("backup")
}

fn is_link(p: &Path) -> bool {
    p.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// A sibling name for a file being swapped into `dest`, so the swap is one atomic rename.
fn sibling_tmp(dest: &Path, tag: &str) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{tag}-{}", std::process::id()));
    dest.with_file_name(name)
}

/// Point `installed` at `debug_bin`. The release binary is copied to `atlas.backup` first (never
/// over a symlink, so a second `link` keeps the real backup), and the link replaces `installed`
/// with one rename: at no point is there no `atlas`, and when the link cannot be created
/// (Windows without Developer Mode, a read-only folder) the release binary is exactly where it was.
/// A dangling link from a cleaned build is replaced like any other.
fn link_binary(installed: &Path, debug_bin: &Path) -> Result<Option<PathBuf>> {
    if let Some(dir) = installed.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let backup = backup_path(installed);
    let backed_up = if installed.exists() && !is_link(installed) {
        std::fs::copy(installed, &backup)
            .with_context(|| format!("cannot back up {}", installed.display()))?;
        Some(backup)
    } else {
        None
    };
    let tmp = sibling_tmp(installed, "link");
    let _ = std::fs::remove_file(&tmp);
    symlink(debug_bin, &tmp)
        .with_context(|| format!("cannot create symlink at {}", installed.display()))?;
    if let Err(e) = std::fs::rename(&tmp, installed) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("cannot replace {}", installed.display()));
    }
    Ok(backed_up)
}

/// What [`unlink_binary`] did.
#[derive(Debug, PartialEq, Eq)]
enum Unlinked {
    /// `installed` was not a symlink.
    NotLinked,
    /// The release backup is back.
    Restored,
    /// No backup: the link was replaced by a copy of the dev build so `atlas` still runs.
    KeptAsCopy,
    /// No backup and the link pointed at nothing: removed.
    Removed,
}

/// Undo [`link_binary`]. The backup is renamed over the link (one step); without one the link
/// becomes a copy of what it pointed at, because removing it would leave no `atlas` to run
/// `atlas install` with.
fn unlink_binary(installed: &Path) -> Result<Unlinked> {
    if !is_link(installed) {
        return Ok(Unlinked::NotLinked);
    }
    let backup = backup_path(installed);
    if backup.is_file() {
        std::fs::rename(&backup, installed)
            .with_context(|| format!("cannot restore {}", backup.display()))?;
        return Ok(Unlinked::Restored);
    }
    if installed.is_file() {
        // `is_file` follows the link: the dev build is still there.
        let tmp = sibling_tmp(installed, "unlink");
        let copied = std::fs::copy(installed, &tmp)
            .with_context(|| format!("cannot copy {}", installed.display()))
            .and_then(|_| {
                std::fs::rename(&tmp, installed)
                    .with_context(|| format!("cannot replace {}", installed.display()))
            });
        if copied.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        copied?;
        return Ok(Unlinked::KeptAsCopy);
    }
    std::fs::remove_file(installed)
        .with_context(|| format!("cannot remove symlink {}", installed.display()))?;
    Ok(Unlinked::Removed)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// `<tmp>/bin/atlas` (a release binary) and `<tmp>/target/debug/atlas` (the dev build).
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let installed = t.path().join("bin/atlas");
        let dev = t.path().join("target/debug/atlas");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::create_dir_all(dev.parent().unwrap()).unwrap();
        std::fs::write(&installed, "release").unwrap();
        std::fs::write(&dev, "dev").unwrap();
        (t, installed, dev)
    }

    #[test]
    fn link_backs_up_the_release_and_unlink_restores_it() {
        let (t, installed, dev) = setup();
        let backup = link_binary(&installed, &dev).unwrap();
        assert_eq!(backup, Some(backup_path(&installed)));
        assert_eq!(std::fs::read_link(&installed).unwrap(), dev);
        assert_eq!(
            std::fs::read_to_string(backup_path(&installed)).unwrap(),
            "release"
        );

        assert_eq!(unlink_binary(&installed).unwrap(), Unlinked::Restored);
        assert!(!is_link(&installed));
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "release");
        assert_eq!(
            names(installed.parent().unwrap()),
            ["atlas"],
            "no backup or temp left"
        );
        assert_eq!(
            std::fs::read_to_string(&dev).unwrap(),
            "dev",
            "the dev build is untouched"
        );
        drop(t);
    }

    /// Incident class: the old `link` removed the release binary and only then created the symlink,
    /// so a symlink that could not be created (read-only folder, Windows without Developer Mode)
    /// left no `atlas` at all.
    #[test]
    fn a_link_that_cannot_be_created_leaves_the_release_binary_in_place() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root ignores the mode: nothing to demonstrate
        }
        let (_t, installed, dev) = setup();
        let bin = installed.parent().unwrap();
        // The backup copy needs write access; a read-only folder refuses it first, and either
        // way `atlas` must still be the release binary.
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o555)).unwrap();
        let r = link_binary(&installed, &dev);
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(r.is_err());
        assert!(!is_link(&installed));
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "release");
        assert_eq!(names(bin), ["atlas"], "no temp link left behind");
    }

    /// Incident class: after `cargo clean` the symlink dangles; `exists()` follows it, so the old
    /// code skipped the removal and `symlink` failed with "File exists": no way to relink.
    #[test]
    fn relinking_over_a_dangling_link_works() {
        let (_t, installed, dev) = setup();
        link_binary(&installed, &dev).unwrap();
        std::fs::remove_file(&dev).unwrap(); // cargo clean
        assert!(
            installed.symlink_metadata().is_ok() && !installed.exists(),
            "dangling"
        );
        std::fs::write(&dev, "rebuilt").unwrap();
        let again = link_binary(&installed, &dev).unwrap();
        assert_eq!(
            again, None,
            "a link is never backed up over the real backup"
        );
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "rebuilt");
        assert_eq!(
            std::fs::read_to_string(backup_path(&installed)).unwrap(),
            "release",
            "the real backup survives a second link"
        );
    }

    /// Incident class: `unlink` with no backup removed the symlink and told the user to run
    /// `atlas install`, a command that no longer existed.
    #[test]
    fn unlink_without_a_backup_keeps_a_working_atlas() {
        let (_t, installed, dev) = setup();
        std::fs::remove_file(&installed).unwrap(); // linked on a machine that never had a release
        link_binary(&installed, &dev).unwrap();
        assert!(!backup_path(&installed).exists());
        assert_eq!(unlink_binary(&installed).unwrap(), Unlinked::KeptAsCopy);
        assert!(!is_link(&installed));
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "dev");
        assert_eq!(names(installed.parent().unwrap()), ["atlas"]);
    }

    #[test]
    fn unlink_of_a_dangling_link_without_a_backup_removes_it() {
        let (_t, installed, dev) = setup();
        std::fs::remove_file(&installed).unwrap();
        link_binary(&installed, &dev).unwrap();
        std::fs::remove_file(&dev).unwrap();
        assert_eq!(unlink_binary(&installed).unwrap(), Unlinked::Removed);
        assert!(installed.symlink_metadata().is_err());
    }

    #[test]
    fn unlink_when_not_linked_changes_nothing() {
        let (_t, installed, _dev) = setup();
        assert_eq!(unlink_binary(&installed).unwrap(), Unlinked::NotLinked);
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "release");
        let gone = installed.with_file_name("nope");
        assert_eq!(unlink_binary(&gone).unwrap(), Unlinked::NotLinked);
    }
}
