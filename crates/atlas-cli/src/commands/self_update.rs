use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::release::{REPO, download_verified};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Hit GitHub at most every 6h (cached result is reused in between).
const CHECK_INTERVAL_SECS: u64 = 6 * 3600;

// ── Update check cache ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub(crate) struct UpdateCache {
    pub(crate) last_checked: u64,
    pub(crate) available_version: Option<String>,
    /// "Not now" answer to the pre-flight prompt: that version is not offered again
    /// until `until` (unix secs). Absent in caches written by older atlas versions.
    #[serde(default)]
    pub(crate) dismissed: Option<Dismissal>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct Dismissal {
    pub(crate) version: String,
    pub(crate) until: u64,
}

/// How long a "No" to the pre-flight prompt silences that version.
pub(crate) const DISMISS_SECS: u64 = 24 * 3600;

impl UpdateCache {
    /// Cache after a check: fresh timestamp and result, keeping a still-relevant dismissal.
    fn checked(&self, available: Option<String>) -> Self {
        Self {
            last_checked: now_secs(),
            available_version: available,
            dismissed: self.dismissed.clone(),
        }
    }

    /// The cache is older than the check interval (or was never written).
    pub(crate) fn is_stale(&self, now: u64) -> bool {
        now.saturating_sub(self.last_checked) >= CHECK_INTERVAL_SECS
    }

    /// `version` was dismissed and the 24h silence has not elapsed.
    pub(crate) fn is_dismissed(&self, version: &str, now: u64) -> bool {
        self.dismissed
            .as_ref()
            .is_some_and(|d| d.version == version && now < d.until)
    }

    pub(crate) fn dismiss(&mut self, version: &str, now: u64) {
        self.dismissed = Some(Dismissal {
            version: version.to_string(),
            until: now + DISMISS_SECS,
        });
    }
}

fn cache_path() -> PathBuf {
    atlas_core::paths::atlas_home().join(".update_check")
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn load_cache() -> UpdateCache {
    let path = cache_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_cache(cache: &UpdateCache) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serde_json::to_string(cache).unwrap_or_default());
}

// ── Install method detection ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InstallMethod {
    Homebrew,
    ShellInstaller,
    Manual,
}

impl InstallMethod {
    pub fn detect() -> Self {
        let exe = env::current_exe().unwrap_or_default();
        let path = exe.to_string_lossy();
        if path.contains("/opt/homebrew")
            || path.contains("/usr/local/Cellar")
            || path.contains("/home/linuxbrew")
        {
            Self::Homebrew
        } else if path.contains(".atlas/bin") {
            Self::ShellInstaller
        } else {
            Self::Manual
        }
    }
}

// ── Platform ──────────────────────────────────────────────────────────────────

pub fn current_platform() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else {
        format!("{arch}-unknown-linux-gnu")
    }
}

// ── Background update check ───────────────────────────────────────────────────

/// Newer version than the running binary, if `v` parses and is greater.
pub(crate) fn newer_than_current(v: &str) -> Option<String> {
    newer_than(CURRENT_VERSION, v)
}

/// `candidate` when it parses and is strictly greater than `current`.
pub(crate) fn newer_than(current: &str, candidate: &str) -> Option<String> {
    let current = semver::Version::parse(current).ok()?;
    (semver::Version::parse(candidate).ok()? > current).then(|| candidate.to_string())
}

pub(crate) const fn current_version() -> &'static str {
    CURRENT_VERSION
}

