//! A Finder/Dock launch hands the app launchd's bare environment
//! (`PATH=/usr/bin:/bin:…`, no nvm/mise/brew/bun), so every service the
//! runner spawns dies with `command not found`. We ask the user's own shell —
//! whatever it is — for its environment and give it to every child we spawn.
//!
//! Shell choice: Settings override, else the account's login shell from the
//! user database (`getpwuid`, what Terminal uses), else `$SHELL`, else zsh.
//! The probe is shell-agnostic: the only things the shell has to run are
//! `echo` and the external `/usr/bin/env -0`, and flag sets degrade from
//! `-l -i -c` to `-l -c` to `-c` for shells that reject combined flags
//! (csh/tcsh). Runs off the main thread; spawns wait for it at most a few
//! seconds and fall back to the inherited environment (reported in the UI).

use std::ffi::CStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const MARK: &str = "__ATLAS_ENV__";
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(6);
const SPAWN_WAIT: Duration = Duration::from_secs(8);
const FLAG_SETS: [&[&str]; 3] = [&["-l", "-i", "-c"], &["-l", "-c"], &["-c"]];
/// Per-session/terminal vars that must not leak into services.
const SKIP: [&str; 6] = [
    "PWD",
    "OLDPWD",
    "SHLVL",
    "_",
    "TERM_SESSION_ID",
    "TERM_PROGRAM",
];

#[derive(Clone, Serialize)]
pub struct Probe {
    pub shell: String,
    /// `settings` | `account` | `$SHELL` | `default`
    pub source: &'static str,
    /// Flags that worked, e.g. `-l -i -c`.
    pub flags: Option<String>,
    pub error: Option<String>,
    pub took_ms: u64,
    #[serde(skip)]
    pub vars: Vec<(String, String)>,
}

static STATE: Mutex<Option<Probe>> = Mutex::new(None);
static READY: Condvar = Condvar::new();

/// Start the first probe in the background (call once at launch).
pub fn start() {
    std::thread::spawn(|| store(run()));
}

/// Re-probe now (after a settings change); blocks.
pub fn reprobe() -> Probe {
    let p = run();
    store(p.clone());
    p
}

fn store(p: Probe) {
    if let Some(e) = &p.error {
        eprintln!("atlas-desktop: shell env probe failed ({}): {e}", p.shell);
    }
    *STATE.lock().unwrap() = Some(p);
    READY.notify_all();
}

/// Latest probe, waiting up to `SPAWN_WAIT` for the first one.
pub fn current() -> Option<Probe> {
    let g = STATE.lock().unwrap();
    let (g, _) = READY
        .wait_timeout_while(g, SPAWN_WAIT, |p| p.is_none())
        .unwrap();
    g.clone()
}

/// The user's shell environment, to pass to every child via `.envs(...)`.
/// Empty when the probe failed: the child then inherits our own env.
pub fn vars() -> Vec<(String, String)> {
    current().map(|p| p.vars).unwrap_or_default()
}

/// The `PATH` children run with (probed, else ours).
pub fn path_var() -> Option<std::ffi::OsString> {
    vars()
        .into_iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.into())
        .or_else(|| std::env::var_os("PATH"))
}

/// [`vars`] without blocking an async worker.
pub async fn vars_async() -> Vec<(String, String)> {
    tauri::async_runtime::spawn_blocking(vars)
        .await
        .unwrap_or_default()
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
        .and_then(|p| p.vars.iter().find(|(k, _)| k == "PATH"))
        .map(|(_, v)| v.split(':').map(String::from).collect())
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
    atlas_core::paths::which_in(name, &path_var()?)
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
    let mut seen = std::collections::HashSet::new();
    std::fs::read_to_string("/etc/shells")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/') && is_executable(Path::new(l)))
        .filter(|l| seen.insert(l.to_string()))
        .map(String::from)
        .collect()
}

// ---------- probe ----------

