//! `atlas update`: the install-method aware self-updater, plus the cached background
//! update check behind the exit banner and the pre-flight prompt.

use anyhow::{Context, Result, bail};
use atlas_core::channel::{self, Channel, Decision};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::AtlasCommand;
use super::release::{REPO, download_verified};
use super::update_channel::{self as ch, Pick};
use super::update_core;
use super::update_pkg::{self, PackageManager};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Hit GitHub at most every 6h (cached result is reused in between).
const CHECK_INTERVAL_SECS: u64 = 6 * 3600;
/// After a failed live check (offline), do not try again for this long.
pub(crate) const FAILED_BACKOFF_SECS: u64 = 30 * 60;

/// `atlas update` — see `docs/cli.md#atlas-update`.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    #[command(subcommand)]
    pub command: Option<UpdateCmd>,
    /// Only check for updates, do not install
    #[arg(long)]
    pub check: bool,
    /// Follow this release channel (stable, beta or alpha); remembered for later updates
    #[arg(long, value_name = "CHANNEL")]
    pub channel: Option<Channel>,
    /// Print the result as JSON (with --check)
    #[arg(long, requires = "check")]
    pub json: bool,
    /// Install the older release the channel offers when you run a newer build
    #[arg(long)]
    pub downgrade: bool,
    /// Do not ask for confirmation (a downgrade, or installing a system package as root)
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Debug, Subcommand)]
pub enum UpdateCmd {
    /// Show the release channels and what each offers now, or switch to one
    Channel {
        /// stable, beta or alpha; omit to list them (and pick one in a terminal)
        name: Option<Channel>,
        /// Print machine-readable JSON; never prompts
        #[arg(long)]
        json: bool,
    },
}

impl AtlasCommand for UpdateArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            Some(UpdateCmd::Channel { name, json }) => channel_command(name, json).await,
            None => run_update(Request {
                check_only: self.check,
                channel: self.channel,
                json: self.json,
                downgrade: self.downgrade,
                yes: self.yes,
            })
            .await
            .map(|_| ()),
        }
    }
}

// ── Update check cache ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub(crate) struct UpdateCache {
    pub(crate) last_checked: u64,
    pub(crate) available_version: Option<String>,
    /// "Not now" answer to the pre-flight prompt: that version is not offered again
    /// until `until` (unix secs). Absent in caches written by older atlas versions.
    #[serde(default)]
    pub(crate) dismissed: Option<Dismissal>,
    /// The channel `available_version` was computed for. Absent in caches written before
    /// channels existed, which only ever knew the stable releases.
    #[serde(default)]
    pub(crate) channel: Option<String>,
    /// When the last live check *failed* (offline, GitHub down), unix secs. Until
    /// [`FAILED_BACKOFF_SECS`] passed, nothing re-checks, so an offline `atlas dev` does not
    /// wait on the network at every start. Cleared by the next successful check.
    #[serde(default)]
    pub(crate) failed_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct Dismissal {
    pub(crate) version: String,
    pub(crate) until: u64,
}

/// How long a "No" to the pre-flight prompt silences that version.
pub(crate) const DISMISS_SECS: u64 = 24 * 3600;

impl UpdateCache {
    /// Cache after a check on `on`: fresh timestamp and result, keeping a still-relevant dismissal.
    pub(crate) fn checked_for(&self, on: Channel, available: Option<String>) -> Self {
        Self {
            last_checked: now_secs(),
            available_version: available,
            dismissed: self.dismissed.clone(),
            channel: Some(on.as_str().to_string()),
            failed_at: None,
        }
    }

    /// Cache after a check on `on` that could not reach GitHub: what was known for `on` stays
    /// (the notice keeps working from it), the failure is stamped so nothing retries for a while.
    /// An offer computed for another channel is not carried over.
    pub(crate) fn failed_for(&self, on: Channel, now: u64) -> Self {
        let same = self.valid_for(on);
        Self {
            last_checked: if same { self.last_checked } else { 0 },
            available_version: if same {
                self.available_version.clone()
            } else {
                None
            },
            dismissed: self.dismissed.clone(),
            channel: Some(on.as_str().to_string()),
            failed_at: Some(now),
        }
    }

    /// Computed for `on` (a cache without a channel is a stable one).
    pub(crate) fn valid_for(&self, on: Channel) -> bool {
        self.channel.as_deref().unwrap_or(Channel::Stable.as_str()) == on.as_str()
    }

    /// A check is due: the cache is older than the check interval, never written, or computed
    /// for another channel, and the last attempt did not fail within the backoff window.
    pub(crate) fn is_stale_for(&self, on: Channel, now: u64) -> bool {
        if self
            .failed_at
            .is_some_and(|f| now.saturating_sub(f) < FAILED_BACKOFF_SECS)
        {
            return false;
        }
        !self.valid_for(on) || now.saturating_sub(self.last_checked) >= CHECK_INTERVAL_SECS
    }

    /// The cached version when it is an upgrade over `installed` on `on`, else `None`.
    pub(crate) fn available_for(&self, installed: &semver::Version, on: Channel) -> Option<String> {
        if !self.valid_for(on) {
            return None;
        }
        background_available(installed, self.available_version.as_deref()?)
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
    load_cache_at(&cache_path())
}

/// The cache at `path`. A file that does not parse as a whole (a field of an unexpected type,
/// a hand edit) still keeps what can be salvaged — the dismissal above all, so "No" to the
/// prompt is not forgotten — and counts as never checked. Missing or unreadable: default.
pub(crate) fn load_cache_at(path: &std::path::Path) -> UpdateCache {
    let Ok(text) = std::fs::read_to_string(path) else {
        return UpdateCache::default();
    };
    if let Ok(cache) = serde_json::from_str(&text) {
        return cache;
    }
    let dismissed = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| serde_json::from_value::<Dismissal>(v.get("dismissed")?.clone()).ok());
    UpdateCache {
        dismissed,
        ..UpdateCache::default()
    }
}

pub(crate) fn save_cache(cache: &UpdateCache) {
    save_cache_at(&cache_path(), cache);
}

/// Best-effort and atomic (`.tmp` + rename): `atlas dev` and `atlas update` may run at once.
pub(crate) fn save_cache_at(path: &std::path::Path, cache: &UpdateCache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = update_core::write_atomic(path, json.as_bytes());
    }
}

// ── Install method detection ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InstallMethod {
    Homebrew,
    ShellInstaller,
    /// A Linux distro package (.deb, .rpm, Arch) owns the binary: its package manager updates
    /// it. `Some` names the manager that owns it (asked with `pacman -Qo`, `dpkg -S`, `rpm -qf`,
    /// or, for a `/usr/bin` binary no database claims, the first manager installed); `None` is
    /// a `/usr/bin` binary on a machine with none of them, where only instructions can be given.
    SystemPackage(Option<PackageManager>),
    Manual,
}

