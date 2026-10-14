//! Atlas Desktop's side of the login-shell environment (`atlas_core::login_env`, shared with the runner and
//! hostd so a service, a `pre_command`, an action and a terminal all see the same PATH). A Finder/Dock or
//! desktop-entry launch hands the app a bare environment (`PATH=/usr/bin:/bin:…`, no nvm/mise/brew/bun);
//! the probe asks the user's own shell for its environment. This module owns the Settings override, the
//! start-up probe off the main thread and the `shell_info`/`shell_set` commands.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_core::login_env::{self, Probe};
use serde::{Deserialize, Serialize};

const SPAWN_WAIT: Duration = Duration::from_secs(8);

/// Start the first probe in the background (call once at launch), with the Settings override applied.
pub fn start() {
    login_env::set_override(load_settings().shell);
    std::thread::spawn(|| report(login_env::reprobe()));
}

/// Re-probe now (after a settings change); blocks.
pub fn reprobe() -> Probe {
    login_env::set_override(load_settings().shell);
    let p = login_env::reprobe();
    report(p.clone());
    p
}

fn report(p: Probe) {
    if let Some(e) = &p.error {
        eprintln!("atlas-desktop: shell env probe failed ({}): {e}", p.shell);
    }
}

/// The user's shell environment, to pass to every child via `.envs(...)`.
pub fn vars() -> Vec<(String, String)> {
    login_env::vars()
}

/// The `PATH` children run with.
pub fn path_var() -> Option<std::ffi::OsString> {
    Some(login_env::path_var())
}

/// [`vars`] without blocking an async worker.
pub async fn vars_async() -> Vec<(String, String)> {
    tauri::async_runtime::spawn_blocking(vars)
        .await
        .unwrap_or_default()
}

/// The probe, waiting at most `SPAWN_WAIT` for a first one still running.
pub fn current() -> Option<Probe> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(login_env::current());
    });
    rx.recv_timeout(SPAWN_WAIT).ok()
}

/// [`vars_async`] as the JSON `env` object hostd's `exec` takes.
pub async fn vars_json_async() -> serde_json::Map<String, serde_json::Value> {
    vars_async()
        .await
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect()
}

// ---------- commands ----------

#[derive(Serialize)]
pub struct ShellInfo {
    /// Override from Settings; `None` = automatic.
    pub configured: Option<String>,
    pub installed: Vec<String>,
    pub probe: Option<Probe>,
    pub path: Vec<String>,
    /// Tools found on the probed PATH, for a quick sanity check.
    pub tools: Vec<(String, Option<String>)>,
}

fn info(probe: Option<Probe>) -> ShellInfo {
    let path = probe
        .as_ref()
        .and_then(Probe::path)
        .map(|v| v.split(':').map(String::from).collect())
        .unwrap_or_default();
    let tools = ["bun", "node", "npm", "pnpm", "docker", "git", "gh", "atlas"]
        .map(|t| {
            (
                t.to_string(),
                which(t).map(|p| p.to_string_lossy().into_owned()),
            )
        })
        .into();
    ShellInfo {
        configured: load_settings().shell,
        installed: installed_shells(),
        probe,
        path,
        tools,
    }
}

#[tauri::command]
pub async fn shell_info() -> ShellInfo {
    tauri::async_runtime::spawn_blocking(|| info(current()))
        .await
        .expect("shell_info task")
}

/// Save the shell override (`None` = automatic) and re-read the environment.
#[tauri::command]
pub async fn shell_set(shell: Option<String>) -> Result<ShellInfo, String> {
    let shell = shell
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(s) = &shell
        && !is_executable(Path::new(s))
    {
        return Err(format!("{s} is not an executable file"));
    }
    save_settings(&Settings {
        shell,
        ..load_settings()
    })?;
    tauri::async_runtime::spawn_blocking(|| info(Some(reprobe())))
        .await
        .map_err(|e| e.to_string())
}

/// First `name` executable on the children's `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    login_env::which(name)
}

pub use atlas_core::paths::is_executable;

// ---------- settings ----------

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    /// Absolute path of the shell to read the environment from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Everything else (worktrees, agents, notifications, quick commands…),
    /// owned by the frontend and the modules that read it.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".atlas/desktop/settings.json"))
}

pub fn load_settings() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> Result<(), String> {
    let p = settings_path().ok_or("cannot locate home directory")?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::registry::write_atomic(&p, &text)
}

#[tauri::command]
pub fn settings_get() -> Settings {
    load_settings()
}

/// Shallow-merge `patch` into settings.json (`null` deletes a key; `shell` excluded).
#[tauri::command]
pub fn settings_patch(
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<Settings, String> {
    let mut s = load_settings();
    for (k, v) in patch {
        if k == "shell" {
            continue;
        }
        if v.is_null() {
            s.extra.remove(&k);
        } else {
            s.extra.insert(k, v);
        }
    }
    save_settings(&s)?;
    Ok(s)
}

/// Shells registered in `/etc/shells` that exist on disk.
pub fn installed_shells() -> Vec<String> {
    shells_from(
        &std::fs::read_to_string("/etc/shells").unwrap_or_default(),
        |p| is_executable(Path::new(p)),
    )
}

/// Entries of an `/etc/shells` text that can report an environment: absolute, present (`exists`),
/// deduplicated. Debian/Ubuntu also list restricted shells (`rbash` refuses `/usr/bin/env`, the
/// probe's only external command) and multiplexers (`tmux`, `screen`), which are not shells.
fn shells_from(text: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    const NOT_PROBEABLE: [&str; 4] = ["rbash", "rksh", "tmux", "screen"];
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .filter(|l| {
            let name = l.rsplit('/').next().unwrap_or(l);
            !NOT_PROBEABLE.contains(&name)
        })
        .filter(|l| exists(l))
        .filter(|l| seen.insert(l.to_string()))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the first Linux build: Ubuntu's `/etc/shells` lists `rbash` (it refuses `/usr/bin/env`, so the
    /// probe can never succeed) and `tmux`, and offering either in Settings breaks every spawn.
    #[test]
    fn shell_list_drops_restricted_shells_and_multiplexers() {
        let debian = "# /etc/shells: valid login shells\n/bin/sh\n/usr/bin/sh\n/bin/bash\n/bin/rbash\n\
                      /usr/bin/rbash\n/usr/bin/dash\n/usr/bin/tmux\n/usr/bin/screen\n/bin/bash\n/opt/gone/zsh\n";
        assert_eq!(
            shells_from(debian, |p| !p.starts_with("/opt/gone")),
            ["/bin/sh", "/usr/bin/sh", "/bin/bash", "/usr/bin/dash"]
        );
    }
}