fn resolve_shell() -> (String, &'static str) {
    if let Some(s) = load_settings()
        .shell
        .filter(|s| is_executable(Path::new(s)))
    {
        return (s, "settings");
    }
    if let Some(s) = account_shell().filter(|s| is_executable(Path::new(s))) {
        return (s, "account");
    }
    if let Some(s) = std::env::var("SHELL")
        .ok()
        .filter(|s| is_executable(Path::new(s)))
    {
        return (s, "$SHELL");
    }
    ("/bin/zsh".into(), "default")
}

/// Login shell from the user database (set by `chsh`).
fn account_shell() -> Option<String> {
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: buffers live for the call; `out` points into `pw` on success.
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut pw,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        )
    };
    if rc != 0 || out.is_null() || pw.pw_shell.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(pw.pw_shell) }
        .to_string_lossy()
        .into_owned();
    (!s.is_empty()).then_some(s)
}

fn run() -> Probe {
    let t0 = Instant::now();
    let (shell, source) = resolve_shell();
    let mut errors = Vec::new();
    for flags in FLAG_SETS {
        match attempt(&shell, flags) {
            Ok(vars) => {
                return Probe {
                    shell,
                    source,
                    flags: Some(flags.join(" ")),
                    error: None,
                    took_ms: t0.elapsed().as_millis() as u64,
                    vars,
                };
            }
            Err(e) => errors.push(format!("{}: {e}", flags.join(" "))),
        }
    }
    Probe {
        shell,
        source,
        flags: None,
        error: Some(errors.join("; ")),
        took_ms: t0.elapsed().as_millis() as u64,
        vars: Vec::new(),
    }
}

fn attempt(shell: &str, flags: &[&str]) -> Result<Vec<(String, String)>, String> {
    let script = format!("echo {MARK}; /usr/bin/env -0; echo {MARK}");
    let mut child = Command::new(shell)
        .args(flags)
        .arg(&script)
        .current_dir(dirs::home_dir().unwrap_or_else(|| "/".into()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Read concurrently so a chatty rc file can't fill the pipe and stall.
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + ATTEMPT_TIMEOUT;
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out after {}s", ATTEMPT_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let out = reader.join().unwrap_or_default();
    parse(&String::from_utf8_lossy(&out)).ok_or_else(|| "no environment in output".into())
}

/// `KEY=VALUE` pairs between the markers; rc files may print around them.
fn parse(out: &str) -> Option<Vec<(String, String)>> {
    let mut parts = out.split(MARK);
    parts.next()?;
    let body = parts.next()?;
    parts.next()?; // closing marker must be present
    let vars: Vec<_> = body
        .trim_start_matches(['\r', '\n'])
        .split('\0')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| !k.is_empty() && !SKIP.contains(k))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    vars.iter().any(|(k, _)| k == "PATH").then_some(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_env_drops_noise_and_session_vars() {
        let out = format!("Welcome!\n{MARK}\nPATH=/a:/b\0FOO=x=y\0PWD=/tmp\0\n{MARK}\nbye");
        let v = parse(&out).unwrap();
        assert_eq!(
            v,
            vec![
                ("PATH".into(), "/a:/b".into()),
                ("FOO".into(), "x=y".into())
            ]
        );
    }

    #[test]
    fn parse_rejects_truncated_or_pathless_output() {
        assert!(parse(&format!("{MARK}\nPATH=/a\0")).is_none());
        assert!(parse(&format!("{MARK}\nFOO=1\0{MARK}")).is_none());
        assert!(parse("nothing").is_none());
    }

    /// Every shell installed here yields a PATH from a launchd-bare env.
    #[test]
    fn probes_installed_shells() {
        for sh in installed_shells() {
            let ok = FLAG_SETS.iter().any(|f| attempt(&sh, f).is_ok());
            assert!(ok, "{sh}: no env");
        }
    }
}