/// Background update check, best-effort and silent on errors.
/// Publishes a cached result immediately, refreshes it when older than
/// `CHECK_INTERVAL_SECS`, and keeps re-checking for long-lived commands
/// (`atlas dev` sessions that stay open for days still learn about releases).
/// The TUI reads it via `atlas_tui::update_notice`; `print_update_notice`
/// prints the exit banner.
pub fn spawn_background_check() {
    if let Some(v) = load_cache()
        .available_version
        .as_deref()
        .and_then(newer_than_current)
    {
        atlas_tui::update_notice::publish(v);
    }
    tokio::spawn(async {
        loop {
            let _ = background_check_inner().await;
            tokio::time::sleep(std::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;
        }
    });
}

async fn background_check_inner() -> Result<()> {
    let cache = load_cache();
    if !cache.is_stale(now_secs()) {
        return Ok(());
    }
    let available = newer_than_current(&fetch_latest_version().await?);
    if let Some(v) = &available {
        atlas_tui::update_notice::publish(v.clone());
    }
    save_cache(&cache.checked(available));
    Ok(())
}

/// Newest version for the pre-flight prompt: the cache when fresh, otherwise a live
/// check bounded by `timeout`. Timeout or error falls back to the cached value silently.
pub(crate) async fn newest_for_preflight(
    timeout: std::time::Duration,
) -> (UpdateCache, Option<String>) {
    let cache = load_cache();
    let cached = cache
        .available_version
        .as_deref()
        .and_then(newer_than_current);
    if !cache.is_stale(now_secs()) {
        return (cache, cached);
    }
    match tokio::time::timeout(timeout, fetch_latest_version()).await {
        Ok(Ok(latest)) => {
            let available = newer_than_current(&latest);
            let fresh = cache.checked(available.clone());
            save_cache(&fresh);
            (fresh, available)
        }
        _ => (cache, cached),
    }
}

/// Print the update banner if a newer release is known. Call after the
/// command's own output (and after the TUI restored the terminal).
pub fn print_update_notice() {
    let Some(v) = atlas_tui::update_notice::available().or_else(|| {
        load_cache()
            .available_version
            .as_deref()
            .and_then(newer_than_current)
    }) else {
        return;
    };
    let width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
        .clamp(40, 200);
    let rule = "─".repeat(width);
    eprintln!();
    eprintln!("\x1b[2m{rule}\x1b[0m");
    eprintln!(" \x1b[1;32m⬆ Update available\x1b[0m");
    eprintln!(
        " atlas \x1b[2mv{CURRENT_VERSION}\x1b[0m → \x1b[1mv{v}\x1b[0m   Run: \x1b[1;36matlas update\x1b[0m"
    );
    eprintln!("\x1b[2m{rule}\x1b[0m");
}

/// Fetch the latest published version from the public GitHub Releases API.
async fn fetch_latest_version() -> Result<String> {
    let release =
        tokio::task::spawn_blocking(|| atlas_core::github::latest_release(REPO)).await??;
    let tag = release.version().to_string();
    if tag.is_empty() {
        bail!("empty version from GitHub");
    }
    Ok(tag)
}

// ── atlas update handler ──────────────────────────────────────────────────────

/// What `run_update` did to the installed binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateOutcome {
    /// The binary on disk is now the newer release.
    Installed,
    /// Nothing was replaced (Homebrew, already current, `--check`).
    Unchanged,
}

pub async fn handle(check_only: bool, channel: Option<String>) -> Result<()> {
    run_update(check_only, channel).await.map(|_| ())
}

