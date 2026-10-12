use std::path::Path;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::info;

use crate::config::PreCommandWhen;

/// Combined stdout+stderr of the last run, relative to the log dir.
pub const SETUP_LOG: &str = "setup.log";
/// State of the last run (`SetupStatus`), relative to the log dir.
pub const SETUP_STATUS: &str = "setup.json";

/// `setup.json`: written "running" before the spawn, final state after.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupStatus {
    /// `running` | `ok` | `failed`.
    pub state: String,
    pub command: String,
    pub started_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What `ensure_deps` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepsOutcome {
    /// Nothing to do (up to date, or `pre_command: ""` disables it).
    UpToDate,
    /// The install ran and exited 0.
    Installed,
    /// The install could not start or exited non-zero; carries the reason.
    Failed(String),
}

/// Pure staleness decision: install when `node_modules` is missing, or when a
/// lockfile is newer than the `.atlas-installed` marker (falling back to the
/// `node_modules` mtime).
fn needs_install(root_dir: &Path) -> bool {
    let node_modules = root_dir.join("node_modules");
    if !node_modules.exists() {
        return true;
    }
    // Use marker file if it exists, otherwise use node_modules mtime
    let marker = node_modules.join(".atlas-installed");
    let nm_mtime = mtime(&marker)
        .or_else(|| mtime(&node_modules))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    [
        "bun.lock",
        "pnpm-lock.yaml",
        "yarn.lock",
        "package-lock.json",
    ]
    .iter()
    .map(|f| root_dir.join(f))
    .any(|lock| lock.exists() && mtime(&lock).unwrap_or(SystemTime::UNIX_EPOCH) > nm_mtime)
}

/// Effective schedule: an explicit non-empty `pre_command` defaults to
/// `always`; the auto-detected installer is always lockfile-gated (`npm ci`
/// deletes `node_modules`, it must never run unconditionally).
pub fn effective_when(pre_command: Option<&str>, when: Option<PreCommandWhen>) -> PreCommandWhen {
    match pre_command {
        Some(c) if !c.is_empty() => when.unwrap_or(PreCommandWhen::Always),
        _ => PreCommandWhen::Lockfile,
    }
}

/// The auto-detected installer for a JS project, by lockfile (`npm ci` last).
/// `None` when there is no `package.json`.
pub fn detect_install_command(root_dir: &Path) -> Option<&'static str> {
    if !root_dir.join("package.json").exists() {
        return None;
    }
    Some(if root_dir.join("bun.lock").exists() {
        "bun install"
    } else if root_dir.join("pnpm-lock.yaml").exists() {
        "pnpm install --frozen-lockfile"
    } else if root_dir.join("yarn.lock").exists() {
        "yarn install --frozen-lockfile"
    } else {
        "npm ci"
    })
}