impl InstallMethod {
    /// How `exe` was installed. `owner` asks the package databases (`pacman -Qo`, `dpkg -S`,
    /// `rpm -qf`); it is injected so the table is testable without any of them. A binary under
    /// `/usr/bin` that no database claims is still a system package (the path rule), updated
    /// with whichever manager the machine has.
    pub(crate) fn of_exe(
        exe: &std::path::Path,
        owner: impl FnOnce(&std::path::Path) -> Option<PackageManager>,
    ) -> Self {
        match Self::of_path(&exe.to_string_lossy()) {
            Self::SystemPackage(_) => Self::SystemPackage(owner(exe).or_else(installed_manager)),
            // A package can own a binary outside /usr/bin (a custom prefix); the database
            // knows. A checkout's build output or a cargo install is never package-owned, so
            // no database is asked about it. Machines without those tools answer None.
            Self::Manual if !is_dev_location(exe) => {
                owner(exe).map_or(Self::Manual, |pm| Self::SystemPackage(Some(pm)))
            }
            other => other,
        }
    }

    /// The path rule alone: where each installer puts the binary.
    fn of_path(path: &str) -> Self {
        let path = &path.replace('\\', "/");
        if path.contains("/opt/homebrew")
            || path.contains("/usr/local/Cellar")
            || path.contains("/home/linuxbrew")
        {
            Self::Homebrew
        } else if path.contains(".atlas/bin") {
            Self::ShellInstaller
        } else if cfg!(target_os = "linux")
            && (path.starts_with("/usr/bin/") || path.starts_with("/bin/"))
        {
            Self::SystemPackage(None)
        } else {
            Self::Manual
        }
    }
}

/// A checkout's build output or a cargo install: never package-owned, so no database is asked.
fn is_dev_location(exe: &std::path::Path) -> bool {
    let p = exe.to_string_lossy().replace('\\', "/");
    p.contains("/target/") || p.contains("/.cargo/bin/")
}

/// The package manager this machine has, for a `/usr/bin` binary no package database claims.
fn installed_manager() -> Option<PackageManager> {
    let has = |n: &str| atlas_core::paths::which(n).is_some();
    if has("pacman") {
        Some(PackageManager::Pacman)
    } else if has("dpkg") {
        Some(PackageManager::Apt)
    } else if has("rpm") {
        Some(if has("zypper") && !has("dnf") {
            PackageManager::Zypper
        } else {
            PackageManager::Dnf
        })
    } else {
        None
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
    } else if cfg!(windows) {
        format!("{arch}-pc-windows-msvc")
    } else {
        format!("{arch}-unknown-linux-gnu")
    }
}

// ── Background update check ───────────────────────────────────────────────────

/// Newer version than the running binary, if `v` parses and is greater.
pub(crate) fn newer_than_current(v: &str) -> Option<String> {
    newer_than(CURRENT_VERSION, v)
}

/// `candidate` when it parses and its SemVer precedence is above `current`'s (build metadata
/// does not count; a leading `v` is fine). Only ever an upgrade: never a downgrade.
pub(crate) fn newer_than(current: &str, candidate: &str) -> Option<String> {
    let current = channel::parse_tag(current)?;
    background_available(&current, candidate)
}

/// What a background check may report for `offered`: the version when it is an
/// [`Decision::Upgrade`] over `installed`, nothing for current or older builds.
pub(crate) fn background_available(installed: &semver::Version, offered: &str) -> Option<String> {
    let (_, decision) = ch::offer(installed, offered)?;
    (decision == Decision::Upgrade).then(|| offered.to_string())
}

pub(crate) const fn current_version() -> &'static str {
    CURRENT_VERSION
}

/// The cached upgrade for the persisted channel, if any.
fn cached_available(cache: &UpdateCache, on: Channel) -> Option<String> {
    cache.available_for(&installed_version(), on)
}

/// Background update check, best-effort and silent on errors.
/// Publishes a cached result immediately, refreshes it when older than
/// `CHECK_INTERVAL_SECS` (or when the channel changed), and keeps re-checking for long-lived
/// commands (`atlas dev` sessions that stay open for days still learn about releases).
/// It follows the persisted channel and only ever reports an upgrade.
/// The TUI reads it via `atlas_tui::update_notice`; `print_update_notice`
/// prints the exit banner.
pub fn spawn_background_check() {
    if let Some(v) = cached_available(&load_cache(), channel::current()) {
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
    let on = channel::current();
    let cache = load_cache();
    if !cache.is_stale_for(on, now_secs()) {
        return Ok(());
    }
    match fetch_latest_version(on).await {
        Ok(latest) => {
            let available = newer_than_current(&latest);
            if let Some(v) = &available {
                atlas_tui::update_notice::publish(v.clone());
            }
            save_cache(&cache.checked_for(on, available));
            Ok(())
        }
        Err(e) => {
            // Keep what was known; do not ask again before the backoff passed.
            save_cache(&cache.failed_for(on, now_secs()));
            Err(e)
        }
    }
}

/// Newest version for the pre-flight prompt: the cache when fresh, otherwise a live
/// check bounded by `timeout`. Timeout or error falls back to the cached value silently and
/// stamps the failure, so an offline machine does not wait `timeout` at every start.
pub(crate) async fn newest_for_preflight(
    timeout: std::time::Duration,
) -> (UpdateCache, Option<String>) {
    let on = channel::current();
    let cache = load_cache();
    let cached = cached_available(&cache, on);
    if !cache.is_stale_for(on, now_secs()) {
        return (cache, cached);
    }
    match tokio::time::timeout(timeout, fetch_latest_version(on)).await {
        Ok(Ok(latest)) => {
            let available = newer_than_current(&latest);
            let fresh = cache.checked_for(on, available.clone());
            save_cache(&fresh);
            (fresh, available)
        }
        _ => {
            let failed = cache.failed_for(on, now_secs());
            save_cache(&failed);
            (failed, cached)
        }
    }
}

/// What the user runs to take `version`: `atlas update` where atlas installs it itself, the
/// package manager's own command for a package install (`atlas update` also does it, but the
/// exact command is what one can read, copy and audit), `brew upgrade atlas` for Homebrew.
pub(crate) fn update_command(method: &InstallMethod, version: &str) -> String {
    match method {
        InstallMethod::Homebrew => "brew upgrade atlas".to_string(),
        InstallMethod::SystemPackage(Some(pm)) => {
            format!("atlas update   ({})", pm.manual_command(version))
        }
        _ => "atlas update".to_string(),
    }
}

/// Print the update banner if a newer release is known. Call after the
/// command's own output (and after the TUI restored the terminal).
pub fn print_update_notice() {
    let Some(v) = atlas_tui::update_notice::available()
        .or_else(|| cached_available(&load_cache(), channel::current()))
    else {
        return;
    };
    let width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
        .clamp(40, 200);
    let rule = "─".repeat(width);
    let run = update_command(
        &InstallMethod::of_exe(&update_core::judged_exe(), update_pkg::owner),
        &v,
    );
    eprintln!();
    eprintln!("\x1b[2m{rule}\x1b[0m");
    eprintln!(" \x1b[1;32m⬆ Update available\x1b[0m");
    eprintln!(
        " atlas \x1b[2mv{CURRENT_VERSION}\x1b[0m → \x1b[1mv{v}\x1b[0m   Run: \x1b[1;36m{run}\x1b[0m"
    );
    eprintln!("\x1b[2m{rule}\x1b[0m");
}

/// The tag `on` offers right now (`v1.4.0`, from the `releases/latest` redirect for Stable,
/// the release feed for Beta and Alpha); never calls the GitHub API.
async fn fetch_latest_tag(on: Channel) -> Result<String> {
    let tag =
        tokio::task::spawn_blocking(move || atlas_core::github::channel_tag(REPO, on)).await??;
    if tag.is_empty() {
        bail!("empty version from GitHub");
    }
    Ok(tag)
}

/// [`fetch_latest_tag`] without the leading `v`: the version the banner and the cache hold.
async fn fetch_latest_version(on: Channel) -> Result<String> {
    let tag = fetch_latest_tag(on).await?;
    Ok(tag.strip_prefix('v').unwrap_or(&tag).to_string())
}

// ── atlas update handler ──────────────────────────────────────────────────────

/// What `run_update` did to the installed binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateOutcome {
    /// The binary on disk is now the newer release.
    Installed,
    /// Nothing was replaced (already current, `--check`, a declined confirmation).
    Unchanged,
    /// Nothing was installed because another tool owns this Atlas (Homebrew, or a system
    /// package with no usable package manager): the user was told what to run. Callers must not
    /// report "up to date" or re-exec for it.
    Managed,
}

