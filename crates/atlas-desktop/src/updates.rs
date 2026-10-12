//! Versions, dev links and self-update. All installs go through the CLI
//! (`atlas update` → `atlas desktop --sync`), so the app and the terminal share
//! one code path for downloading, verifying and replacing binaries.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::cmds::atlas_bin;

const REPO: &str = "codeatlasdev/atlas";

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
    version: &'static str,
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
    /// Installed CLI is older than `version` (false when unknown or unparseable).
    cli_behind: bool,
    /// This app is older than `version`.
    app_behind: bool,
}

/// `latest > current` by semver (a pre-release sorts before its release); unparseable → false.
fn behind(latest: &str, current: Option<&str>) -> bool {
    use semver::Version;
    match (Version::parse(latest), current.map(Version::parse)) {
        (Ok(l), Some(Ok(c))) => l > c,
        _ => false,
    }
}

/// First line of `atlas version` is `atlas X.Y.Z`.
async fn cli_version(bin: &Path) -> Option<String> {
    let out = tokio::process::Command::new(bin)
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

/// `.../Atlas.app` when this binary is `.../Atlas.app/Contents/MacOS/<bin>`.
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
        version: env!("CARGO_PKG_VERSION"),
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
    let info = tauri::async_runtime::spawn_blocking(|| atlas_core::github::latest_release(REPO))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let version = info.version().to_string();
    let cli = match atlas_bin() {
        Ok(bin) => cli_version(&bin).await,
        Err(_) => None,
    };
    Ok(Release {
        cli_behind: behind(&version, cli.as_deref()),
        app_behind: behind(&version, Some(env!("CARGO_PKG_VERSION"))),
        version,
        notes: info.notes().to_string(),
        url: info.url,
        published_at: info.published_at,
    })
}

/// One published release, for the "What's new" sheet.
#[derive(Serialize)]
pub struct ReleaseNotes {
    version: String,
    url: String,
    published_at: String,
    notes: String,
}

/// Tags (newest first, as GitHub lists them) newer than `installed` and not newer than `latest`.
fn tags_between(tags: Vec<String>, installed: &str, latest: &str) -> Vec<String> {
    use semver::Version;
    let (Ok(from), Ok(to)) = (Version::parse(installed), Version::parse(latest)) else {
        return Vec::new();
    };
    tags.into_iter()
        .filter(|t| Version::parse(t.trim_start_matches('v')).is_ok_and(|v| v > from && v <= to))
        .collect()
}

/// Release notes of every release in `(from, to]`, newest first (at most 10): the
/// CHANGELOG sections published as release bodies. `from == to` yields just that release,
/// so the sheet also works for "what changed in the version I run".
#[tauri::command]
pub async fn releases_between(from: String, to: String) -> Result<Vec<ReleaseNotes>, String> {
    const MAX: usize = 10;
    tauri::async_runtime::spawn_blocking(move || {
        let mut tags = if from == to {
            vec![format!("v{to}")]
        } else {
            let listed = atlas_core::github::release_tags(REPO).map_err(|e| e.to_string())?;
            tags_between(listed, &from, &to)
        };
        tags.truncate(MAX);
        tags.iter()
            .map(|tag| {
                let info =
                    atlas_core::github::release_by_tag(REPO, tag).map_err(|e| e.to_string())?;
                Ok(ReleaseNotes {
                    version: info.version().to_string(),
                    notes: info.notes().to_string(),
                    url: info.url,
                    published_at: info.published_at,
                })
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forward lines as `updates://log`; returns the last non-empty one (the error, on failure).
async fn pump<R: tokio::io::AsyncRead + Unpin>(rd: R, app: AppHandle) -> String {
    let mut lines = BufReader::new(rd).lines();
    let mut last = String::new();
    while let Ok(Some(l)) = lines.next_line().await {
        let clean = atlas_tui::runtime::logs::strip_ansi(&l);
        if !clean.trim().is_empty() {
            last = clean.clone();
        }
        let _ = app.emit("updates://log", clean);
    }
    last
}

/// Run an atlas CLI subcommand, streaming each output line as `updates://log`.
async fn stream(app: &AppHandle, args: &[&str]) -> Result<(), String> {
    let mut child = tokio::process::Command::new(atlas_bin()?)
        .envs(crate::shell_env::vars_async().await)
        .args(args)
        .env("NO_COLOR", "1")
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

/// `atlas update`: the CLI, then (via `atlas desktop --sync`) the installed app.
/// Returns whether this app's bundle changed on disk and needs a relaunch.
#[tauri::command]
pub async fn update_all(app: AppHandle) -> Result<bool, String> {
    let before = own_bundle().and_then(|b| {
        std::fs::metadata(b.join("Contents/Info.plist"))
            .ok()?
            .modified()
            .ok()
    });
    stream(&app, &["update"]).await?;
    let after = own_bundle().and_then(|b| {
        std::fs::metadata(b.join("Contents/Info.plist"))
            .ok()?
            .modified()
            .ok()
    });
    Ok(before.is_some() && before != after)
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
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"sleep 1; /usr/bin/open "$0""#)
        .arg(&bundle)
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::behind;

    #[test]
    fn behind_compares_semver_with_prereleases() {
        assert!(behind("0.5.9", Some("0.5.8")));
        assert!(!behind("0.5.8", Some("0.5.8")));
        assert!(!behind("0.5.7", Some("0.5.8")));
        assert!(
            behind("0.5.8", Some("0.5.8-rc.1")),
            "release > its pre-release"
        );
        assert!(!behind("0.5.8-rc.1", Some("0.5.8")));
        assert!(behind("0.10.0", Some("0.9.9")), "numeric, not lexical");
    }

    #[test]
    fn behind_is_false_when_unknown_or_unparseable() {
        assert!(!behind("0.5.9", None));
        assert!(!behind("garbage", Some("0.5.8")));
        assert!(!behind("0.5.9", Some("garbage")));
    }

    #[test]
    fn tags_between_keeps_only_the_range() {
        use super::tags_between;
        let tags = || {
            ["v1.0.2", "v1.0.1", "v1.0.0", "v0.9.0", "nightly"]
                .map(String::from)
                .to_vec()
        };
        assert_eq!(tags_between(tags(), "1.0.0", "1.0.2"), ["v1.0.2", "v1.0.1"]);
        assert_eq!(tags_between(tags(), "1.0.0", "1.0.1"), ["v1.0.1"]);
        assert!(tags_between(tags(), "1.0.2", "1.0.2").is_empty());
        assert!(tags_between(tags(), "garbage", "1.0.2").is_empty());
    }
}