/// True when `ensure_deps` (non-forced) would run an install (same gate, no
/// side effects): the TUI uses it to hold app-service start until it finishes.
pub fn install_pending(
    root_dir: &Path,
    pre_command: Option<&str>,
    when: Option<PreCommandWhen>,
) -> bool {
    if pre_command == Some("")
        || (pre_command.is_none() && detect_install_command(root_dir).is_none())
    {
        return false;
    }
    effective_when(pre_command, when) == PreCommandWhen::Always || needs_install(root_dir)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Best effort: the setup files are diagnostics, never a reason to fail a run.
fn write_status(dir: &Path, status: &SetupStatus) {
    if let Ok(json) = serde_json::to_string(status) {
        let _ = std::fs::write(dir.join(SETUP_STATUS), json);
    }
}

/// Check if dependencies need to be installed and install them if so.
/// `pre_command` (when non-empty) replaces the auto-detected package manager;
/// an empty string disables the step entirely (even when `force`d). Without a
/// `pre_command` the auto-detected install only applies to JS projects (a
/// `package.json`), so a Rust/Go project never runs `npm ci` (and never fails
/// `--ci` because of it). `when` (see [`effective_when`]) decides whether an
/// explicit `pre_command` runs every time or only on a stale lockfile; `force`
/// skips that gate ("Run now"). Every actual run leaves its combined output in
/// `.logs/setup.log` and its state in `.logs/setup.json`.
pub async fn ensure_deps(
    root_dir: &Path,
    pre_command: Option<&str>,
    when: Option<PreCommandWhen>,
    force: bool,
) -> DepsOutcome {
    let runnable = match pre_command {
        Some("") => false,
        Some(_) => true,
        None => detect_install_command(root_dir).is_some(),
    };
    if !runnable || !(force || install_pending(root_dir, pre_command, when)) {
        return DepsOutcome::UpToDate;
    }
    let command = match pre_command {
        Some(c) => c.to_string(),
        None => detect_install_command(root_dir)
            .unwrap_or("npm ci")
            .to_string(),
    };

    info!("Running dependency install: {command}");
    let log_dir = super::log_dir(root_dir);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join(SETUP_LOG);
    let mut status = SetupStatus {
        state: "running".into(),
        command: command.clone(),
        started_at: unix_now(),
        finished_at: None,
        reason: None,
    };
    write_status(&log_dir, &status);

    // One file for stdout and stderr keeps their interleaving; truncated per run.
    let spawned = std::fs::File::create(&log_path).and_then(|mut f| {
        use std::io::Write;
        writeln!(f, "$ {command}")?;
        let err = f.try_clone()?;
        Command::new("sh")
            .args(["-c", &command])
            .current_dir(root_dir)
            .stdin(Stdio::null())
            .stdout(f)
            .stderr(err)
            .spawn()
    });

    let outcome = match spawned {
        Ok(mut child) => match child.wait().await {
            Ok(exit) if exit.success() => {
                append_log(&log_path, "exit status: 0");
                DepsOutcome::Installed
            }
            Ok(exit) => {
                let code = exit.code().map_or("signal".to_string(), |c| c.to_string());
                append_log(&log_path, &format!("exit status: {code}"));
                let tail = last_nonempty_line(&log_path);
                DepsOutcome::Failed(format!("exit {code}: {tail}"))
            }
            Err(e) => {
                let reason = format!("could not run: {e}");
                append_log(&log_path, &reason);
                DepsOutcome::Failed(reason)
            }
        },
        Err(e) => {
            let reason = format!("could not run: {e}");
            append_log(&log_path, &reason);
            DepsOutcome::Failed(reason)
        }
    };

    status.finished_at = Some(unix_now());
    match &outcome {
        DepsOutcome::Failed(reason) => {
            status.state = "failed".into();
            status.reason = Some(reason.clone());
        }
        _ => {
            status.state = "ok".into();
            let _ = touch_marker(&root_dir.join("node_modules"));
            info!("Dependencies installed successfully");
        }
    }
    write_status(&log_dir, &status);
    outcome
}

fn append_log(path: &Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Last non-empty output line before the trailing `exit status:` marker.
fn last_nonempty_line(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("exit status:"))
        .map(str::to_string)
        .next()
        .unwrap_or_default()
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn touch_marker(node_modules: &Path) -> std::io::Result<()> {
    std::fs::write(
        node_modules.join(".atlas-installed"),
        format!("{}", std::process::id()),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// node_modules whose marker is older than any lockfile written after it.
    fn project_with_stale_lock() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        touch_marker(&dir.path().join("node_modules")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.path().join("bun.lock"), "lock").unwrap();
        dir
    }

    #[test]
    fn needs_install_when_node_modules_missing() {
        let dir = TempDir::new().unwrap();
        assert!(needs_install(dir.path()));
    }

    #[test]
    fn no_install_when_node_modules_fresh() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("bun.lock"), "lock").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        touch_marker(&dir.path().join("node_modules")).unwrap();
        assert!(!needs_install(dir.path()));
    }

    #[test]
    fn needs_install_when_lockfile_newer_than_marker() {
        assert!(needs_install(project_with_stale_lock().path()));
    }

    #[tokio::test]
    async fn empty_pre_command_disables_install() {
        let dir = TempDir::new().unwrap(); // node_modules missing = stale
        assert_eq!(
            ensure_deps(dir.path(), Some(""), None, false).await,
            DepsOutcome::UpToDate
        );
        // Not even "Run now" runs a disabled step.
        assert_eq!(
            ensure_deps(dir.path(), Some(""), None, true).await,
            DepsOutcome::UpToDate
        );
        assert!(!dir.path().join(".logs").exists());
    }

    #[tokio::test]
    async fn pre_command_success_installs_and_writes_marker() {
        let dir = TempDir::new().unwrap();
        let lockfile = Some(PreCommandWhen::Lockfile);
        let out = ensure_deps(
            dir.path(),
            Some("mkdir -p node_modules && echo ran > ran.txt"),
            lockfile,
            false,
        )
        .await;
        assert_eq!(out, DepsOutcome::Installed);
        assert!(dir.path().join("ran.txt").exists());
        assert!(dir.path().join("node_modules/.atlas-installed").exists());
        // Second call: fresh now, a lockfile-gated pre_command must not run again.
        fs::remove_file(dir.path().join("ran.txt")).unwrap();
        let again = ensure_deps(dir.path(), Some("echo ran > ran.txt"), lockfile, false).await;
        assert_eq!(again, DepsOutcome::UpToDate);
        assert!(!dir.path().join("ran.txt").exists());
    }

    #[tokio::test]
    async fn pre_command_always_reruns_when_fresh() {
        let dir = TempDir::new().unwrap();
        let cmd = Some("mkdir -p node_modules && echo ran >> ran.txt");
        // Default (None) with an explicit pre_command is `always`.
        for expected_lines in [1, 2] {
            assert_eq!(
                ensure_deps(dir.path(), cmd, None, false).await,
                DepsOutcome::Installed
            );
            let runs = fs::read_to_string(dir.path().join("ran.txt")).unwrap();
            assert_eq!(runs.lines().count(), expected_lines);
        }
        assert!(install_pending(
            dir.path(),
            cmd,
            Some(PreCommandWhen::Always)
        ));
    }

    #[tokio::test]
    async fn pre_command_lockfile_does_not_rerun_when_fresh() {
        let dir = TempDir::new().unwrap();
        let cmd = Some("mkdir -p node_modules && echo ran >> ran.txt");
        let lockfile = Some(PreCommandWhen::Lockfile);
        assert_eq!(
            ensure_deps(dir.path(), cmd, lockfile, false).await,
            DepsOutcome::Installed
        );
        assert_eq!(
            ensure_deps(dir.path(), cmd, lockfile, false).await,
            DepsOutcome::UpToDate
        );
        assert!(!install_pending(dir.path(), cmd, lockfile));
        // "Run now" skips the gate.
        assert_eq!(
            ensure_deps(dir.path(), cmd, lockfile, true).await,
            DepsOutcome::Installed
        );
        let runs = fs::read_to_string(dir.path().join("ran.txt")).unwrap();
        assert_eq!(runs.lines().count(), 2);
    }

    #[tokio::test]
    async fn pre_command_failure_reports_exit_and_stderr() {
        let dir = TempDir::new().unwrap();
        let out = ensure_deps(dir.path(), Some("echo boom >&2; exit 3"), None, false).await;
        assert_eq!(out, DepsOutcome::Failed("exit 3: boom".into()));
        assert!(!dir.path().join("node_modules/.atlas-installed").exists());
    }

    #[tokio::test]
    async fn setup_files_record_success_and_failure() {
        let dir = TempDir::new().unwrap();
        let logs = dir.path().join(".logs");

        ensure_deps(dir.path(), Some("echo hi; echo oops >&2"), None, false).await;
        let log = fs::read_to_string(logs.join(SETUP_LOG)).unwrap();
        assert!(log.starts_with("$ echo hi; echo oops >&2\n"), "{log}");
        assert!(log.contains("hi") && log.contains("oops"), "{log}");
        assert!(log.trim_end().ends_with("exit status: 0"), "{log}");
        let ok: SetupStatus =
            serde_json::from_str(&fs::read_to_string(logs.join(SETUP_STATUS)).unwrap()).unwrap();
        assert_eq!(ok.state, "ok");
        assert_eq!(ok.command, "echo hi; echo oops >&2");
        assert!(ok.finished_at.is_some() && ok.reason.is_none());

        // Next run truncates the log and records the failure + reason.
        let out = ensure_deps(dir.path(), Some("echo boom >&2; exit 3"), None, false).await;
        assert_eq!(out, DepsOutcome::Failed("exit 3: boom".into()));
        let log = fs::read_to_string(logs.join(SETUP_LOG)).unwrap();
        assert!(
            !log.contains("oops"),
            "log must be truncated per run: {log}"
        );
        assert!(log.trim_end().ends_with("exit status: 3"), "{log}");
        let failed: SetupStatus =
            serde_json::from_str(&fs::read_to_string(logs.join(SETUP_STATUS)).unwrap()).unwrap();
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.reason.as_deref(), Some("exit 3: boom"));
    }

    #[tokio::test]
    async fn skipped_run_does_not_touch_setup_files() {
        let dir = TempDir::new().unwrap(); // no package.json: auto-detect skipped
        assert_eq!(
            ensure_deps(dir.path(), None, None, false).await,
            DepsOutcome::UpToDate
        );
        assert!(!dir.path().join(".logs").exists());
    }

    #[test]
    fn auto_detect_always_lockfile_gated() {
        assert_eq!(
            effective_when(None, Some(PreCommandWhen::Always)),
            PreCommandWhen::Lockfile
        );
        assert_eq!(effective_when(Some("x"), None), PreCommandWhen::Always);
        assert_eq!(
            effective_when(Some("x"), Some(PreCommandWhen::Lockfile)),
            PreCommandWhen::Lockfile
        );
    }

    #[test]
    fn detect_install_command_by_lockfile() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_install_command(dir.path()), None);
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_install_command(dir.path()), Some("npm ci"));
        fs::write(dir.path().join("bun.lock"), "").unwrap();
        assert_eq!(detect_install_command(dir.path()), Some("bun install"));
    }

    #[test]
    fn test_touch_marker() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();

        touch_marker(&nm).unwrap();
        assert!(nm.join(".atlas-installed").exists());
    }
}