/// Ask before installing an older build (default No). `--yes` skips the question; outside a
/// terminal there is nobody to ask, so it needs `--yes`.
fn confirm_downgrade(
    current: &semver::Version,
    offered: &semver::Version,
    on: Channel,
    yes: bool,
) -> Result<bool> {
    println!();
    println!(
        "\x1b[1;33m!\x1b[0m Downgrading v{current} → v{offered} ({on}). Data written by v{current} (settings, caches, \
         terminal host state) may not be readable by the older build; back up ~/.atlas first if it matters."
    );
    if yes {
        return Ok(true);
    }
    if !interactive() {
        bail!("a downgrade needs confirmation: run it in a terminal, or pass --yes");
    }
    print!("Install v{offered} now? [y/N] ");
    std::io::stdout().flush()?;
    Ok(ch::parse_confirm(&read_line().unwrap_or_default()))
}

/// What `atlas update` was asked to do.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Request {
    pub check_only: bool,
    /// `--channel X`: persisted, then followed. `None` follows the persisted/env channel.
    pub channel: Option<Channel>,
    pub json: bool,
    pub downgrade: bool,
    pub yes: bool,
}

/// Stdin line, `None` on EOF or error.
fn read_line() -> Option<String> {
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Persist `on` in the file shared with Atlas Desktop.
fn remember_channel(on: Channel) -> Result<()> {
    ch::persist_channel(&channel::file(), on).with_context(|| {
        format!(
            "cannot save the update channel to {}",
            channel::file().display()
        )
    })
}

/// `atlas update channel [<name>] [--json]`.
async fn channel_command(name: Option<Channel>, json: bool) -> Result<()> {
    let installed = installed_version();
    if let Some(name) = name {
        remember_channel(name)?;
    }
    let selected = name.unwrap_or_else(channel::current);
    let views = {
        let installed = installed.clone();
        tokio::task::spawn_blocking(move || {
            atlas_core::github::channel_views(REPO, &installed, selected)
        })
        .await?
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ch::channels_json(&views, selected, &installed))?
        );
        return Ok(());
    }
    if let Some(name) = name {
        let view = views
            .iter()
            .find(|v| v.channel == name)
            .context("channel missing from the listing")?;
        print!("{}", ch::render_switched(view));
        return Ok(());
    }

    print!("{}", ch::render_channels(&views, &installed));
    if !interactive() {
        println!("Switch with: atlas update channel <stable|beta|alpha>");
        return Ok(());
    }
    loop {
        print!("Pick a channel [1-3 or name, Enter keeps {selected}]: ");
        std::io::stdout().flush()?;
        // EOF behaves like Enter: leave things as they are.
        match ch::parse_pick(&read_line().unwrap_or_default()) {
            Pick::Keep => return Ok(()),
            Pick::Set(picked) => {
                remember_channel(picked)?;
                let view = views
                    .iter()
                    .find(|v| v.channel == picked)
                    .context("channel missing from the listing")?;
                println!();
                print!("{}", ch::render_switched(view));
                return Ok(());
            }
            Pick::Invalid => {
                println!("Enter 1, 2, 3, stable, beta or alpha (or nothing to keep {selected}).")
            }
        }
    }
}

fn installed_version() -> semver::Version {
    semver::Version::parse(CURRENT_VERSION).expect("CARGO_PKG_VERSION is SemVer")
}

