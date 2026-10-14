//! `atlas desktop`: open Atlas Desktop, installing the release that matches this CLI
//! when it is missing. The app ships as a release asset next to the CLI tarball
//! (`Atlas-<version>-<platform>.app.tar.gz` on macOS, `Atlas-<version>-<platform>.AppImage` on
//! Linux, see `desktop_linux.rs`, `Atlas-<version>-<platform>-setup.exe` on Windows, see
//! `desktop_windows.rs`), downloaded and checksum-verified like `atlas update`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use super::AtlasCommand;
use super::release::{REPO, download_verified};
use super::self_update::{current_platform, current_version};
use super::update_core;

#[cfg(target_os = "macos")]
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
    #[arg(long, conflicts_with_all = ["install", "from", "dev", "sync", "agent"])]
    pub status: bool,

    /// Start an isolated instance for an agent (own HOME, hostd and debug socket; never on your desktop), print its
    /// socket and exit. Debug builds only; drive it with `ATLAS_DEBUG_SOCK=<socket> atlas desktop debug …`
    #[arg(long, conflicts_with_all = ["install", "from", "dev", "sync", "latest", "no_open"])]
    pub agent: bool,

    /// With --agent: name the instance (default a1, a2, …); with --stop: the one to stop
    #[arg(long, value_name = "NAME", requires = "agent")]
    pub name: Option<String>,

    /// With --agent: seed the instance's HOME with your desktop settings (never projects, tokens or sockets)
    #[arg(long, requires = "agent")]
    pub scratch: bool,

    /// Stop agent instances: `--agent --stop` stops all of them, `--agent --stop --name a1` that one
    #[arg(long, requires = "agent")]
    pub stop: bool,

    /// Bring an installed app to this CLI's version (run by `atlas update`); never installs fresh
    #[arg(long, hide = true)]
    pub sync: bool,

    #[command(subcommand)]
    pub sub: Option<DesktopSub>,
}

#[derive(Debug, clap::Subcommand)]
pub enum DesktopSub {
    /// Inspect a running debug build (console, CSP, eval, screenshot) — for agents
    #[command(subcommand)]
    Debug(super::desktop_debug::DebugCmd),
    /// Record or screenshot the app window natively, without focusing the app (any build)
    #[command(subcommand)]
    Capture(super::desktop_capture::CaptureCmd),
    /// Run a capture script inside the app and write video + timeline.json
    Record(super::desktop_capture::RecordArgs),
}

