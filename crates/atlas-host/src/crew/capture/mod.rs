//! The `capture` tool of a crewmate: open a URL in real browsers (Playwright's chromium and webkit)
//! at several widths and themes and hand the agent the screenshots, so a UI change is looked at
//! instead of guessed at.
//!
//! | file         | what it owns                                                                |
//! |--------------|-----------------------------------------------------------------------------|
//! | `spec.rs`    | the request, its validation, the runner's report, "every combination is in it" |
//! | `runner.mjs` | the Playwright script (embedded; `atlas crew browsers install` writes it next to the package) |
//! | this file    | running the runner (timeout, killed on cancel), the files, [`attach_captures`] |
//!
//! Where it runs: on the crew server, under `<hostd dir>/capture/` (`package.json`, `runner.mjs`,
//! `node_modules/playwright`, `browsers/`). Nothing here installs anything: that is
//! `atlas crew browsers install` (root, once, because Playwright's `--with-deps` uses apt).

pub mod spec;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::json;
use tokio::io::AsyncWriteExt;

use super::store::CrewStore;
pub use spec::{Combo, Spec};

/// The one place the Playwright version is written. The browsers it downloads match this exact
/// release, so it moves only together with a re-run of `atlas crew browsers install`.
pub const PLAYWRIGHT_VERSION: &str = "1.65.1";

/// The runner script, written next to the package at install.
pub const RUNNER_MJS: &str = include_str!("runner.mjs");

/// `package.json` of the capture dir: an ES module project that depends on the pinned Playwright.
pub fn package_json() -> String {
    format!(
        "{{\"name\":\"atlas-crew-capture\",\"private\":true,\"type\":\"module\",\"dependencies\":{{\"playwright\":\"{PLAYWRIGHT_VERSION}\"}}}}\n"
    )
}

/// How long one capture may run before the runner is killed (a slow page across 16 cells).
const TIMEOUT: Duration = Duration::from_secs(300);

/// Test seams, set under [`TEST_LOCK`]: a program to run in place of `node runner.mjs` (it reads the
/// spec on stdin and prints the report, like the real runner) and a shorter timeout. They do not
/// exist in a release build.
#[cfg(test)]
pub(crate) static TEST_RUNNER: parking_lot::Mutex<Option<(PathBuf, Duration)>> =
    parking_lot::Mutex::new(None);
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The program that runs the capture, and how long it may take.
fn runner_command(dir: &Path) -> Result<(tokio::process::Command, Duration), String> {
    #[cfg(test)]
    if let Some((prog, timeout)) = TEST_RUNNER.lock().clone() {
        return Ok((tokio::process::Command::new(prog), timeout));
    }
    let script = dir.join("runner.mjs");
    if !script.is_file() {
        return Err(format!(
            "capture: the browsers are not installed on this server ({} is missing). \
             Run `atlas crew browsers install <ssh-target>` from your computer; \
             `atlas crew browsers status <ssh-target>` says what else is missing",
            script.display()
        ));
    }
    let mut c = tokio::process::Command::new("node");
    c.arg(script);
    Ok((c, TIMEOUT))
}

/// `<hostd dir>/capture`.
pub fn capture_dir(hostd_dir: &Path) -> PathBuf {
    hostd_dir.join("capture")
}

/// What a finished capture hands back.
#[derive(Debug)]
pub struct Captured {
    /// One PNG per requested combination, in matrix order, in `out_dir`.
    pub files: Vec<(Combo, PathBuf)>,
}