pub(crate) async fn run_update(req: Request) -> Result<UpdateOutcome> {
    let judged = update_core::judged_exe();
    let method = InstallMethod::of_exe(&judged, update_pkg::owner);

    // `--channel X` sticks, even where this binary cannot install anything itself.
    if let Some(on) = req.channel {
        remember_channel(on)?;
    }
    let on = req.channel.unwrap_or_else(channel::current);

    // A dev build or an `atlas link` symlink is never overwritten by a release, whatever
    // installed it: the one rule the `atlas dev` prompt uses (`update_core::is_dev_binary`).
    // `--check` only reports, so it is allowed.
    if !req.check_only && update_core::is_dev_binary() {
        bail!(
            "this atlas is a dev build or linked to one ({}); `atlas update` only replaces release installs. \
             `atlas unlink` goes back to the release build",
            judged.display()
        );
    }
    // `atlas link` made ~/.atlas/bin/atlas a symlink to target/debug/atlas: replacing
    // "the binary" would overwrite the dev build, not install a release.
    if !req.check_only
        && let installed = atlas_core::paths::atlas_home()
            .join("bin")
            .join(atlas_core::paths::exe("atlas"))
        && installed
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    {
        bail!(
            "atlas is linked to a dev build ({}); run `atlas unlink` first",
            installed.display()
        );
    }

    let current = installed_version();
    if !req.json {
        println!("\x1b[1;34m◆\x1b[0m Checking for updates...");
        println!("  Current: v{CURRENT_VERSION}");
        println!("  Channel: {on}");
        println!();
    }

    // A failed check keeps the cache as it was (plus a backoff stamp): a network error is
    // neither "current" nor "ahead", and must not silence the notice.
    let tag = match fetch_latest_tag(on).await {
        Ok(tag) => tag,
        Err(e) => {
            save_cache(&load_cache().failed_for(on, now_secs()));
            return Err(e.context(
                "Could not check for updates. Check your network connection (needs curl).",
            ));
        }
    };

    if req.json {
        // `--json` is a report: it never installs.
        let report = ch::check_json(&current, on, Some(&tag));
        let report = managed_report(&method, &current, &tag, report);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(UpdateOutcome::Unchanged);
    }

    let (offered, decision) =
        ch::offer(&current, &tag).with_context(|| format!("Bad remote version '{tag}'"))?;
    let managed = !matches!(
        method,
        InstallMethod::ShellInstaller | InstallMethod::Manual
    );

    match decision {
        Decision::Current => {
            println!("{}", ch::describe(&current, &tag, on, req.downgrade));
            // Clear any stale cache entry
            save_cache(&load_cache().checked_for(on, None));
            // The app may still lag behind (an earlier sync failed or was skipped). Homebrew
            // and packages own the CLI, not the app: the sync is still ours to do.
            if !req.check_only {
                run_self(&["desktop", "--sync"]);
            }
            return Ok(UpdateOutcome::Unchanged);
        }
        Decision::Ahead => {
            save_cache(&load_cache().checked_for(on, None));
            println!("{}", ch::describe(&current, &tag, on, req.downgrade));
            if !req.downgrade || req.check_only {
                return Ok(UpdateOutcome::Unchanged);
            }
            if managed {
                bail!(
                    "--downgrade needs an Atlas installed by the shell installer; another tool manages this one"
                );
            }
            if !confirm_downgrade(&current, &offered, on, req.yes)? {
                println!("Cancelled; nothing was changed.");
                return Ok(UpdateOutcome::Unchanged);
            }
        }
        Decision::Upgrade => {
            if req.downgrade {
                bail!(
                    "--downgrade only applies when the {on} channel offers an older build than v{current}; \
                     v{offered} is newer, run `atlas update`"
                );
            }
            println!("  Latest:  v{offered}");
            println!("  Changelog: https://github.com/{REPO}/releases/tag/{tag}");
            println!();
            if req.check_only {
                println!(
                    "\x1b[1;33m◆\x1b[0m {}",
                    ch::describe(&current, &tag, on, false)
                );
                if let InstallMethod::SystemPackage(Some(pm)) = &method {
                    println!(
                        "  Installed by {}: `atlas update` downloads, verifies and installs the package.",
                        pm.name()
                    );
                }
                return Ok(UpdateOutcome::Unchanged);
            }
        }
    }
    let latest = offered.to_string();

    match &method {
        InstallMethod::Homebrew => {
            println!("\x1b[1;34m◆\x1b[0m Installed via Homebrew — run:");
            println!("  brew upgrade atlas");
            println!("  (latest: v{latest})");
            update_core::managed_event("homebrew");
            return Ok(UpdateOutcome::Managed);
        }
        InstallMethod::SystemPackage(Some(pm)) => {
            return install_package(*pm, &latest, &tag, req.yes).await;
        }
        InstallMethod::SystemPackage(None) => {
            println!(
                "\x1b[1;34m◆\x1b[0m Installed by a system package, but no package manager (pacman, dpkg, rpm) was found."
            );
            println!(
                "  Install the v{latest} package for your distribution: https://github.com/{REPO}/releases/tag/{tag}"
            );
            update_core::managed_event("package");
            return Ok(UpdateOutcome::Managed);
        }
        InstallMethod::ShellInstaller | InstallMethod::Manual => {}
    }

    let exe = env::current_exe().context("cannot locate the running atlas binary")?;

    // Every artifact this update replaces must be writable *before* anything is: the CLI's
    // directory (new binary and `.atlas.prev`) and the installed app (`atlas desktop --sync`).
    // Otherwise the CLI would be replaced, the app sync would fail and the CLI be rolled back.
    let app = atlas_core::paths::installed_app();
    let mut artifacts: Vec<(&str, &std::path::Path)> = vec![("Atlas CLI", exe.as_path())];
    if let Some(app) = app.as_deref() {
        artifacts.push(("Atlas Desktop", app));
    }
    let blocked = update_core::unwritable(&artifacts);
    if !blocked.is_empty() {
        bail!(
            "cannot update, nothing was changed: {}. Fix the permissions, or reinstall into your home \
             with the shell installer (`sudo atlas update` would leave root-owned files there)",
            blocked.join("; ")
        );
    }

    // Held until this function returns; the kernel frees it if we die. The spawned
    // `atlas desktop --sync` must not wait on us, hence the env marker on that child.
    let _lock = update_core::UpdateLock::acquire()?;

    println!("\x1b[1;34m◆\x1b[0m Downloading v{latest}...");
    update_core::phase(update_core::Phase::Download);

    let platform = current_platform();
    let tarball = format!("atlas-{latest}-{platform}.tar.gz");
    let tmp_dir = tempfile::Builder::new().prefix("atlas-update-").tempdir()?;

    let tarball_path = download_verified(
        &format!("v{latest}"),
        &tarball,
        tmp_dir.path(),
        update_core::bytes,
    )
    .with_context(|| format!("Download failed. Releases: https://github.com/{REPO}/releases"))?;

    // `download_verified` checks the sha256 before returning.
    update_core::phase(update_core::Phase::Verify);
    println!("\x1b[1;32m✓\x1b[0m Checksum verified");

    update_core::phase(update_core::Phase::InstallCli);
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
    let backup = update_core::CliBackup::save(&exe)?;
    if let Err(e) = self_replace::self_replace(tmp_bin.path()) {
        backup.discard();
        return Err(e).context("Failed to replace the atlas binary");
    }

    // Clear cache
    save_cache(&load_cache().checked_for(on, None));

    println!();
    println!("\x1b[1;32m✓\x1b[0m Updated to v{latest}");
    println!();

    settle(&exe, backup, CURRENT_VERSION, &latest)?;

    // Last step: the terminal host outlives updates, so it is usually older than the CLI and
    // app by now. The new binary upgrades it live, restarts it when it is empty, or says
    // exactly what is stale; nothing here ends a running terminal without being asked.
    reconcile_host_with_new_binary(&exe);

    Ok(UpdateOutcome::Installed)
}

/// The `--check --json` report for an install another tool owns.
fn managed_report(
    method: &InstallMethod,
    current: &semver::Version,
    tag: &str,
    report: serde_json::Value,
) -> serde_json::Value {
    match method {
        InstallMethod::Homebrew => ch::managed_json(report, "homebrew", None, "brew upgrade atlas"),
        InstallMethod::SystemPackage(pm) => {
            let command = match (*pm, ch::offer(current, tag)) {
                (Some(pm), Some((v, _))) => pm.manual_command(&v.to_string()),
                _ => "atlas update".to_string(),
            };
            ch::managed_json(report, "package", pm.map(PackageManager::name), &command)
        }
        InstallMethod::ShellInstaller | InstallMethod::Manual => report,
    }
}

