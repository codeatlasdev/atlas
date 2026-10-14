//! Versions, dev links and self-update. All installs go through the CLI
//! (`atlas update` → `atlas desktop --sync`), so the app and the terminal share
//! one code path for downloading, verifying and replacing binaries.

use atlas_core::sys::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use atlas_core::channel::{self, Action, Channel};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::cmds::atlas_bin;

const REPO: &str = "codeatlasdev/atlas";

/// Version this app reports as installed. Debug builds honour `ATLAS_DEBUG_FAKE_VERSION`
/// so the "behind" UI can be exercised without a real older build.
fn app_version() -> String {
    #[cfg(debug_assertions)]
    if let Ok(v) = std::env::var("ATLAS_DEBUG_FAKE_VERSION")
        && semver::Version::parse(&v).is_ok()
    {
        return v;
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// An install is running in this process (any window, the tray): a second one is refused.
static INSTALLING: AtomicBool = AtomicBool::new(false);

struct InstallGuard;

impl InstallGuard {
    fn take() -> Result<Self, String> {
        INSTALLING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "An update is already running".to_string())
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

/// Structured progress of an install, parsed from the CLI's `@@atlas …` lines and sent as
/// `updates://progress`. The phase ids match `Phase::id` in `atlas-cli`'s `update_core`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Progress {
    Phase { id: String },
    Bytes { done: u64, total: u64 },
    Rollback { state: String },
    Host { state: String, live: u64 },
}

/// `@@atlas phase download` / `@@atlas bytes 10 100` / `@@atlas rollback restored`.
fn parse_progress(line: &str) -> Option<Progress> {
    let mut it = line.strip_prefix("@@atlas ")?.split_whitespace();
    match it.next()? {
        "phase" => Some(Progress::Phase {
            id: it.next()?.to_string(),
        }),
        "bytes" => Some(Progress::Bytes {
            done: it.next()?.parse().ok()?,
            total: it.next()?.parse().ok()?,
        }),
        "rollback" => Some(Progress::Rollback {
            state: it.next()?.to_string(),
        }),
        "host" => Some(Progress::Host {
            state: it.next()?.to_string(),
            live: it.next().and_then(|n| n.parse().ok()).unwrap_or(0),
        }),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct CliInfo {
    path: Option<String>,
    version: Option<String>,
    /// `atlas link`: `~/.atlas/bin/atlas` → the repo's `target/debug/atlas`.
    linked_to: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    /// The `.app` this process runs from (None when running `cargo tauri dev`).
    bundle: Option<String>,
    /// The installed `Atlas.app` is a symlink to a dev bundle (`atlas link --desktop`).
    linked_to: Option<String>,
    dev: bool,
}

#[derive(Serialize)]
pub struct SystemInfo {
    app: AppInfo,
    cli: CliInfo,
}

#[derive(Serialize)]
pub struct Release {
    version: String,
    url: String,
    published_at: String,
    notes: String,
    /// The channel this release was cut for (`stable`/`beta`/`alpha`; `stable` for a label
    /// Atlas does not ship, which no channel offers anyway).
    channel: String,
    /// What installing `version` is for this app: `update`, `downgrade` or `none`.
    action: Action,
    /// `action == downgrade`: the offer is older than what runs (never installed on its own).
    downgrade: bool,
    /// Installed CLI is older than `version` (false when unknown, unparseable or not older).
    cli_behind: bool,
    /// This app is older than `version`.
    app_behind: bool,
    /// Installed CLI is newer than `version`.
    cli_ahead: bool,
    /// This app is newer than `version`.
    app_ahead: bool,
}

/// What `offered` is relative to `installed`; unknown or unparseable → `None`.
fn action_of(offered: &str, installed: Option<&str>) -> Action {
    let (Some(o), Some(i)) = (
        channel::parse_tag(offered),
        installed.and_then(channel::parse_tag),
    ) else {
        return Action::None;
    };
    match channel::decide(&i, &o) {
        channel::Decision::Upgrade => Action::Update,
        channel::Decision::Ahead => Action::Downgrade,
        channel::Decision::Current => Action::None,
    }
}

/// `behind`/`ahead` flags of one component (only an upgrade is "behind").
struct Flavor {
    behind: bool,
    ahead: bool,
}

impl Flavor {
    fn of(a: Action) -> Self {
        Self {
            behind: a == Action::Update,
            ahead: a == Action::Downgrade,
        }
    }
}

impl Release {
    /// `info` as the app sees it given the installed CLI (when known) and app versions.
    fn new(info: atlas_core::github::ReleaseInfo, cli: Option<&str>, app: &str) -> Self {
        let version = info.version().to_string();
        let (cli_f, app_f) = (
            Flavor::of(action_of(&version, cli)),
            Flavor::of(action_of(&version, Some(app))),
        );
        let action = action_of(&version, Some(app));
        Self {
            channel: channel::parse_tag(&version)
                .and_then(|v| Channel::of_version(&v))
                .unwrap_or_default()
                .to_string(),
            downgrade: action == Action::Downgrade,
            action,
            cli_behind: cli_f.behind,
            cli_ahead: cli_f.ahead,
            app_behind: app_f.behind,
            app_ahead: app_f.ahead,
            notes: info.notes().to_string(),
            url: info.url,
            published_at: info.published_at,
            version,
        }
    }
}

/// First line of `atlas version` is `atlas X.Y.Z`.
pub(crate) async fn cli_version(bin: &Path) -> Option<String> {
    let out = tokio::process::Command::new(bin)
        .no_window()
        .arg("version")
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("atlas "))
        .map(|v| v.trim().to_string())
}

fn link_target(p: &Path) -> Option<String> {
    std::fs::read_link(p)
        .ok()
        .map(|t| t.to_string_lossy().into_owned())
}

/// Linux: the AppImage this process runs from (`$APPIMAGE`, set by its runtime), else the distro
/// package's `/usr/bin/atlas-desktop`; none for a dev build. Windows: the installed exe is
/// [`atlas_core::paths::installed_app`] already, so this finds nothing more.
#[cfg(not(target_os = "macos"))]
fn own_bundle() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("APPIMAGE").map(PathBuf::from)
        && p.is_absolute()
    {
        return Some(p);
    }
    let exe = std::env::current_exe().ok()?;
    (atlas_core::paths::packaged_app().as_deref() == Some(exe.as_path())).then_some(exe)
}

/// `.../Atlas.app` when this binary is `.../Atlas.app/Contents/MacOS/<bin>`.
#[cfg(target_os = "macos")]
fn own_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let app = exe.parent()?.parent()?.parent()?;
    (app.extension()? == "app").then(|| app.to_path_buf())
}

#[tauri::command]
pub async fn system_info() -> Result<SystemInfo, String> {
    let cli = match atlas_bin() {
        Err(e) => CliInfo {
            path: None,
            version: None,
            linked_to: None,
            error: Some(e),
        },
        Ok(bin) => {
            let version = cli_version(&bin).await;
            CliInfo {
                linked_to: link_target(&bin),
                path: Some(bin.to_string_lossy().into_owned()),
                error: version
                    .is_none()
                    .then(|| "could not read `atlas version`".into()),
                version,
            }
        }
    };
    let bundle = own_bundle();
    let app = AppInfo {
        version: app_version(),
        dev: bundle.is_none(),
        bundle: bundle.map(|b| b.to_string_lossy().into_owned()),
        linked_to: atlas_core::paths::installed_app()
            .as_deref()
            .and_then(link_target),
    };
    Ok(SystemInfo { app, cli })
}

/// Latest published release (public GitHub Releases API).
#[tauri::command]
pub async fn latest_release() -> Result<Release, String> {
    let info = tauri::async_runtime::spawn_blocking(|| {
        atlas_core::github::latest_release(REPO, channel::current())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let cli = match atlas_bin() {
        Ok(bin) => cli_version(&bin).await,
        Err(_) => None,
    };
    Ok(Release::new(info, cli.as_deref(), &app_version()))
}

/// Platform suffix of the release assets (`aarch64-apple-darwin`, ...), as `atlas update` names them.
fn platform() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    format!("{arch}-apple-darwin")
}

/// Bytes an update to `version` downloads (CLI tarball + app tarball, whichever is behind);
/// `None` when a size cannot be read (offline, asset missing).
#[tauri::command]
pub async fn update_size(version: String, cli: bool, app: bool) -> Option<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        let tag = format!("v{version}");
        let p = platform();
        let mut total = 0;
        if cli {
            total +=
                atlas_core::github::asset_size(REPO, &tag, &format!("atlas-{version}-{p}.tar.gz"))?;
        }
        if app {
            total += atlas_core::github::asset_size(
                REPO,
                &tag,
                &format!("Atlas-{version}-{p}.app.tar.gz"),
            )?;
        }
        (total > 0).then_some(total)
    })
    .await
    .ok()
    .flatten()
}

