//! Atlas Desktop's side of the login-shell environment (`atlas_core::login_env`, shared with the runner and
//! hostd so a service, a `pre_command`, an action and a terminal all see the same PATH). A Finder/Dock or
//! desktop-entry launch hands the app a bare environment (`PATH=/usr/bin:/bin:…`, no nvm/mise/brew/bun);
//! the probe asks the user's own shell for its environment. This module owns the Settings override, the
//! start-up probe off the main thread and the `shell_info`/`shell_set` commands. On Windows the "probe" is
//! the app's own environment, which already is the user's (see `login_env`).

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
        .map(|v| {
            std::env::split_paths(v)
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
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
    let p = settings_path().ok_or("cannot locate home directory")?;
    set_shell_at(&p, shell)?;
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

/// Settings for readers: whatever is on disk, defaults when it is missing or unusable, so the app always starts.
/// Writers go through [`patch_settings_at`], which never turns an unusable file into silent data loss.
pub fn load_settings() -> Settings {
    settings_path()
        .map(|p| lenient_settings_at(&p))
        .unwrap_or_default()
}

fn lenient_settings_at(p: &Path) -> Settings {
    read_settings_at(p).unwrap_or_default()
}

/// Missing file = defaults. A file that is empty, truncated, not JSON or not a JSON object is an error naming it.
fn read_settings_at(p: &Path) -> Result<Settings, String> {
    match std::fs::read_to_string(p) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

static SETTINGS_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Load, change, save settings.json under one lock: two patches in flight (the onboarding flag and a preference)
/// each started from the same file and the later save dropped the earlier key.
///
/// A file that cannot be parsed (a crash mid-write on an old build, a hand edit gone wrong) used to be read as
/// "no settings", so the first patch rewrote it from defaults and every setting was lost. It is copied to
/// `settings.json.corrupt` first and the change starts from defaults; the user's bytes stay recoverable. A file
/// that cannot be *read* (permissions, I/O) is an error and is left alone.
fn mutate_settings_at(p: &Path, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let _guard = SETTINGS_LOCK.lock();
    let mut s = match std::fs::read(p) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(_) => {
                let mut aside = p.as_os_str().to_os_string();
                aside.push(".corrupt");
                std::fs::write(&aside, &bytes)
                    .map_err(|e| format!("cannot keep the unreadable {}: {e}", p.display()))?;
                Settings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    f(&mut s);
    write_settings_at(p, &s)?;
    Ok(s)
}

fn patch_settings_at(
    p: &Path,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<Settings, String> {
    mutate_settings_at(p, |s| apply_patch(s, patch))
}

fn set_shell_at(p: &Path, shell: Option<String>) -> Result<Settings, String> {
    mutate_settings_at(p, |s| s.shell = shell)
}

fn write_settings_at(p: &Path, s: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::registry::write_atomic(p, &text)
}

/// Shallow merge: `null` deletes a key; `shell` is owned by [`shell_set`].
fn apply_patch(s: &mut Settings, patch: serde_json::Map<String, serde_json::Value>) {
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
    let p = settings_path().ok_or("cannot locate home directory")?;
    patch_settings_at(&p, patch)
}

/// Windows: the shells a terminal tab can run, as found on `PATH` (PowerShell 7, Windows
/// PowerShell, Command Prompt, Git Bash).
#[cfg(windows)]
pub fn installed_shells() -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ["pwsh", "powershell", "cmd", "bash"]
        .iter()
        .filter_map(|s| atlas_core::paths::which(s))
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| seen.insert(p.to_lowercase()))
        .collect()
}

/// Shells registered in `/etc/shells` that exist on disk.
#[cfg(unix)]
pub fn installed_shells() -> Vec<String> {
    shells_from(
        &std::fs::read_to_string("/etc/shells").unwrap_or_default(),
        |p| is_executable(Path::new(p)),
    )
}

/// Entries of an `/etc/shells` text that can report an environment: absolute, present (`exists`),
/// deduplicated. Debian/Ubuntu also list restricted shells (`rbash` refuses `/usr/bin/env`, the
/// probe's only external command) and multiplexers (`tmux`, `screen`), which are not shells.
#[cfg_attr(windows, allow(dead_code))]
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

    fn patch_of(json: &str) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_str(json).unwrap()
    }

    /// Incident class: `load_settings` swallowed a corrupt `settings.json` into defaults, so the next patch (the
    /// onboarding flag, any preference) rewrote the file from defaults and every other setting was gone with no
    /// way back. A corrupt file is now moved aside, never overwritten.
    #[test]
    fn patching_a_corrupt_settings_file_keeps_the_original_aside() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("desktop/settings.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let broken = r#"{"worktrees":{"base":"/x"},"onboarding":{"done":true"#; // truncated mid-write
        std::fs::write(&p, broken).unwrap();

        let s = patch_settings_at(&p, patch_of(r#"{"theme":"dark"}"#)).unwrap();

        assert_eq!(s.extra["theme"], "dark");
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");
        let aside =
            std::fs::read_to_string(dir.path().join("desktop/settings.json.corrupt")).unwrap();
        assert_eq!(aside, broken, "the user's bytes must survive for recovery");
    }

    /// A document that parses but is not an object (`[]`, `null`, `"x"`) is as unusable as a truncated one.
    #[test]
    fn non_object_settings_are_corrupt_not_empty() {
        for text in ["[]", "null", "\"x\"", "42", ""] {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("settings.json");
            std::fs::write(&p, text).unwrap();
            let Err(err) = read_settings_at(&p) else {
                panic!("{text:?} parsed as settings");
            };
            assert!(err.contains("settings.json"), "{text:?}: {err}");
            // readers still get defaults: the app must start
            assert!(lenient_settings_at(&p).extra.is_empty(), "{text:?}");
        }
    }

    #[test]
    fn a_missing_settings_file_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nope/settings.json");
        assert!(read_settings_at(&p).unwrap().extra.is_empty());
        let s = patch_settings_at(&p, patch_of(r#"{"a":1}"#)).unwrap();
        assert_eq!(s.extra["a"], 1);
        assert!(!dir.path().join("nope/settings.json.corrupt").exists());
    }

    /// Incident class: `settings_patch` was load → modify → save with no lock, so two calls in flight (onboarding
    /// `done` + a pref saved at the same moment) each started from the same file and the later write dropped the
    /// earlier one's key.
    #[test]
    fn concurrent_patches_never_lose_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = std::sync::Arc::new(dir.path().join("settings.json"));
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let p = p.clone();
                std::thread::spawn(move || {
                    patch_settings_at(&p, patch_of(&format!(r#"{{"k{i}":{i}}}"#))).unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let s = read_settings_at(&p).unwrap();
        for i in 0..16 {
            assert_eq!(
                s.extra[&format!("k{i}")],
                i,
                "k{i} lost: {:?}",
                s.extra.keys()
            );
        }
    }

    /// `shell` is owned by `shell_set`; a frontend patch must not be able to rewrite it, and `null` deletes a key.
    #[test]
    fn patch_skips_shell_and_null_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"shell":"/bin/zsh","a":1,"b":2}"#).unwrap();
        let s = patch_settings_at(&p, patch_of(r#"{"shell":"/evil","a":null,"c":3}"#)).unwrap();
        assert_eq!(s.shell.as_deref(), Some("/bin/zsh"));
        assert!(!s.extra.contains_key("a"));
        assert_eq!(
            (s.extra["b"].clone(), s.extra["c"].clone()),
            (2.into(), 3.into())
        );
    }

    /// `shell_set` shared the wipe: it saved `Settings { shell, ..load_settings() }`, so choosing a shell while
    /// settings.json was corrupt rewrote the file from defaults. It uses the same locked, backing-up path.
    #[test]
    fn setting_the_shell_keeps_other_keys_and_backs_up_a_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"theme":"dark"}"#).unwrap();
        let s = set_shell_at(&p, Some("/bin/zsh".into())).unwrap();
        assert_eq!(
            (s.shell.as_deref(), &s.extra["theme"]),
            (Some("/bin/zsh"), &"dark".into())
        );
        assert!(set_shell_at(&p, None).unwrap().shell.is_none());
        assert_eq!(read_settings_at(&p).unwrap().extra["theme"], "dark");

        std::fs::write(&p, "{ not json").unwrap();
        set_shell_at(&p, Some("/bin/bash".into())).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("settings.json.corrupt")).unwrap(),
            "{ not json"
        );
    }

    /// A settings file with invalid UTF-8 (an editor saving as UTF-16, a disk fault) is corrupt, not an I/O
    /// error to retry forever: it is kept aside byte for byte and the app can save settings again.
    #[test]
    fn a_non_utf8_settings_file_is_backed_up_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        let raw = [0xFF_u8, 0xFE, b'{', 0, b'}', 0];
        std::fs::write(&p, raw).unwrap();
        assert!(lenient_settings_at(&p).extra.is_empty());
        patch_settings_at(&p, patch_of(r#"{"a":1}"#)).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("settings.json.corrupt")).unwrap(),
            raw
        );
        assert_eq!(read_settings_at(&p).unwrap().extra["a"], 1);
    }
}
