//! `atlas desktop`: open Atlas Desktop, installing the release that matches this CLI
//! when it is missing. The app ships as a release asset next to the CLI tarball
//! (`Atlas-<version>-<platform>.app.tar.gz`), downloaded with `gh` like `atlas update`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use super::AtlasCommand;
use super::release::{REPO, download_verified};
use super::self_update::{current_platform, current_version};

const APP: &str = "Atlas.app";

#[derive(Debug, Args)]
pub struct DesktopArgs {
    /// Download and install the app even when one is installed (repairs or switches versions)
    #[arg(long)]
    pub install: bool,

    /// Install the newest published release instead of the one matching this CLI
    #[arg(long, requires = "install")]
    pub latest: bool,

    /// Install from a local `.app` or `.app.tar.gz` instead of downloading
    #[arg(long, value_name = "PATH", conflicts_with = "latest")]
    pub from: Option<PathBuf>,

    /// Install/update only; do not launch the app
    #[arg(long)]
    pub no_open: bool,

    /// Run the app from source with hot reload (`bun run desktop:dev`, inside the atlas repo)
    #[arg(long, conflicts_with_all = ["install", "from", "sync"])]
    pub dev: bool,

    /// Print where the app is installed, its version and whether it is linked, then exit
    #[arg(long, conflicts_with_all = ["install", "from", "dev", "sync"])]
    pub status: bool,

    /// Bring an installed app to this CLI's version (run by `atlas update`); never installs fresh
    #[arg(long, hide = true)]
    pub sync: bool,

    #[command(subcommand)]
    pub debug: Option<Debug>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Debug {
    /// Inspect a running debug build (console, CSP, eval, screenshot) — for agents
    #[command(subcommand)]
    Debug(super::desktop_debug::DebugCmd),
}

impl AtlasCommand for DesktopArgs {
    async fn run(self) -> Result<()> {
        if let Some(Debug::Debug(cmd)) = self.debug {
            return super::desktop_debug::run(cmd);
        }
        if self.dev {
            return dev();
        }
        let installed = installed_app();
        if self.status {
            return status(installed.as_deref());
        }
        if self.sync {
            return sync(installed.as_deref()).await;
        }
        let target = installed.clone().unwrap_or_else(default_location);
        if self.install || self.from.is_some() || installed.is_none() {
            if is_symlink(&target) {
                bail!(
                    "{} is linked to a dev build (atlas link --desktop); run atlas unlink first",
                    target.display()
                );
            }
            // Keeps the downloaded tarball alive until the install is done.
            let mut _download_dir = None;
            let source = match self.from {
                Some(p) => p,
                None => {
                    let version = if self.latest {
                        latest_release()?
                    } else {
                        current_version().to_string()
                    };
                    let (dir, tarball) = download(&version)?;
                    _download_dir = Some(dir);
                    tarball
                }
            };
            install(&source, &target)?;
            println!(
                "\x1b[32m✓\x1b[0m Installed {} ({})",
                target.display(),
                app_version(&target).unwrap_or_default()
            );
        }
        if !self.no_open {
            open(&target)?;
        }
        Ok(())
    }
}

// ── locations ─────────────────────────────────────────────────────────────────

pub(crate) use atlas_core::paths::installed_app;

/// Where a fresh install goes: `/Applications` when writable, else `~/Applications`.
pub(crate) fn default_location() -> PathBuf {
    let sys = Path::new("/Applications");
    let probe = sys.join(format!(".atlas-probe-{}", std::process::id()));
    let writable = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    if writable {
        sys.join(APP)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join("Applications")
            .join(APP)
    }
}

/// Where `atlas link --desktop` keeps the release app it replaced.
pub(crate) fn backup_path() -> PathBuf {
    atlas_core::paths::atlas_home()
        .join("desktop")
        .join("Atlas.app.backup")
}

pub(crate) fn is_symlink(p: &Path) -> bool {
    p.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}

/// `CFBundleShortVersionString` of an installed bundle.
pub(crate) fn app_version(app: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn asset_name(version: &str) -> String {
    format!("Atlas-{version}-{}.app.tar.gz", current_platform())
}

// ── actions ───────────────────────────────────────────────────────────────────

fn status(installed: Option<&Path>) -> Result<()> {
    match installed {
        None => println!("Atlas Desktop: not installed (run: atlas desktop)"),
        Some(app) => {
            let version = app_version(app).unwrap_or_else(|| "?".into());
            println!("Atlas Desktop {version}");
            println!("  path: {}", app.display());
            if is_symlink(app) {
                let to = std::fs::read_link(app).unwrap_or_default();
                println!("  linked → {} (atlas unlink to restore)", to.display());
            }
        }
    }
    println!("  cli:  {}", current_version());
    Ok(())
}

/// After `atlas update`: keep an installed (not linked) app in lockstep with the CLI.
async fn sync(installed: Option<&Path>) -> Result<()> {
    let Some(app) = installed else { return Ok(()) };
    if is_symlink(app) {
        return Ok(());
    }
    let want = current_version();
    if app_version(app).as_deref() == Some(want) {
        return Ok(());
    }
    println!("\x1b[1;34m◆\x1b[0m Updating Atlas Desktop to v{want}...");
    let (_dir, src) = download(want)?;
    install(&src, app)?;
    println!("\x1b[32m✓\x1b[0m Atlas Desktop v{want}");
    Ok(())
}

fn latest_release() -> Result<String> {
    Ok(atlas_core::github::latest_release(REPO)?
        .version()
        .to_string())
}

/// Download the app tarball (+ checksum) of `version` into a temp dir and verify it.
/// The `TempDir` must outlive the install: dropping it deletes the tarball.
fn download(version: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    let asset = asset_name(version);
    let dir = tempfile::Builder::new()
        .prefix(&format!("atlas-desktop-{version}-"))
        .tempdir()?;
    println!("\x1b[1;34m◆\x1b[0m Downloading {asset}...");
    let tarball = download_verified(&format!("v{version}"), &asset, dir.path())?;
    Ok((dir, tarball))
}

/// Put the bundle at `source` (`.app` dir or `.app.tar.gz`) at `target`, replacing any
/// existing app only once the new one is fully in place next to it.
pub(crate) fn install(source: &Path, target: &Path) -> Result<()> {
    let parent = target.parent().context("bad install path")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    let staging = parent.join(".Atlas.app.new");
    let old = parent.join(".Atlas.app.old");
    for p in [&staging, &old] {
        let _ = remove(p);
    }

    let src_app = if source.is_dir() {
        source.to_path_buf()
    } else {
        let x = parent.join(".Atlas.app.extract");
        let _ = std::fs::remove_dir_all(&x);
        std::fs::create_dir_all(&x)?;
        run(Command::new("/usr/bin/tar")
            .arg("xzf")
            .arg(source)
            .arg("-C")
            .arg(&x))?;
        let app = x.join(APP);
        if !app.is_dir() {
            bail!("{} does not contain {APP}", source.display());
        }
        app
    };
    // ditto keeps the bundle's symlinks, xattrs and code signature intact.
    run(Command::new("/usr/bin/ditto").arg(&src_app).arg(&staging))?;
    let _ = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&staging)
        .status();

    if target.symlink_metadata().is_ok() {
        std::fs::rename(target, &old)
            .with_context(|| format!("cannot move {}", target.display()))?;
    }
    if let Err(e) = std::fs::rename(&staging, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e).with_context(|| format!("cannot install {}", target.display()));
    }
    let _ = remove(&old);
    let _ = std::fs::remove_dir_all(parent.join(".Atlas.app.extract"));
    // Refresh Launch Services so Spotlight/Dock pick up the new version.
    let _ = Command::new(
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
    )
    .arg("-f")
    .arg(target)
    .status();
    Ok(())
}