/// Run the runner for `spec`, writing PNGs under `out_dir`, and check the report covers every
/// combination. The child is killed when this future is dropped (a cancelled task) or when the
/// timeout passes.
pub async fn run(hostd_dir: &Path, spec: &Spec, out_dir: &Path) -> Result<Captured, String> {
    let dir = capture_dir(hostd_dir);
    let (mut cmd, timeout) = runner_command(&dir)?;
    cmd.current_dir(dir_or_cwd(&dir))
        .env("PLAYWRIGHT_BROWSERS_PATH", dir.join("browsers"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("capture: cannot start the runner (is node installed?): {e}"))?;

    let mut input = json!(spec);
    input["out_dir"] = json!(out_dir);
    let mut stdin = child.stdin.take().expect("piped");
    // The runner reads all of stdin before it prints; a write error means it already died, which
    // the exit status below reports with its stderr.
    let _ = stdin.write_all(input.to_string().as_bytes()).await;
    drop(stdin);

    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(r) => r.map_err(|e| format!("capture: the runner failed: {e}"))?,
        // `wait_with_output` consumed the child; dropping that future killed it (`kill_on_drop`).
        Err(_) => {
            return Err(format!(
                "capture: the runner took more than {} s and was stopped",
                timeout.as_secs()
            ));
        }
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err
            .chars()
            .rev()
            .take(600)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!(
            "capture: the runner exited with {}: {}",
            out.status,
            tail.trim()
        ));
    }
    let report = spec::parse_report(&String::from_utf8_lossy(&out.stdout))?;
    let cells = spec::check_report(spec, &report)?;
    let mut files = vec![];
    for (combo, path) in cells {
        let p = PathBuf::from(&path);
        if !p.is_file() {
            return Err(format!(
                "capture incomplete. {combo}: the runner reported {path} but the file does not exist"
            ));
        }
        files.push((combo, p));
    }
    Ok(Captured { files })
}

/// The runner runs in the capture dir (Node finds `node_modules` from the script, not the cwd, but a
/// fake runner and relative paths expect a real directory); fall back to the current one.
fn dir_or_cwd(dir: &Path) -> PathBuf {
    if dir.is_dir() {
        dir.to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

/// Where the screenshots of one task are kept: `<crewmate dir>/captures/<task id>/`.
pub fn task_captures_dir(store: &CrewStore, crewmate_id: &str, task_id: &str) -> PathBuf {
    store
        .crewmate_dir(crewmate_id)
        .join("captures")
        .join(task_id)
}

/// Attach `files` to the task and return the names they are kept under.
///
/// For now they are copied into the task's `captures/` directory. // #209: replace with
/// crew::attachments::store_agent(..) (source "agent"), which owns the storage, the caps and the
/// work-dir copy; this function is the one call to swap.
pub fn attach_captures(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    files: &[PathBuf],
) -> std::io::Result<Vec<String>> {
    let dir = task_captures_dir(store, crewmate_id, task_id);
    std::fs::create_dir_all(&dir)?;
    let mut names = vec![];
    for f in files {
        let name = f
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| std::io::Error::other(format!("bad file name: {}", f.display())))?
            .to_string();
        let dest = dir.join(&name);
        if f != &dest {
            std::fs::copy(f, &dest)?;
        }
        names.push(name);
    }
    Ok(names)
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What the `capture` hook route does: validate, run, attach, and answer with the combinations
/// visited. `Err` is the message the agent sees as the tool's error.
pub async fn handle(
    hostd_dir: &Path,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let spec = Spec::from_args(args)?;
    // The runner writes into a scratch dir that is removed whatever happens; what is kept is the copy
    // `attach_captures` makes.
    let scratch = capture_dir(hostd_dir)
        .join("runs")
        .join(crate::paths::new_id());
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("capture: cannot make {}: {e}", scratch.display()))?;
    let _cleanup = RemoveOnDrop(scratch.clone());
    let captured = run(hostd_dir, &spec, &scratch).await?;
    let paths: Vec<PathBuf> = captured.files.iter().map(|(_, p)| p.clone()).collect();
    let names = attach_captures(store, crewmate_id, task_id, &paths)
        .map_err(|e| format!("capture: cannot keep the screenshots: {e}"))?;
    let shots: Vec<_> = captured
        .files
        .iter()
        .zip(&names)
        .map(|((c, _), name)| {
            json!({
                "engine": c.engine.as_str(), "width": c.width, "theme": c.theme.as_str(),
                "name": name,
                "path": task_captures_dir(store, crewmate_id, task_id).join(name),
            })
        })
        .collect();
    Ok(json!({"url": spec.url, "visited": shots.len(), "shots": shots}))
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