/// Settle a CLI update once the binary at `exe` is the new one and `backup` holds the old one:
/// bring the app to the new version, then refresh skills and agent hooks with whichever binary
/// stays on disk. Order matters. The refresh used to run first, so a failed app sync rolled
/// the CLI back but left skills, MCP entries and hooks on the new version.
fn settle(
    exe: &std::path::Path,
    backup: update_core::CliBackup,
    from: &str,
    to: &str,
) -> Result<()> {
    let synced = sync_desktop_with_new_binary(exe);
    let result = backup.finish(synced, from, to);
    println!("\x1b[1;34m◆\x1b[0m Refreshing skills...");
    refresh_with(exe);
    result
}

/// `#144`: update a package-managed install through its package manager. The asset and its
/// checksum are downloaded and verified *before* anything asks for privileges; then one
/// escalated package-manager command installs it, under the update lock, and the installed
/// binary refreshes skills and agent hooks.
async fn install_package(
    pm: PackageManager,
    latest: &str,
    tag: &str,
    yes: bool,
) -> Result<UpdateOutcome> {
    let exe = update_core::judged_exe();
    let asset = pm.asset(latest);
    println!("\x1b[1;34m◆\x1b[0m Downloading {asset}...");
    update_core::phase(update_core::Phase::Download);
    let tmp_dir = tempfile::Builder::new().prefix("atlas-update-").tempdir()?;
    let file =
        download_verified(tag, &asset, tmp_dir.path(), update_core::bytes).with_context(|| {
            format!("Download failed. Releases: https://github.com/{REPO}/releases/tag/{tag}")
        })?;
    update_core::phase(update_core::Phase::Verify);
    println!("\x1b[1;32m✓\x1b[0m Checksum verified");

    let tty = interactive();
    let has = |n: &str| atlas_core::paths::which(n).is_some();
    let is_root = update_pkg::is_root();
    let argv = update_pkg::command_line(update_pkg::escalation(is_root, tty, has)?, pm, &file);
    println!();
    println!("  This installs v{latest} with {} as root:", pm.name());
    println!("    {}", argv.join(" "));
    if !yes {
        if !tty {
            bail!(
                "installing a system package needs confirmation: run it in a terminal, or pass --yes"
            );
        }
        print!("Install v{latest} now? [y/N] ");
        std::io::stdout().flush()?;
        if !ch::parse_confirm(&read_line().unwrap_or_default()) {
            println!("Cancelled; nothing was changed.");
            return Ok(UpdateOutcome::Unchanged);
        }
    }

    // Nothing above changed the machine. From here a second update (another terminal, the
    // app) must wait: the package manager and the refresh are one critical section.
    let _lock = update_core::UpdateLock::acquire()?;
    update_core::phase(update_core::Phase::InstallCli);
    println!("\x1b[1;34m◆\x1b[0m Installing with {}...", pm.name());
    update_pkg::run_install(&argv)?;
    save_cache(&load_cache().checked_for(channel::current(), None));
    println!();
    println!("\x1b[1;32m✓\x1b[0m Updated to v{latest}");
    println!();

    // The package manager replaced the file at `exe`; the *installed* binary carries the new
    // skills and hooks, this process still holds the old ones.
    println!("\x1b[1;34m◆\x1b[0m Refreshing skills...");
    refresh_with(&exe);
    // The app is package-owned too (`atlas-desktop` in the same package set); nothing to sync
    // unless a user-level app exists, which `desktop --sync` handles.
    update_core::phase(update_core::Phase::UpdateApp);
    sync_desktop_best_effort(&exe);
    reconcile_host_with_new_binary(&exe);
    Ok(UpdateOutcome::Installed)
}

/// `atlas hostd --reconcile` of the new binary. A failure is reported, not fatal: the CLI and
/// the app are already on the new version, and rolling them back would not fix the host.
fn reconcile_host_with_new_binary(exe: &std::path::Path) {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["hostd", "--reconcile"]);
    match cmd.status() {
        Ok(st) if st.success() => {}
        Ok(st) => {
            update_core::host_event("failed", 0);
            eprintln!(
                "\x1b[33m◆\x1b[0m Terminal host check failed ({st}); run `atlas hostd --check`"
            );
        }
        Err(e) => {
            update_core::host_event("failed", 0);
            eprintln!(
                "\x1b[33m◆\x1b[0m Terminal host check failed: {e}; run `atlas hostd --check`"
            );
        }
    }
}

/// `atlas desktop --sync` of the new binary; unlike the integrations refresh it is fatal.
fn sync_desktop_with_new_binary(exe: &std::path::Path) -> Result<()> {
    let st = std::process::Command::new(exe)
        .args(["desktop", "--sync"])
        .env(update_core::HOLDS_LOCK_ENV, "1")
        .status()
        .context("cannot run the new atlas")?;
    if st.success() {
        Ok(())
    } else {
        bail!("`atlas desktop --sync` failed ({st})")
    }
}

/// `atlas desktop --sync` of the installed binary after a package install: the app follows
/// the CLI when a user-level one exists. A failure is reported, never fatal (the package
/// manager already committed the CLI; there is no CLI backup to restore).
fn sync_desktop_best_effort(exe: &std::path::Path) {
    if let Err(e) = sync_desktop_with_new_binary(exe) {
        eprintln!(
            "\x1b[33m◆\x1b[0m Atlas Desktop could not follow: {e:#} (non-fatal); run `atlas desktop --install`"
        );
    }
}