/// One published release, for the "What's new" sheet.
#[derive(Serialize)]
pub struct ReleaseNotes {
    version: String,
    url: String,
    published_at: String,
    notes: String,
}

/// Release notes of every release in `(from, to]`, newest first (at most 10).
#[tauri::command]
pub async fn releases_between(from: String, to: String) -> Result<Vec<ReleaseNotes>, String> {
    const MAX: usize = 10;
    tauri::async_runtime::spawn_blocking(move || {
        let all =
            atlas_core::github::releases(REPO, channel::current()).map_err(|e| e.to_string())?;
        Ok(atlas_core::github::in_range(all, &from, &to)
            .into_iter()
            .take(MAX)
            .map(|info| ReleaseNotes {
                version: info.version().to_string(),
                notes: info.notes().to_string(),
                url: info.url,
                published_at: info.published_at,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forward lines as `updates://log` (and `@@atlas` protocol lines as `updates://progress`);
/// returns the last non-empty plain line (the error, on failure).
async fn pump<R: tokio::io::AsyncRead + Unpin>(rd: R, app: AppHandle) -> String {
    let mut lines = BufReader::new(rd).lines();
    let mut last = String::new();
    while let Ok(Some(l)) = lines.next_line().await {
        let clean = atlas_tui::runtime::text::strip_ansi(&l);
        if let Some(p) = parse_progress(&clean) {
            let _ = app.emit("updates://progress", p);
            continue;
        }
        if !clean.trim().is_empty() {
            last = clean.clone();
        }
        let _ = app.emit("updates://log", clean);
    }
    last
}

/// Run an atlas CLI subcommand, streaming each output line as `updates://log`.
/// Only one install runs at a time in this process; the CLI's own lock covers other processes.
async fn stream(app: &AppHandle, args: &[&str]) -> Result<(), String> {
    let _guard = InstallGuard::take()?;
    let mut child = tokio::process::Command::new(atlas_bin()?)
        .no_window()
        .envs(crate::shell_env::vars_async().await)
        .args(args)
        .env("NO_COLOR", "1")
        .env("ATLAS_UPDATE_PROGRESS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run atlas: {e}"))?;
    let out = child.stdout.take().expect("piped");
    let err = child.stderr.take().expect("piped");
    let (_, last_err) = tokio::join!(pump(out, app.clone()), pump(err, app.clone()));
    let st = child.wait().await.map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(if last_err.is_empty() {
            format!("atlas {} failed ({st})", args.join(" "))
        } else {
            last_err
        })
    }
}

/// Modification time of this bundle's `Info.plist`: changes when the app was replaced.
fn bundle_stamp() -> Option<std::time::SystemTime> {
    // Linux: the AppImage is one file, replaced by rename on update.
    let stamp = if cfg!(target_os = "macos") {
        own_bundle()?.join("Contents/Info.plist")
    } else {
        own_bundle()?
    };
    std::fs::metadata(stamp).ok()?.modified().ok()
}

/// Every channel with the copy and what it offers this app right now (network; an offline
/// check still returns the three cards, with empty offers).
#[tauri::command]
pub async fn update_channels() -> Result<Vec<channel::ChannelView>, String> {
    let installed = semver::Version::parse(&app_version()).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        atlas_core::github::channel_views(REPO, &installed, channel::current())
    })
    .await
    .map_err(|e| e.to_string())
}

/// The three cards from copy alone (no network): what the UI shows while the live offers cannot
/// be read. Nothing is offered, so every action is `none`.
#[tauri::command]
pub fn update_channels_offline() -> Vec<channel::ChannelView> {
    let selected = channel::current();
    Channel::ALL
        .into_iter()
        .map(|c| channel::view(c, selected, &semver::Version::new(0, 0, 0), None))
        .collect()
}

/// The followed channel, the product stage and whether `ATLAS_UPDATE_CHANNEL` forces it.
#[derive(Serialize)]
pub struct ChannelState {
    channel: Channel,
    stage: String,
    stage_note: String,
    env_override: bool,
}

/// `override_` is the value of `ATLAS_UPDATE_CHANNEL`; an invalid one is ignored (as in atlas-core).
fn channel_state_at(file: &Path, override_: Option<&str>) -> ChannelState {
    ChannelState {
        channel: channel::effective(override_, channel::read_from(file)),
        stage: channel::stage().to_string(),
        stage_note: channel::stage_note().to_string(),
        env_override: override_.is_some_and(|s| s.parse::<Channel>().is_ok()),
    }
}

fn env_channel() -> Option<String> {
    std::env::var(channel::ENV).ok()
}

#[tauri::command]
pub fn update_channel_get() -> ChannelState {
    channel_state_at(&channel::file(), env_channel().as_deref())
}

/// Validate `name`, refuse under an active env override, persist into `file`.
fn set_channel_at(file: &Path, name: &str, override_: Option<&str>) -> Result<Channel, String> {
    let wanted: Channel = name.parse()?;
    if override_.is_some_and(|s| s.parse::<Channel>().is_ok()) {
        return Err(format!(
            "The update channel is forced by {} in this environment; unset it to change the channel here",
            channel::ENV
        ));
    }
    channel::write_to(file, wanted).map_err(|e| format!("cannot save the update channel: {e}"))?;
    Ok(wanted)
}

/// Persist the channel in the file the CLI shares (`~/.atlas/update-channel`).
#[tauri::command]
pub fn update_channel_set(channel: String) -> Result<ChannelState, String> {
    let file = channel::file();
    let over = env_channel();
    set_channel_at(&file, &channel, over.as_deref())?;
    Ok(channel_state_at(&file, over.as_deref()))
}

#[cfg(test)]
fn release_sample(version: &str, cli: Option<&str>, app: &str) -> Release {
    let info = atlas_core::github::ReleaseInfo {
        tag_name: format!("v{version}"),
        url: format!("https://github.com/codeatlasdev/atlas/releases/tag/v{version}"),
        published_at: "2026-10-12".into(),
        body: "### Added\n- A thing".into(),
    };
    Release::new(info, cli, app)
}

/// Deterministic sample of everything the channel UI deserializes, for the frontend contract
/// test (`__fixtures__/channel-views.json`): produced by the real serializers.
#[cfg(test)]
fn fixture_views() -> serde_json::Value {
    let installed = semver::Version::parse("1.3.0").unwrap();
    let tag = |c: Channel| match c {
        Channel::Stable => Some("v1.3.0".to_string()),
        Channel::Beta => Some("v1.4.0-beta.2".to_string()),
        Channel::Alpha => Some("v1.4.0-alpha.3".to_string()),
    };
    let views = |sel: Channel, inst: &semver::Version| -> Vec<_> {
        Channel::ALL
            .into_iter()
            .map(|c| channel::view(c, sel, inst, tag(c)))
            .collect()
    };
    let older = semver::Version::parse("1.5.0").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("update-channel");
    channel::write_to(&file, Channel::Beta).unwrap();
    serde_json::json!({
        "installed": "1.3.0",
        "views": views(Channel::Stable, &installed),
        "ahead": views(Channel::Stable, &older),
        "offline": Channel::ALL
            .into_iter()
            .map(|c| channel::view(c, Channel::Beta, &installed, None))
            .collect::<Vec<_>>(),
        "release": {
            "update": release_sample("1.4.0-beta.2", Some("1.3.0"), "1.3.0"),
            "downgrade": release_sample("1.3.0", Some("1.4.0"), "1.4.0"),
            "none": release_sample("1.3.0", None, "1.3.0"),
        },
        "state": channel_state_at(&file, None),
        "state_override": channel_state_at(&file, Some("alpha")),
        // A profile that never chose a channel: no `update-channel` file, no env override.
        "state_default": channel_state_at(&dir.path().join("none"), None),
    })
}

/// `atlas update`: the CLI, then (via `atlas desktop --sync`) the installed app, rolling the
/// CLI back when the app cannot follow. Returns whether this app's bundle changed on disk
/// and needs a relaunch. `downgrade` installs an older build the followed channel offers
/// (explicit user choice; a plain update never goes down).
#[tauri::command]
pub async fn update_all(app: AppHandle, downgrade: Option<bool>) -> Result<bool, String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("ATLAS_DEBUG_FAKE_VERSION").is_some() {
        return Err("Updates are disabled while ATLAS_DEBUG_FAKE_VERSION is set".into());
    }
    let before = bundle_stamp();
    // The persisted channel is authoritative: `--channel` is never passed from here.
    let args: &[&str] = if downgrade == Some(true) {
        &["update", "--downgrade", "--yes"]
    } else {
        &["update"]
    };
    stream(&app, args).await?;
    Ok(before.is_some() && before != bundle_stamp())
}

/// Install/repair the app from the newest release, through the CLI.
#[tauri::command]
pub async fn reinstall_app(app: AppHandle) -> Result<(), String> {
    stream(&app, &["desktop", "--install", "--latest", "--no-open"]).await
}

/// `atlas unlink`: back to the release CLI (and release app, when linked).
#[tauri::command]
pub async fn unlink(app: AppHandle) -> Result<(), String> {
    stream(&app, &["unlink"]).await
}

/// Reopen the (possibly replaced) bundle, then quit this process.
#[tauri::command]
pub fn relaunch(app: AppHandle) -> Result<(), String> {
    let bundle = atlas_core::paths::installed_app()
        .or_else(own_bundle)
        .ok_or("not running from an installed Atlas.app")?;
    // macOS opens the bundle through Launch Services; Linux runs the AppImage/binary in its own session.
    #[cfg(unix)]
    {
        let script = if cfg!(target_os = "macos") {
            r#"sleep 1; /usr/bin/open "$0""#
        } else {
            r#"sleep 1; exec setsid "$0" </dev/null >/dev/null 2>&1"#
        };
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .arg(&bundle)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    // Windows: the new exe starts on its own while this one exits. A second instance would
    // find the control socket taken, so give this process a moment to release it.
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new(&bundle);
        cmd.env("ATLAS_RELAUNCH_WAIT_PID", std::process::id().to_string())
            .stdin(std::process::Stdio::null());
        atlas_core::sys::spawn_detached(&mut cmd).map_err(|e| e.to_string())?;
    }
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{InstallGuard, Progress, parse_progress};

    #[test]
    fn progress_lines_parse_and_plain_lines_do_not() {
        assert_eq!(
            parse_progress("@@atlas phase install-cli"),
            Some(Progress::Phase {
                id: "install-cli".into()
            })
        );
        assert_eq!(
            parse_progress("@@atlas bytes 1024 4096"),
            Some(Progress::Bytes {
                done: 1024,
                total: 4096
            })
        );
        assert_eq!(
            parse_progress("@@atlas rollback restored"),
            Some(Progress::Rollback {
                state: "restored".into()
            })
        );
        assert_eq!(
            parse_progress("@@atlas host needs-restart 2"),
            Some(Progress::Host {
                state: "needs-restart".into(),
                live: 2
            })
        );
        for bad in [
            "◆ Downloading v1",
            "@@atlas bytes x 1",
            "@@atlas bytes 1",
            "@@atlas nope 1",
            "@@atlas phase",
        ] {
            assert_eq!(parse_progress(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_one_install_at_a_time() {
        let first = InstallGuard::take().expect("free");
        assert!(InstallGuard::take().is_err(), "second install is refused");
        drop(first);
        InstallGuard::take().expect("free again after the first ends");
    }

    use super::{Flavor, action_of, channel_state_at, fixture_views, set_channel_at};
    use atlas_core::channel::{Action, Channel};

    #[test]
    fn action_of_is_update_only_for_a_strictly_newer_offer() {
        // (offered, installed) → action
        let table = [
            ("1.4.0", Some("1.3.0"), Action::Update),
            ("1.3.0", Some("1.3.0"), Action::None),
            ("1.3.0", Some("1.4.0"), Action::Downgrade),
            // prereleases sort before their release
            ("1.4.0", Some("1.4.0-rc.1"), Action::Update),
            ("1.4.0-rc.1", Some("1.4.0"), Action::Downgrade),
            ("1.4.0-alpha.10", Some("1.4.0-alpha.9"), Action::Update),
            ("1.4.0-alpha.9", Some("1.4.0-alpha.10"), Action::Downgrade),
            // build metadata is not precedence
            ("1.3.0+b2", Some("1.3.0+b1"), Action::None),
            // unknown installed or garbage → never an update
            ("1.4.0", None, Action::None),
            ("1.4.0", Some("garbage"), Action::None),
            ("garbage", Some("1.3.0"), Action::None),
        ];
        for (offered, installed, want) in table {
            assert_eq!(
                action_of(offered, installed),
                want,
                "{offered} vs {installed:?}"
            );
        }
    }

    #[test]
    fn flavor_only_upgrade_counts_as_behind() {
        let f = Flavor::of(Action::Update);
        assert!(f.behind && !f.ahead);
        let f = Flavor::of(Action::Downgrade);
        assert!(!f.behind && f.ahead);
        let f = Flavor::of(Action::None);
        assert!(!f.behind && !f.ahead);
    }

    #[test]
    fn channel_set_rejects_bad_names_and_never_touches_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        for bad in ["", "nightly", "Stabl", "canary"] {
            assert!(set_channel_at(&file, bad, None).is_err(), "{bad:?}");
        }
        assert!(!file.exists(), "a rejected name writes nothing");
    }

    #[test]
    fn channel_set_persists_through_atlas_core_and_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested/update-channel");
        assert_eq!(set_channel_at(&file, "Beta", None).unwrap(), Channel::Beta);
        assert_eq!(atlas_core::channel::read_from(&file), Channel::Beta);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "beta\n");
        assert_eq!(
            set_channel_at(&file, "stable", None).unwrap(),
            Channel::Stable
        );
        assert_eq!(atlas_core::channel::read_from(&file), Channel::Stable);
    }

    #[test]
    fn channel_set_refuses_while_the_env_override_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        let err = set_channel_at(&file, "beta", Some("alpha")).unwrap_err();
        assert!(err.contains("ATLAS_UPDATE_CHANNEL"), "{err}");
        assert!(!file.exists(), "nothing is persisted under an override");
        // an invalid override is ignored by atlas-core, so it does not block
        assert_eq!(
            set_channel_at(&file, "beta", Some("bogus")).unwrap(),
            Channel::Beta
        );
    }

    #[test]
    fn channel_state_reports_stored_channel_stage_and_override() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        atlas_core::channel::write_to(&file, Channel::Alpha).unwrap();
        let st = channel_state_at(&file, None);
        assert_eq!(st.channel, Channel::Alpha);
        assert!(!st.env_override);
        assert_eq!(st.stage, atlas_core::channel::stage());
        assert_eq!(st.stage_note, atlas_core::channel::stage_note());
        let st = channel_state_at(&file, Some("beta"));
        assert_eq!(st.channel, Channel::Beta, "the override wins");
        assert!(st.env_override);
        assert!(
            !channel_state_at(&file, Some("bogus")).env_override,
            "invalid override is ignored"
        );
    }

    /// Pre-GA default: an app whose user never chose a channel (no `~/.atlas/update-channel`, no
    /// env override) follows Alpha, so the card marked "Following" is Alpha's. An explicit Stable
    /// stays Stable.
    #[test]
    fn channel_state_without_a_file_follows_the_default_and_keeps_an_explicit_stable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        let st = channel_state_at(&file, None);
        assert_eq!(st.channel, Channel::Alpha);
        assert!(!st.env_override);
        atlas_core::channel::write_to(&file, Channel::Stable).unwrap();
        assert_eq!(channel_state_at(&file, None).channel, Channel::Stable);
    }

    #[test]
    fn offline_cards_carry_the_copy_and_offer_nothing() {
        let v = super::update_channels_offline();
        assert_eq!(
            v.iter().map(|c| c.channel).collect::<Vec<_>>(),
            Channel::ALL
        );
        for c in &v {
            assert!(c.offered.is_none() && c.tag.is_none());
            assert_eq!(c.action, Action::None);
            assert!(!c.promise.is_empty() && !c.switch_to.is_empty());
        }
    }

    /// Contract with the frontend: `apps/desktop/src/lib/__fixtures__/channel-views.json` is the
    /// REAL serialization of `ChannelView` (and the channel state). The TS test parses that file,
    /// so a rename on either side fails a test. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn channel_views_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/channel-views.json");
        // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&fixture_views(), &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it");
        // Compare values, not bytes: whether `json!` keeps insertion order depends on which crates
        // of the build enable serde_json's `preserve_order` (`cargo test -p atlas-desktop` and
        // `cargo test --workspace` differ), and key order is not part of the contract.
        let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
        let fresh: serde_json::Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