pub(crate) async fn run_update(check_only: bool, channel: Option<String>) -> Result<UpdateOutcome> {
    let method = InstallMethod::detect();

    if method == InstallMethod::Homebrew {
        println!("\x1b[1;34m◆\x1b[0m Installed via Homebrew — run:");
        println!("  brew upgrade atlas");
        return Ok(UpdateOutcome::Unchanged);
    }

    // `atlas link` made ~/.atlas/bin/atlas a symlink to target/debug/atlas: replacing
    // "the binary" would overwrite the dev build, not install a release.
    if !check_only
        && let installed = atlas_core::paths::atlas_home().join("bin/atlas")
        && installed
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    {
        bail!(
            "atlas is linked to a dev build ({}); run `atlas unlink` first",
            installed.display()
        );
    }

    println!("\x1b[1;34m◆\x1b[0m Checking for updates...");
    println!("  Current: v{CURRENT_VERSION}");
    let _channel = channel.as_deref().unwrap_or("stable");
    println!();

    let latest = fetch_latest_version()
        .await
        .context("Could not check for updates. Check your network connection (needs curl).")?;

    let current = semver::Version::parse(CURRENT_VERSION).context("Bad current version")?;
    let remote = semver::Version::parse(&latest).context("Bad remote version")?;

    if remote <= current {
        println!("\x1b[1;32m✓\x1b[0m Already up to date (v{CURRENT_VERSION})");
        // Clear any stale cache entry
        save_cache(&load_cache().checked(None));
        // The app may still lag behind (an earlier sync failed or was skipped).
        if !check_only {
            run_self(&["desktop", "--sync"]);
        }
        return Ok(UpdateOutcome::Unchanged);
    }

    println!("  Latest:  v{latest}");
    println!("  Changelog: https://github.com/{REPO}/releases/tag/v{latest}");
    println!();

    if check_only {
        println!("\x1b[1;33m◆\x1b[0m Update available: v{CURRENT_VERSION} → v{latest}");
        println!("  Run \x1b[1matlas update\x1b[0m to install.");
        return Ok(UpdateOutcome::Unchanged);
    }

    println!("\x1b[1;34m◆\x1b[0m Downloading v{latest}...");

    let platform = current_platform();
    let tarball = format!("atlas-{latest}-{platform}.tar.gz");
    let tmp_dir = tempfile::Builder::new().prefix("atlas-update-").tempdir()?;

    let tarball_path = download_verified(&format!("v{latest}"), &tarball, tmp_dir.path())
        .with_context(|| {
            format!("Download failed. Releases: https://github.com/{REPO}/releases")
        })?;

    println!("\x1b[1;34m◆\x1b[0m Installing...");

    let binary = extract_tarball(&std::fs::read(&tarball_path)?)?;

    let tmp_bin = tempfile::Builder::new()
        .prefix("atlas-update-bin-")
        .tempfile()?;
    std::fs::write(tmp_bin.path(), &binary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp_bin.path(), std::fs::Permissions::from_mode(0o755))?;
    }
    self_replace::self_replace(tmp_bin.path())
        .context("Failed to replace binary. Try: sudo atlas update")?;

    // Clear cache
    save_cache(&load_cache().checked(None));

    println!();
    println!("\x1b[1;32m✓\x1b[0m Updated to v{latest}");
    println!();

    // The skills (and agent hook) are embedded in the *new* binary, so refresh them by
    // running it; the running process still holds the old embedded copy.
    println!("\x1b[1;34m◆\x1b[0m Refreshing skills...");
    refresh_with_new_binary();

    Ok(UpdateOutcome::Installed)
}

/// Run the freshly installed binary to refresh the skills and, when already set up,
/// the OMP / Claude Code / Codex integrations (hooks, MCP entry), all embedded in that binary,
/// and bring an installed Atlas Desktop to the same version.
/// Failures are non-fatal.
fn refresh_with_new_binary() {
    let Ok(exe) = env::current_exe() else {
        eprintln!("\x1b[33m◆\x1b[0m Skills refresh skipped: cannot locate the new binary");
        return;
    };
    for args in [
        &["skills", "install"][..],
        &["setup", "omp", "--refresh"][..],
        &["setup", "claude-code", "--refresh"][..],
        &["setup", "codex", "--refresh"][..],
        // An installed Atlas Desktop follows the CLI version (no-op when absent or linked).
        &["desktop", "--sync"][..],
    ] {
        run_self_with(&exe, args);
    }
}

fn run_self(args: &[&str]) {
    match env::current_exe() {
        Ok(exe) => run_self_with(&exe, args),
        Err(e) => eprintln!("\x1b[33m◆\x1b[0m `atlas {}` skipped: {e}", args.join(" ")),
    }
}

fn run_self_with(exe: &std::path::Path, args: &[&str]) {
    match std::process::Command::new(exe).args(args).status() {
        Ok(st) if st.success() => {}
        Ok(st) => eprintln!(
            "\x1b[33m◆\x1b[0m `atlas {}` failed ({st}) (non-fatal)",
            args.join(" ")
        ),
        Err(e) => eprintln!(
            "\x1b[33m◆\x1b[0m `atlas {}` failed: {e} (non-fatal)",
            args.join(" ")
        ),
    }
}

// ── Tarball extraction ────────────────────────────────────────────────────────