/// Run `exe` to refresh the skills and, when already set up, the OMP / Claude Code / Codex
/// integrations (hooks, MCP entry), all embedded in that binary: the running process still
/// holds its own (old) copy. Failures are non-fatal.
fn refresh_with(exe: &std::path::Path) {
    for args in [
        &["skills", "install"][..],
        &["setup", "omp", "--refresh"][..],
        &["setup", "claude-code", "--refresh"][..],
        &["setup", "codex", "--refresh"][..],
    ] {
        run_self_with(exe, args);
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
            .is_some_and(|n| n.to_string_lossy() == atlas_core::paths::exe("atlas"))
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
    use atlas_core::channel::Channel;

    #[test]
    fn test_current_platform() {
        let p = current_platform();
        assert!(p.contains("apple-darwin") || p.contains("linux") || p.contains("windows"));
        assert!(p.starts_with("aarch64") || p.starts_with("x86_64"));
    }

    #[test]
    fn test_install_method_of_this_binary() {
        let m = InstallMethod::of_exe(&update_core::judged_exe(), update_pkg::owner);
        assert!(matches!(
            m,
            InstallMethod::Manual
                | InstallMethod::ShellInstaller
                | InstallMethod::Homebrew
                | InstallMethod::SystemPackage(_)
        ));
    }

    /// `atlas update` must never try to overwrite a file pacman/apt/dnf own (it would fail without root,
    /// or worse, succeed under sudo and leave the package database lying).
    #[test]
    fn a_binary_in_usr_bin_is_a_system_package_on_linux_only() {
        let pkg = if cfg!(target_os = "linux") {
            InstallMethod::SystemPackage(None)
        } else {
            InstallMethod::Manual
        };
        assert_eq!(InstallMethod::of_path("/usr/bin/atlas"), pkg);
        assert_eq!(
            InstallMethod::of_path("/home/u/.atlas/bin/atlas"),
            InstallMethod::ShellInstaller
        );
        assert_eq!(
            InstallMethod::of_path("/usr/local/bin/atlas"),
            InstallMethod::Manual
        );
        assert_eq!(
            InstallMethod::of_path("/opt/homebrew/bin/atlas"),
            InstallMethod::Homebrew
        );
        // The shell installer's place on Windows (install.ps1), backslashes and all.
        assert_eq!(
            InstallMethod::of_path(r"C:\Users\u\.atlas\bin\atlas.exe"),
            InstallMethod::ShellInstaller
        );
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
            channel: Some("beta".into()),
            failed_at: None,
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
        let refreshed = cache.checked_for(Channel::Stable, Some("1.0.0".into()));
        assert_eq!(refreshed.dismissed, cache.dismissed);
        assert_eq!(refreshed.available_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn staleness_uses_the_6h_interval() {
        let cache = UpdateCache {
            last_checked: 100,
            ..Default::default()
        };
        assert!(!cache.is_stale_for(Channel::Stable, 100 + CHECK_INTERVAL_SECS - 1));
        assert!(cache.is_stale_for(Channel::Stable, 100 + CHECK_INTERVAL_SECS));
        assert!(
            UpdateCache::default().is_stale_for(Channel::Stable, now_secs()),
            "never checked"
        );
    }

    #[test]
    fn newer_than_compares_semver_not_strings() {
        assert_eq!(newer_than("0.4.1", "0.10.0").as_deref(), Some("0.10.0"));
        assert_eq!(newer_than("0.4.1", "0.4.1"), None);
        assert_eq!(newer_than("0.4.1", "0.4.0"), None);
        assert_eq!(newer_than("0.4.1", "garbage"), None);
    }

    // ── channels: clap ────────────────────────────────────────────────────────

    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        update: UpdateArgs,
    }

    fn parse(args: &[&str]) -> Result<UpdateArgs, clap::Error> {
        Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied())).map(|c| c.update)
    }

    fn ver(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn update_flags_parse() {
        let a = parse(&[]).unwrap();
        assert!(!a.check && !a.json && !a.downgrade && !a.yes);
        assert!(a.channel.is_none() && a.command.is_none());

        let a = parse(&["--check", "--channel", "beta", "--json"]).unwrap();
        assert!(a.check && a.json);
        assert_eq!(a.channel, Some(Channel::Beta));

        let a = parse(&["--downgrade", "--yes"]).unwrap();
        assert!(a.downgrade && a.yes);

        let a = parse(&["--channel", "ALPHA"]).unwrap();
        assert_eq!(
            a.channel,
            Some(Channel::Alpha),
            "names are case-insensitive"
        );
    }

    #[test]
    fn invalid_channel_is_an_error_that_lists_the_valid_names() {
        let e = parse(&["--channel", "nightly"]).unwrap_err().to_string();
        assert!(e.contains("nightly"), "{e}");
        for name in ["stable", "beta", "alpha"] {
            assert!(e.contains(name), "{e} should list {name}");
        }
    }

    #[test]
    fn channel_subcommand_parses() {
        let a = parse(&["channel"]).unwrap();
        assert!(matches!(
            a.command,
            Some(UpdateCmd::Channel {
                name: None,
                json: false
            })
        ));
        let a = parse(&["channel", "alpha", "--json"]).unwrap();
        assert!(matches!(
            a.command,
            Some(UpdateCmd::Channel {
                name: Some(Channel::Alpha),
                json: true
            })
        ));
        assert!(parse(&["channel", "nightly"]).is_err());
    }

    // ── channels: cache and background checks ─────────────────────────────────

    #[test]
    fn cache_written_before_channels_loads_and_counts_as_stable() {
        let old: UpdateCache =
            serde_json::from_str(r#"{"last_checked":1,"available_version":"1.2.0"}"#).unwrap();
        assert_eq!(old.channel, None);
        assert_eq!(old.available_version.as_deref(), Some("1.2.0"));
        assert!(old.valid_for(Channel::Stable), "old caches were stable's");
        assert!(!old.valid_for(Channel::Beta));
        assert!(!old.valid_for(Channel::Alpha));
    }

    #[test]
    fn cache_is_ignored_when_the_channel_changed() {
        let c = UpdateCache {
            last_checked: 10_000,
            ..Default::default()
        }
        .checked_for(Channel::Beta, Some("1.4.0-beta.1".into()));
        assert_eq!(c.channel.as_deref(), Some("beta"));
        assert!(c.valid_for(Channel::Beta));
        assert!(!c.valid_for(Channel::Stable));
        // Fresh by age, but computed for another channel: stale, and its offer is not reported.
        let now = c.last_checked;
        assert!(c.is_stale_for(Channel::Stable, now));
        assert!(!c.is_stale_for(Channel::Beta, now));
        assert_eq!(c.available_for(&ver("1.3.0"), Channel::Stable), None);
        assert_eq!(
            c.available_for(&ver("1.3.0"), Channel::Beta).as_deref(),
            Some("1.4.0-beta.1")
        );
    }

    #[test]
    fn old_cache_is_stale_for_a_non_stable_channel() {
        let old: UpdateCache =
            serde_json::from_str(r#"{"last_checked":1,"available_version":"1.2.0"}"#).unwrap();
        assert!(!old.is_stale_for(Channel::Stable, 2));
        assert!(old.is_stale_for(Channel::Beta, 2));
        assert_eq!(old.available_for(&ver("1.0.0"), Channel::Beta), None);
        assert_eq!(
            old.available_for(&ver("1.0.0"), Channel::Stable).as_deref(),
            Some("1.2.0")
        );
    }

    /// Pre-GA default is Alpha. A 1.4.0 Stable install that never chose a channel has an old
    /// cache (written by Stable-only atlas versions, `channel: None`) and no channel file: that
    /// cache is not the default channel's, so the next check refetches, and the newest alpha is
    /// then reported as an upgrade (never as a downgrade or "current").
    #[test]
    fn a_stable_install_that_never_chose_a_channel_is_offered_the_newest_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let on = atlas_core::channel::effective(None, channel::read_from(&dir.path().join("none")));
        assert_eq!(on, Channel::Alpha);
        let old: UpdateCache =
            serde_json::from_str(r#"{"last_checked":1,"available_version":"1.4.0"}"#).unwrap();
        assert!(old.is_stale_for(on, 2), "a Stable-era cache is not Alpha's");
        assert_eq!(old.available_for(&ver("1.3.0"), on), None);
        let fresh = old.checked_for(on, newer_than("1.4.0", "1.5.0-alpha.2"));
        assert_eq!(fresh.channel.as_deref(), Some("alpha"));
        assert_eq!(
            fresh.available_for(&ver("1.4.0"), on).as_deref(),
            Some("1.5.0-alpha.2")
        );
        // Stable's tag is older than the alpha that runs after the update: never offered back.
        assert_eq!(background_available(&ver("1.5.0-alpha.2"), "1.4.0"), None);
    }

    #[test]
    fn changing_channel_keeps_the_dismissal_of_its_version() {
        let mut c = UpdateCache::default();
        c.dismiss("1.4.0-beta.1", 5);
        let c = c.checked_for(Channel::Beta, Some("1.4.0-beta.1".into()));
        assert!(c.is_dismissed("1.4.0-beta.1", 6));
        assert!(!c.is_dismissed("1.4.0-beta.2", 6));
    }

    #[test]
    fn background_check_never_reports_ahead_or_current() {
        let installed = ver("1.5.0-alpha.1");
        // Stable offers an older build than the alpha that runs: not an "update".
        assert_eq!(background_available(&installed, "1.3.0"), None);
        assert_eq!(background_available(&installed, "1.5.0-alpha.1"), None);
        assert_eq!(background_available(&installed, "garbage"), None);
        assert_eq!(
            background_available(&installed, "1.5.0-alpha.2").as_deref(),
            Some("1.5.0-alpha.2")
        );
        assert_eq!(
            background_available(&ver("1.3.0"), "1.4.0").as_deref(),
            Some("1.4.0")
        );
    }

    #[test]
    fn newer_than_uses_precedence_not_string_or_build_metadata() {
        assert_eq!(
            newer_than("1.4.0-alpha.2", "1.4.0-alpha.10").as_deref(),
            Some("1.4.0-alpha.10")
        );
        assert_eq!(
            newer_than("1.4.0-alpha.1", "1.4.0").as_deref(),
            Some("1.4.0")
        );
        assert_eq!(newer_than("1.4.0", "1.4.0-rc.1"), None);
        assert_eq!(
            newer_than("1.4.0+a", "1.4.0+b"),
            None,
            "build metadata ignored"
        );
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    use std::path::Path;

    fn claims(pm: PackageManager) -> impl FnOnce(&Path) -> Option<PackageManager> {
        move |_| Some(pm)
    }
    fn nobody(_: &Path) -> Option<PackageManager> {
        None
    }

    /// The detection table of #144: the package database decides, the path rule is the fallback.
    #[test]
    fn install_method_detection_table() {
        use InstallMethod::*;
        let p = |s: &str| std::path::PathBuf::from(s);
        // A database claims a binary even outside /usr/bin (custom prefix).
        assert_eq!(
            InstallMethod::of_exe(&p("/opt/atlas/bin/atlas"), claims(PackageManager::Apt)),
            SystemPackage(Some(PackageManager::Apt))
        );
        assert_eq!(
            InstallMethod::of_exe(&p("/usr/local/bin/atlas"), claims(PackageManager::Pacman)),
            SystemPackage(Some(PackageManager::Pacman))
        );
        // Nobody claims it: a hand-copied binary stays Manual.
        assert_eq!(
            InstallMethod::of_exe(&p("/usr/local/bin/atlas"), nobody),
            Manual
        );
        // Homebrew and the shell installer are decided by path, never by a database.
        assert_eq!(
            InstallMethod::of_exe(&p("/opt/homebrew/bin/atlas"), claims(PackageManager::Apt)),
            Homebrew
        );
        assert_eq!(
            InstallMethod::of_exe(&p("/home/u/.atlas/bin/atlas"), claims(PackageManager::Apt)),
            ShellInstaller
        );
        // A checkout's build output and a cargo install are never package-owned.
        assert_eq!(
            InstallMethod::of_exe(
                &p("/home/u/src/atlas/target/debug/atlas"),
                claims(PackageManager::Apt)
            ),
            Manual
        );
        assert_eq!(
            InstallMethod::of_exe(&p("/home/u/.cargo/bin/atlas"), claims(PackageManager::Apt)),
            Manual
        );
        // /usr/bin on Linux is a system package even when no database answers.
        if cfg!(target_os = "linux") {
            assert!(matches!(
                InstallMethod::of_exe(&p("/usr/bin/atlas"), nobody),
                SystemPackage(_)
            ));
            assert_eq!(
                InstallMethod::of_exe(&p("/usr/bin/atlas"), claims(PackageManager::Dnf)),
                SystemPackage(Some(PackageManager::Dnf))
            );
        }
    }

    #[test]
    fn the_notice_names_the_command_for_each_install_method() {
        let apt = InstallMethod::SystemPackage(Some(PackageManager::Apt));
        let line = update_command(&apt, "1.6.0");
        assert!(line.starts_with("atlas update"), "{line}");
        assert!(
            line.contains("sudo apt install ./atlas_1.6.0_amd64.deb"),
            "{line}"
        );
        assert_eq!(
            update_command(&InstallMethod::Homebrew, "1.6.0"),
            "brew upgrade atlas"
        );
        assert_eq!(
            update_command(&InstallMethod::ShellInstaller, "1.6.0"),
            "atlas update"
        );
        assert_eq!(
            update_command(&InstallMethod::Manual, "1.6.0"),
            "atlas update"
        );
        assert_eq!(
            update_command(&InstallMethod::SystemPackage(None), "1.6.0"),
            "atlas update"
        );
    }

    // ── G7: the cache is written atomically and a damaged file keeps the dismissal ──

    #[test]
    fn save_cache_leaves_no_temp_file_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".update_check");
        let mut c = UpdateCache::default().checked_for(Channel::Alpha, Some("9.9.9".into()));
        c.dismiss("9.9.9", 5);
        save_cache_at(&path, &c);
        assert_eq!(load_cache_at(&path), c);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [".update_check"], "{names:?}");
    }

    #[test]
    fn a_truncated_cache_still_counts_as_never_checked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c");
        std::fs::write(&path, r#"{"last_checked":17,"available_ver"#).unwrap();
        assert_eq!(load_cache_at(&path), UpdateCache::default());
        assert!(load_cache_at(&path).is_stale_for(Channel::Alpha, 20));
    }

    /// The reported loss: a cache with a good dismissal but one field of the wrong type used to
    /// reset to `default()`, so "No" to the prompt was forgotten and `atlas dev` asked again.
    #[test]
    fn a_cache_that_does_not_parse_keeps_its_dismissal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c");
        std::fs::write(
            &path,
            r#"{"last_checked":"yesterday","available_version":"9.9.9",
                "dismissed":{"version":"9.9.9","until":4102444800}}"#,
        )
        .unwrap();
        let c = load_cache_at(&path);
        assert!(c.is_dismissed("9.9.9", 1_000), "{c:?}");
        assert_eq!(c.available_version, None, "the rest is not trusted");
        assert!(
            c.is_stale_for(Channel::Alpha, 1_000),
            "and the next check refetches"
        );
    }

    #[test]
    fn garbage_or_a_missing_cache_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_cache_at(&dir.path().join("none")),
            UpdateCache::default()
        );
        let p = dir.path().join("g");
        std::fs::write(&p, "not json at all").unwrap();
        assert_eq!(load_cache_at(&p), UpdateCache::default());
    }

    // ── G14: a failed check is stamped with a backoff ──

    #[test]
    fn a_failed_check_backs_off_and_keeps_what_was_known() {
        let known = UpdateCache {
            last_checked: 1_000,
            available_version: Some("9.9.9".into()),
            dismissed: None,
            channel: Some("alpha".into()),
            failed_at: None,
        };
        let now = 1_000 + CHECK_INTERVAL_SECS + 10; // stale: a check is due
        assert!(known.is_stale_for(Channel::Alpha, now));
        let failed = known.failed_for(Channel::Alpha, now);
        assert_eq!(
            failed.available_version.as_deref(),
            Some("9.9.9"),
            "notice keeps working"
        );
        assert_eq!(failed.last_checked, 1_000, "not pretended fresh");
        assert!(
            !failed.is_stale_for(Channel::Alpha, now + 1),
            "no retry inside the backoff"
        );
        assert!(!failed.is_stale_for(Channel::Alpha, now + FAILED_BACKOFF_SECS - 1));
        assert!(
            failed.is_stale_for(Channel::Alpha, now + FAILED_BACKOFF_SECS),
            "then it retries"
        );
    }

    #[test]
    fn a_successful_check_clears_the_failure() {
        let failed = UpdateCache::default().failed_for(Channel::Alpha, 50);
        assert_eq!(failed.failed_at, Some(50));
        assert_eq!(failed.checked_for(Channel::Alpha, None).failed_at, None);
    }

    #[test]
    fn a_failure_never_carries_another_channels_offer() {
        let beta = UpdateCache::default().checked_for(Channel::Beta, Some("2.0.0-beta.1".into()));
        let failed = beta.failed_for(Channel::Alpha, 99);
        assert_eq!(failed.available_version, None);
        assert_eq!(failed.channel.as_deref(), Some("alpha"));
        assert_eq!(failed.available_for(&ver("1.0.0"), Channel::Alpha), None);
    }

    #[test]
    fn a_failure_keeps_the_dismissal() {
        let mut c = UpdateCache::default();
        c.dismiss("2.0.0", 10);
        assert!(c.failed_for(Channel::Alpha, 20).is_dismissed("2.0.0", 21));
    }

    fn ver(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    // ── G11: the JSON report names the owner and what to run ──

    #[test]
    fn managed_report_for_each_install_method() {
        let cur = ver("1.0.0");
        let base = ch::check_json(&cur, Channel::Stable, Some("v1.1.0"));
        let pkg = managed_report(
            &InstallMethod::SystemPackage(Some(PackageManager::Pacman)),
            &cur,
            "v1.1.0",
            base.clone(),
        );
        assert_eq!(pkg["action"], "package");
        assert_eq!(pkg["manager"], "pacman");
        assert_eq!(
            pkg["command"],
            PackageManager::Pacman.manual_command("1.1.0")
        );
        let brew = managed_report(&InstallMethod::Homebrew, &cur, "v1.1.0", base.clone());
        assert_eq!(brew["action"], "managed");
        let own = managed_report(&InstallMethod::ShellInstaller, &cur, "v1.1.0", base.clone());
        assert_eq!(own, base, "self-installed reports are untouched");
    }
}