fn remove(p: &Path) -> std::io::Result<()> {
    if is_symlink(p) || p.is_file() {
        std::fs::remove_file(p)
    } else {
        std::fs::remove_dir_all(p)
    }
}

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd
        .output()
        .with_context(|| format!("cannot run {cmd:?}"))?;
    if !out.status.success() {
        bail!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn open(app: &Path) -> Result<()> {
    run(Command::new("/usr/bin/open").arg(app))
}

/// `bun run desktop:dev` from the atlas workspace (hot-reloading frontend + Tauri shell).
fn dev() -> Result<()> {
    let ws = super::link::find_workspace_root()?;
    if !ws.join("apps/desktop/package.json").is_file() {
        bail!("{} is not the atlas repo (no apps/desktop)", ws.display());
    }
    let st = Command::new("bun")
        .args(["run", "desktop:dev"])
        .current_dir(&ws)
        .status()
        .context("bun not found")?;
    if !st.success() {
        bail!("desktop:dev exited with {st}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal bundle: enough for `install` and `app_version`.
    #[cfg(target_os = "macos")]
    fn fake_app(dir: &Path, version: &str) -> PathBuf {
        let app = dir.join(APP);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(
            app.join("Contents/Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#
            ),
        )
        .unwrap();
        app
    }

    // Uses `ditto`, which only exists on macOS (Atlas Desktop is macOS-only).
    #[cfg(target_os = "macos")]
    #[test]
    fn install_from_tarball_then_replace_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        fake_app(&src, "1.0.0");
        let tarball = tmp.path().join("a.app.tar.gz");
        run(Command::new("/usr/bin/tar")
            .arg("czf")
            .arg(&tarball)
            .arg("-C")
            .arg(&src)
            .arg(APP))
        .unwrap();

        let target = tmp.path().join("Applications").join(APP);
        install(&tarball, &target).unwrap();
        assert_eq!(app_version(&target).as_deref(), Some("1.0.0"));

        let src2 = tmp.path().join("src2");
        let newer = fake_app(&src2, "1.1.0");
        install(&newer, &target).unwrap();
        assert_eq!(app_version(&target).as_deref(), Some("1.1.0"));
        let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec![APP.to_string()]);
    }

    #[test]
    fn install_rejects_tarball_without_app() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("x"), "x").unwrap();
        let tarball = tmp.path().join("bad.tar.gz");
        run(Command::new("/usr/bin/tar")
            .arg("czf")
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("x"))
        .unwrap();
        let target = tmp.path().join("Apps").join(APP);
        assert!(install(&tarball, &target).is_err());
        assert!(target.symlink_metadata().is_err());
    }
}