impl AtlasCommand for DesktopArgs {
    async fn run(self) -> Result<()> {
        match self.sub {
            Some(DesktopSub::Debug(cmd)) => return super::desktop_debug::run(cmd),
            Some(DesktopSub::Capture(cmd)) => return super::desktop_capture::run(cmd),
            Some(DesktopSub::Record(a)) => return super::desktop_capture::record(a),
            None => {}
        }
        if self.agent {
            return agent(self.stop, self.name, self.scratch);
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
        // Linux: a distro package's app is opened as is; its package manager updates it.
        if installed.is_none()
            && !self.install
            && self.from.is_none()
            && let Some(pkg) = atlas_core::paths::packaged_app()
        {
            if !self.no_open {
                open(&pkg)?;
            }
            return Ok(());
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
            let (source, version) = match self.from {
                Some(p) => (p, None),
                None => {
                    let version = if self.latest {
                        latest_release()?
                    } else {
                        current_version().to_string()
                    };
                    let (dir, tarball) = download(&version)?;
                    _download_dir = Some(dir);
                    (tarball, Some(version))
                }
            };
            install_app(&source, &target, version.as_deref())?;
            let version = app_version(&target).map(|v| format!(" ({v})"));
            println!(
                "\x1b[32m✓\x1b[0m Installed {}{}",
                target.display(),
                version.unwrap_or_default()
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

/// Where a fresh install goes: `/Applications` when writable, else `~/Applications`
/// (Linux: `~/.local/bin/atlas-desktop`; Windows: `%LOCALAPPDATA%\Atlas\atlas-desktop.exe`).
#[cfg(target_os = "linux")]
pub(crate) fn default_location() -> PathBuf {
    atlas_core::paths::linux_app(&dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
}

#[cfg(windows)]
pub(crate) fn default_location() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    atlas_core::paths::windows_app(&atlas_core::paths::local_app_data(&home))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "linux")]
pub(crate) fn app_version(app: &Path) -> Option<String> {
    super::desktop_linux::app_version(app)
}

#[cfg(windows)]
pub(crate) fn app_version(app: &Path) -> Option<String> {
    super::desktop_windows::app_version(app)
}

/// `CFBundleShortVersionString` of an installed bundle.
#[cfg(target_os = "macos")]
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
    let ext = if cfg!(target_os = "macos") {
        ".app.tar.gz"
    } else if cfg!(windows) {
        "-setup.exe"
    } else {
        ".AppImage"
    };
    format!("Atlas-{version}-{}{ext}", current_platform())
}

// ── actions ───────────────────────────────────────────────────────────────────

fn status(installed: Option<&Path>) -> Result<()> {
    match installed {
        None => match atlas_core::paths::packaged_app() {
            Some(pkg) => println!(
                "Atlas Desktop: {} (from a distro package; update it with your package manager)",
                pkg.display()
            ),
            None => println!("Atlas Desktop: not installed (run: atlas desktop)"),
        },
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
    // The update that spawned this already holds the lock; a standalone `--sync` takes it.
    let _lock = if std::env::var_os(update_core::HOLDS_LOCK_ENV).is_some() {
        None
    } else {
        Some(update_core::UpdateLock::acquire()?)
    };
    sync_app(installed)
}

/// Bring the installed app to this CLI's version. Fails (instead of printing and moving on)
/// so `atlas update` can roll the CLI back and the app never ends up on another version.
pub(crate) fn sync_app(installed: Option<&Path>) -> Result<()> {
    let Some(app) = installed else { return Ok(()) };
    if is_symlink(app) {
        return Ok(());
    }
    let want = current_version();
    if app_version(app).as_deref() == Some(want) {
        return Ok(());
    }
    update_core::phase(update_core::Phase::UpdateApp);
    println!("\x1b[1;34m◆\x1b[0m Updating Atlas Desktop to v{want}...");
    let (_dir, src) = download(want)?;
    install_app(&src, app, Some(want))?;
    println!("\x1b[32m✓\x1b[0m Atlas Desktop v{want}");
    Ok(())
}

fn latest_release() -> Result<String> {
    Ok(
        atlas_core::github::latest_release(REPO, atlas_core::channel::current())?
            .version()
            .to_string(),
    )
}

/// Download the app tarball (+ checksum) of `version` into a temp dir and verify it.
/// The `TempDir` must outlive the install: dropping it deletes the tarball.
fn download(version: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    let asset = asset_name(version);
    let dir = tempfile::Builder::new()
        .prefix(&format!("atlas-desktop-{version}-"))
        .tempdir()?;
    println!("\x1b[1;34m◆\x1b[0m Downloading {asset}...");
    let tarball = download_verified(
        &format!("v{version}"),
        &asset,
        dir.path(),
        update_core::bytes,
    )?;
    Ok((dir, tarball))
}

/// Install the downloaded (or `--from`) app at `target`; `version` when known.
fn install_app(source: &Path, target: &Path, version: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = version; // the bundle's Info.plist carries it
        install(source, target)
    }
    #[cfg(target_os = "linux")]
    return super::desktop_linux::install(source, target, version);
    #[cfg(windows)]
    super::desktop_windows::install(source, target, version)
}

/// Put the bundle at `source` (`.app` dir or `.app.tar.gz`) at `target`, replacing any
/// existing app only once the new one is fully in place next to it.
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn remove(p: &Path) -> std::io::Result<()> {
    if is_symlink(p) || p.is_file() {
        std::fs::remove_file(p)
    } else {
        std::fs::remove_dir_all(p)
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn open(app: &Path) -> Result<()> {
    run(Command::new("/usr/bin/open").arg(app))
}

#[cfg(target_os = "linux")]
fn open(app: &Path) -> Result<()> {
    super::desktop_linux::open(app)
}

#[cfg(windows)]
fn open(app: &Path) -> Result<()> {
    super::desktop_windows::open(app)
}

/// `bun run desktop:dev` from the atlas workspace (hot-reloading frontend + Tauri shell).
/// Agent instances live on a private macOS virtual display; there is no equivalent here.
#[cfg(windows)]
fn agent(_stop: bool, _name: Option<String>, _scratch: bool) -> Result<()> {
    bail!("agent instances (atlas desktop --agent) need macOS")
}

/// `atlas desktop --agent [--name N] [--scratch] [--stop]`: see `desktop_agent.rs`.
#[cfg(unix)]
fn agent(stop: bool, name: Option<String>, scratch: bool) -> Result<()> {
    if stop {
        return super::desktop_agent::stop(name.as_deref());
    }
    let exe = std::env::current_exe()?;
    // The CLI built from a checkout knows its repo; otherwise the checkout we are standing in.
    let repo = match super::desktop_agent::repo_of_exe(&exe) {
        Some(r) => r,
        None => super::link::find_workspace_root()?,
    };
    super::desktop_agent::start(super::desktop_agent::StartOpts {
        name,
        scratch,
        atlas_bin: exe,
        repo,
        extra_env: vec![],
    })
}

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

    /// The asset the release workflow uploads per platform (`release.yml`, `generate-manifest.py`).
    #[test]
    fn the_app_asset_is_the_platforms_bundle() {
        let name = asset_name("1.6.0");
        if cfg!(target_os = "macos") {
            assert_eq!(
                name,
                format!("Atlas-1.6.0-{}.app.tar.gz", current_platform())
            );
        } else if cfg!(windows) {
            assert_eq!(name, "Atlas-1.6.0-x86_64-pc-windows-msvc-setup.exe");
        } else {
            assert_eq!(name, format!("Atlas-1.6.0-{}.AppImage", current_platform()));
        }
    }

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

    #[cfg(target_os = "macos")]
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
