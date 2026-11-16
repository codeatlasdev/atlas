//! The `capture` tool of a crewmate: open a URL in real browsers (Playwright's chromium and webkit)
//! at several widths and themes and hand the agent the screenshots, so a UI change is looked at
//! instead of guessed at.
//!
//! | file         | what it owns                                                                |
//! |--------------|-----------------------------------------------------------------------------|
//! | `spec.rs`    | the request, its validation, the runner's report, "every combination is in it" |
//! | `runner.mjs` | the Playwright script (embedded; `atlas crew browsers install` writes it next to the package) |
//! | `detectors.mjs` | TEMPORARY UI detectors the runner imports (ui#1290 stand-in), with `fixtures/`; `detectors.rs` reads their findings |
//! | `package-lock.json` | the lock `npm ci` installs the pinned Playwright from (embedded) |
//! | this file    | running the runner (timeout, killed on cancel), the files, [`attach_captures`] |
//!
//! Where it runs: on the crew server, under `<hostd dir>/capture/` (`package.json`,
//! `package-lock.json`, `runner.mjs`, `node_modules/playwright`, `browsers/`). Nothing here
//! installs anything: that is `atlas crew browsers install`, which runs every npm command as the
//! crew user and uses root only for an `apt-get install` of validated package names.

pub mod detectors;
#[cfg(unix)]
pub mod guard;
pub mod spec;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::json;
use tokio::io::AsyncWriteExt;

use super::store::CrewStore;
pub use spec::{Combo, Reach, Spec};

/// The one place the Playwright version is written in Rust. The browsers it downloads match this
/// exact release, so it moves only together with `package-lock.json` (below) and a re-run of
/// `atlas crew browsers install`.
pub const PLAYWRIGHT_VERSION: &str = "1.65.1";

/// The runner script, written next to the package at install.
pub const RUNNER_MJS: &str = include_str!("runner.mjs");

/// The UI detectors the runner imports (`./detectors.mjs`), written next to it at install.
/// TEMPORARY: it goes when `@codeatlasdev/ui/testing/detectors` (ui#1290) ships.
pub const DETECTORS_MJS: &str = include_str!("detectors.mjs");

/// The lockfile of [`package_json`], written next to it at install and installed with `npm ci`:
/// npm checks every downloaded tarball against the integrity hashes in it, so the crew server runs
/// the `playwright` and `playwright-core` atlas pinned and nothing else, whatever the registry
/// serves later. Regenerate with `npm install --package-lock-only` in a directory holding only
/// [`package_json`] (a test fails while the two disagree).
pub const PACKAGE_LOCK: &str = include_str!("package-lock.json");

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
// Its users (`capture::tests`, `crew::tests::capture`) are Unix-only.
#[cfg(all(test, unix))]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The program that runs the capture, and how long it may take.
// tree-access: the capture runner (node, hostd's own script); no tree input
fn runner_command(dir: &Path) -> Result<(tokio::process::Command, Duration), String> {
    #[cfg(test)]
    if let Some((prog, timeout)) = TEST_RUNNER.lock().clone() {
        // tree-access: the capture runner (node, hostd's own script); no tree input
        return Ok((tokio::process::Command::new(prog), timeout));
    }
    let script = dir.join("runner.mjs");
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if !script.is_file() {
        return Err(format!(
            "capture: the browsers are not installed on this server ({} is missing). \
             Run `atlas crew browsers install <ssh-target>` from your computer; \
             `atlas crew browsers status <ssh-target>` says what else is missing",
            script.display()
        ));
    }
    // tree-access: the capture runner (node, hostd's own script); no tree input
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
    /// The DOM report of each combination, when the spec asked for `dom`, in the same order.
    pub doms: Vec<(Combo, PathBuf)>,
    /// The detectors' findings of each combination, when the spec asked for `checks`, in the same
    /// order: a JSON list of [`detectors::Finding`].
    pub findings: Vec<(Combo, PathBuf)>,
}

/// Run the runner for `spec`, writing PNGs under `out_dir`, and check the report covers every
/// combination. The child is killed when this future is dropped (a cancelled task) or when the
/// timeout passes.
pub async fn run(hostd_dir: &Path, spec: &Spec, out_dir: &Path) -> Result<Captured, String> {
    let dir = capture_dir(hostd_dir);
    // For an isolated crewmate: the URL first (before any browser starts), then a proxy the browser is started with
    // that decides every connection the page makes, each hop of a redirect included (`guard`).
    #[cfg(unix)]
    let guard = match &spec.reach {
        Some(reach) => {
            reach.check_url(&spec.url).await?;
            Some(
                guard::Guard::start(reach)
                    .await
                    .map_err(|e| format!("capture: cannot start the network guard: {e}"))?,
            )
        }
        None => None,
    };
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
    #[cfg(unix)]
    if let Some(g) = &guard {
        input["proxy"] = json!(g.url());
    }
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
    let mut doms = vec![];
    let mut findings = vec![];
    for cell in cells {
        let combo = cell.combo;
        for (what, path) in [
            ("screenshot", Some(cell.png)),
            ("DOM report", cell.dom),
            ("findings", cell.findings),
        ] {
            let Some(path) = path else { continue };
            let p = PathBuf::from(&path);
            // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
            if !p.is_file() {
                return Err(format!(
                    "capture incomplete. {combo}: the runner reported {path} but the {what} file does not exist"
                ));
            }
            match what {
                "screenshot" => files.push((combo, p)),
                "DOM report" => doms.push((combo, p)),
                _ => findings.push((combo, p)),
            }
        }
    }
    Ok(Captured {
        files,
        doms,
        findings,
    })
}