fn extract_tarball(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let gz = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path
            .file_name()
            .map(|n| n.to_string_lossy() == "atlas")
            .unwrap_or(false)
        {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    bail!("Could not find atlas binary in tarball")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let p = current_platform();
        assert!(p.contains("apple-darwin") || p.contains("linux"));
        assert!(p.starts_with("aarch64") || p.starts_with("x86_64"));
    }

    #[test]
    fn test_install_method_detect() {
        let m = InstallMethod::detect();
        assert!(matches!(
            m,
            InstallMethod::Manual | InstallMethod::ShellInstaller | InstallMethod::Homebrew
        ));
    }

    #[test]
    fn test_version_comparison() {
        let current = semver::Version::parse("0.1.0").unwrap();
        let latest = semver::Version::parse("0.2.0").unwrap();
        assert!(latest > current);
    }

    #[test]
    fn test_platform_format() {
        let p = current_platform();
        assert!(p.split('-').count() >= 3);
    }

    #[test]
    fn test_cache_roundtrip() {
        let mut cache = UpdateCache {
            last_checked: 1234567890,
            available_version: Some("1.2.3".into()),
            dismissed: None,
        };
        cache.dismiss("1.2.3", 1000);
        let json = serde_json::to_string(&cache).unwrap();
        let loaded: UpdateCache = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, cache);
    }

    #[test]
    fn cache_written_by_older_atlas_still_loads() {
        // Pre-dismissal format: no `dismissed` key.
        let loaded: UpdateCache =
            serde_json::from_str(r#"{"last_checked":42,"available_version":"9.9.9"}"#).unwrap();
        assert_eq!(loaded.last_checked, 42);
        assert_eq!(loaded.available_version.as_deref(), Some("9.9.9"));
        assert_eq!(loaded.dismissed, None);
        let none: UpdateCache =
            serde_json::from_str(r#"{"last_checked":1,"available_version":null}"#).unwrap();
        assert_eq!(none.available_version, None);
    }

    #[test]
    fn dismissal_expires_after_24h_and_is_per_version() {
        let mut cache = UpdateCache::default();
        cache.dismiss("1.0.0", 1_000);
        assert!(cache.is_dismissed("1.0.0", 1_000));
        assert!(cache.is_dismissed("1.0.0", 1_000 + DISMISS_SECS - 1));
        assert!(
            !cache.is_dismissed("1.0.0", 1_000 + DISMISS_SECS),
            "expired at 24h"
        );
        assert!(
            !cache.is_dismissed("1.0.1", 1_000),
            "a newer version is offered again"
        );
    }

    #[test]
    fn refreshing_the_check_keeps_the_dismissal() {
        let mut cache = UpdateCache::default();
        cache.dismiss("1.0.0", 5);
        let refreshed = cache.checked(Some("1.0.0".into()));
        assert_eq!(refreshed.dismissed, cache.dismissed);
        assert_eq!(refreshed.available_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn staleness_uses_the_6h_interval() {
        let cache = UpdateCache {
            last_checked: 100,
            ..Default::default()
        };
        assert!(!cache.is_stale(100 + CHECK_INTERVAL_SECS - 1));
        assert!(cache.is_stale(100 + CHECK_INTERVAL_SECS));
        assert!(UpdateCache::default().is_stale(now_secs()), "never checked");
    }

    #[test]
    fn newer_than_compares_semver_not_strings() {
        assert_eq!(newer_than("0.4.1", "0.10.0").as_deref(), Some("0.10.0"));
        assert_eq!(newer_than("0.4.1", "0.4.1"), None);
        assert_eq!(newer_than("0.4.1", "0.4.0"), None);
        assert_eq!(newer_than("0.4.1", "garbage"), None);
    }
}

use super::AtlasCommand;
use clap::Args;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Only check for updates, do not install
    #[arg(long)]
    pub check: bool,
    #[arg(long, hide = true)]
    pub channel: Option<String>,
}

impl AtlasCommand for UpdateArgs {
    async fn run(self) -> anyhow::Result<()> {
        handle(self.check, self.channel).await
    }
}