#[cfg(all(test, unix))]
mod settle_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in for an atlas binary: logs `<name> <args>` to `log`, and fails `desktop --sync`
    /// when `fail_sync`.
    fn stub(path: &std::path::Path, name: &str, log: &std::path::Path, fail_sync: bool) {
        let fail = if fail_sync {
            "[ \"$1 $2\" = 'desktop --sync' ] && exit 1\n"
        } else {
            ""
        };
        std::fs::write(
            path,
            format!(
                "#!/bin/sh\necho \"{name} $*\" >> '{}'\n{fail}exit 0\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn lines(log: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// The incident (G1): the CLI is rolled back when `atlas desktop --sync` fails, but skills,
    /// MCP entries and hooks had already been refreshed by the *new* binary, so the CLI that
    /// stays was older than what it left on disk. The refresh must come after, by whichever
    /// binary stays.
    #[test]
    fn a_failed_app_sync_rolls_back_and_the_old_binary_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls");
        let exe = dir.path().join("atlas");
        stub(&exe, "old", &log, false);
        let backup = update_core::CliBackup::save(&exe).unwrap();
        stub(&exe, "new", &log, true); // the freshly installed CLI; its app sync fails

        let e = settle(&exe, backup, "1.0.0", "1.1.0").unwrap_err();
        assert!(format!("{e:#}").contains("stays on v1.0.0"), "{e:#}");

        let calls = lines(&log);
        assert_eq!(calls[0], "new desktop --sync", "{calls:?}");
        assert!(
            !calls
                .iter()
                .any(|c| c.starts_with("new skills") || c.starts_with("new setup")),
            "the rolled-back binary's replacement must not have refreshed anything: {calls:?}"
        );
        assert!(
            calls.contains(&"old skills install".to_string()),
            "{calls:?}"
        );
        assert!(
            calls.contains(&"old setup omp --refresh".to_string()),
            "{calls:?}"
        );
    }

    #[test]
    fn a_successful_sync_refreshes_with_the_new_binary_afterwards() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls");
        let exe = dir.path().join("atlas");
        stub(&exe, "old", &log, false);
        let backup = update_core::CliBackup::save(&exe).unwrap();
        stub(&exe, "new", &log, false);

        settle(&exe, backup, "1.0.0", "1.1.0").unwrap();

        let calls = lines(&log);
        assert_eq!(calls[0], "new desktop --sync", "{calls:?}");
        let skills = calls
            .iter()
            .position(|c| c == "new skills install")
            .unwrap();
        assert!(skills > 0, "refresh follows the sync: {calls:?}");
        assert!(!calls.iter().any(|c| c.starts_with("old ")), "{calls:?}");
        assert!(!dir.path().join(".atlas.prev").exists(), "backup discarded");
    }

    #[test]
    fn when_the_rollback_itself_fails_the_binary_on_disk_still_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls");
        let exe = dir.path().join("atlas");
        stub(&exe, "old", &log, false);
        let backup = update_core::CliBackup::save(&exe).unwrap();
        stub(&exe, "new", &log, true);
        std::fs::remove_file(dir.path().join(".atlas.prev")).unwrap(); // nothing to restore

        let e = settle(&exe, backup, "1.0.0", "1.1.0").unwrap_err();
        assert!(
            format!("{e:#}").contains("could not be rolled back"),
            "{e:#}"
        );
        // The new CLI is what is left, so it is the one whose skills match.
        assert!(
            lines(&log).contains(&"new skills install".to_string()),
            "{:?}",
            lines(&log)
        );
    }
}