/// No isolation off Unix: there is no crewmate user to protect another one from.
#[cfg(not(unix))]
impl Reach {
    pub fn of(_: &CrewStore, _: &str) -> Result<Option<Self>, String> {
        Ok(None)
    }
}

/// The runner runs in the capture dir (Node finds `node_modules` from the script, not the cwd, but a
/// fake runner and relative paths expect a real directory); fall back to the current one.
fn dir_or_cwd(dir: &Path) -> PathBuf {
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if dir.is_dir() {
        dir.to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

/// Attach `files` to the task as images of the agent ([`crate::crew::attachments::SOURCE_AGENT`]) and
/// return the names they are kept under. Storage, caps, names and the sniff of the bytes are
/// `crew::attachments`'s; the thread, the cards and the ui gate read them from the task.
pub fn attach_captures(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    files: &[PathBuf],
) -> Result<Vec<String>, String> {
    crate::crew::attachments::store_agent(store, crewmate_id, task_id, files)
}

/// Removes a directory when dropped: the scratch dir of a run, whatever way the run ended.
pub(crate) struct RemoveOnDrop(pub(crate) PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Where the DOM reports of one task's captures are kept: `<crewmate dir>/dom/<task id>/`.
pub fn task_dom_dir(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
) -> Result<PathBuf, String> {
    if !crate::crew::pane::is_plain_name(task_id) {
        return Err(format!("capture: {task_id:?} is not a task id"));
    }
    Ok(store.crewmate_dir(crewmate_id).join("dom").join(task_id))
}

/// A fresh scratch directory path for one run (`<capture dir>/runs/<id>`; the caller creates it).
pub(crate) fn scratch_dir(hostd_dir: &Path) -> PathBuf {
    capture_dir(hostd_dir)
        .join("runs")
        .join(crate::paths::new_id())
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
    let dom_dir = task_dom_dir(store, crewmate_id, task_id)?;
    let mut spec = Spec::from_args(args)?;
    spec.reach = Reach::of(store, crewmate_id)?;
    // The runner writes into a scratch dir that is removed whatever happens; what is kept is the copy
    // `attach_captures` stores with the task.
    let scratch = scratch_dir(hostd_dir);
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("capture: cannot make {}: {e}", scratch.display()))?;
    let _cleanup = RemoveOnDrop(scratch.clone());
    let captured = run(hostd_dir, &spec, &scratch).await?;
    let paths: Vec<PathBuf> = captured.files.iter().map(|(_, p)| p.clone()).collect();
    let names = attach_captures(store, crewmate_id, task_id, &paths)
        .map_err(|e| format!("capture: cannot keep the screenshots: {e}"))?;
    // The DOM reports and the findings are JSON, not images: they are kept in the task's own `dom/`
    // folder (the scratch dir goes away with this call), never as attachments.
    let mut kept: std::collections::BTreeMap<PathBuf, String> = Default::default();
    if !captured.doms.is_empty() || !captured.findings.is_empty() {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        std::fs::create_dir_all(&dom_dir)
            .map_err(|e| format!("capture: cannot make {}: {e}", dom_dir.display()))?;
    }
    for (_, p) in captured.doms.iter().chain(&captured.findings) {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("capture: bad report name {}", p.display()))?
            .to_string();
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        std::fs::copy(p, dom_dir.join(&name))
            .map_err(|e| format!("capture: cannot keep the report {name}: {e}"))?;
        kept.insert(p.clone(), name);
    }
    let shots: Vec<_> = captured
        .files
        .iter()
        .zip(&names)
        .map(|((c, _), name)| {
            let mut shot = json!({
                "engine": c.engine.as_str(), "width": c.width, "theme": c.theme.as_str(),
                "name": name,
                // Where the agent opens it: its copy in the work dir (staged when the task starts again).
                "path": format!("{}/{task_id}/{name}", crate::crew::attachments::WORK_SUBDIR),
            });
            if let Some((_, p)) = captured.doms.iter().find(|(dc, _)| dc == c) {
                shot["dom"] = json!(kept[p]);
                shot["dom_path"] = json!(dom_dir.join(&kept[p]));
            }
            if let Some((_, p)) = captured.findings.iter().find(|(dc, _)| dc == c) {
                shot["findings_file"] = json!(kept[p]);
                shot["findings_path"] = json!(dom_dir.join(&kept[p]));
                // What the agent acts on, without a second read: a report it cannot read is an error.
                shot["findings"] = match detectors::read(p) {
                    Ok(f) => json!(f),
                    Err(e) => json!({ "error": e }),
                };
            }
            shot
        })
        .collect();
    Ok(json!({"url": spec.url, "visited": shots.len(), "shots": shots}))
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
