//! Atlas's own update, as a phone asks for it: what is on offer (read from the CLI's cache, or
//! asked for now) and starting `atlas update` on this computer.
//!
//! hostd never downloads or replaces anything itself: it runs the same `atlas update` a person
//! runs (this daemon's own binary), so the install-method rules, the checksum, the lock, the
//! rollback and the terminal-host handoff are the CLI's, in one place. What lives here is the
//! plumbing around one run: one at a time, an audit line, and the CLI's `@@atlas` progress lines
//! turned into `atlas_update` events.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use atlas_core::channel;
use atlas_core::sys::CommandExt as _;
use atlas_core::update_cache::{self, UpdateCache};
use serde_json::{Value, json};

use crate::paths;
use crate::server::Hub;

/// A check asks GitHub (through `curl`): bounded, so a dead network cannot hold a request for long.
const CHECK_DEADLINE: Duration = Duration::from_secs(45);
/// How often the output of a running update is read.
const POLL: Duration = Duration::from_millis(150);

/// What this daemon knows about updating itself.
pub(crate) struct Update {
    /// The `atlas` that runs the update; `None`: this process's own binary.
    exe: Option<PathBuf>,
    /// An update started by [`start`] is still running.
    running: AtomicBool,
    /// One check at a time: each is a process and a request to GitHub, and a phone that taps
    /// "Check now" repeatedly must not fan out into a crowd of them.
    checking: tokio::sync::Mutex<()>,
}

impl Update {
    pub(crate) fn new(exe: Option<PathBuf>) -> Self {
        Self {
            exe,
            running: AtomicBool::new(false),
            checking: tokio::sync::Mutex::new(()),
        }
    }

    fn exe(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.exe {
            return Ok(p.clone());
        }
        std::env::current_exe()
            .map(live_path)
            .map_err(|e| format!("cannot locate the atlas binary: {e}"))
    }
}

/// Linux names the binary of a process whose file was replaced `<path> (deleted)`: a daemon that
/// outlived an update (its terminals kept it up) would otherwise try to run a path that is not
/// there. The path itself is the new binary.
fn live_path(exe: PathBuf) -> PathBuf {
    let shown = exe.to_string_lossy();
    match shown.strip_suffix(" (deleted)") {
        Some(path) if Path::new(path).exists() => PathBuf::from(path),
        _ => exe,
    }
}

/// `info.update`: what the last check of the channel this machine follows found, read from the file
/// the CLI keeps in the registry dir (`~/.atlas`). No network. `None` when no check is on record.
pub(crate) fn cached(registry: &Path) -> Option<Value> {
    let cache = UpdateCache::load_at(&registry.join(update_cache::FILE));
    let followed = channel::effective(
        std::env::var(channel::ENV).ok().as_deref(),
        channel::read_from(&registry.join(channel::FILE)),
    );
    let summary = cache.summary(env!("CARGO_PKG_VERSION"), followed)?;
    serde_json::to_value(summary).ok()
}

/// `atlas_update_check`: the CLI's own `update --check --json` report (which also refreshes the cache).
pub(crate) async fn check(hub: &Hub) -> Result<Value, String> {
    let _one = hub.update.checking.lock().await;
    let exe = hub.update.exe()?;
    let login = tokio::task::spawn_blocking(atlas_core::login_env::vars)
        .await
        .unwrap_or_default();
    let mut cmd = tokio::process::Command::new(exe);
    cmd.no_window()
        .args(["update", "--check", "--json"])
        .envs(login)
        .env(paths::DIR_ENV, &hub.dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Dropped on the deadline: the check must not outlive the request.
        .kill_on_drop(true);
    let out = tokio::time::timeout(CHECK_DEADLINE, cmd.output())
        .await
        .map_err(|_| "the update check took too long: is the computer online?".to_string())?
        .map_err(|e| format!("cannot run atlas: {e}"))?;
    if !out.status.success() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        return Err(failure(&text, out.status.code()));
    }
    let report: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("the update check answered something unreadable: {e}"))?;
    if report["installed"].is_string() && report["action"].is_string() {
        Ok(report)
    } else {
        Err("the update check answered something unexpected".into())
    }
}

/// What an `atlas_update` is going to do, from a check report.
#[derive(Debug, PartialEq)]
struct Plan {
    from: String,
    to: String,
}

/// Decide from `atlas update --check --json`'s report whether an update may start from here, with the
/// CLI's own words for why not. Only an `update` action installs: a `managed`/`package` one belongs to
/// Homebrew or a package manager (they need a terminal and, for a package, root), a `downgrade` is
/// never made from a phone.
fn plan(report: &Value) -> Result<Plan, String> {
    let installed = report["installed"].as_str().unwrap_or("?");
    let offered = report["offered"].as_str();
    match (report["action"].as_str(), offered) {
        (Some("update"), Some(to)) => Ok(Plan {
            from: installed.to_string(),
            to: to.to_string(),
        }),
        (Some(action @ ("managed" | "package")), offered) => {
            let who = match (action, report["manager"].as_str()) {
                (_, Some(pm)) => format!("{pm}, a system package"),
                ("managed", None) => "Homebrew".to_string(),
                _ => "a system package".to_string(),
            };
            let to = offered.map(|v| format!(" to v{v}")).unwrap_or_default();
            let run = report["command"]
                .as_str()
                .map(|c| format!(": run `{c}` there"))
                .unwrap_or_default();
            Err(format!(
                "Atlas on this computer is installed by {who}, so it is updated there{to}{run}"
            ))
        }
        (Some("downgrade"), Some(to)) => Err(format!(
            "this channel offers v{to}, older than the v{installed} that runs: nothing to update"
        )),
        _ => Err(format!("Atlas v{installed} is up to date")),
    }
}

/// Clears "an update runs" when the update's watcher ends, however it ends.
struct Running(Arc<Hub>);

impl Running {
    fn take(hub: &Arc<Hub>) -> Result<Self, String> {
        hub.update
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self(hub.clone()))
            .map_err(|_| "an update is already running".to_string())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.0.update.running.store(false, Ordering::Release);
    }
}

/// `atlas_update`: check, then run `atlas update --yes` detached and report its progress as
/// `atlas_update` events. Replies once it is running (`{started, from, to}`). `by` is who asked
/// (`phone:<node id>` or `desktop`).
pub(crate) async fn start(hub: &Arc<Hub>, by: &str) -> Result<Value, String> {
    // Taken before the slow part: two taps are one update, whichever check answers first.
    let running = Running::take(hub)?;
    let plan = plan(&check(hub).await?)?;
    let exe = hub.update.exe()?;
    let login = tokio::task::spawn_blocking(atlas_core::login_env::vars)
        .await
        .unwrap_or_default();

    // The output goes to a file: the update's last step replaces this daemon, and a pipe with no
    // reader would make the update's own prints fail.
    let log_path = paths::update_log(&hub.dir);
    let log = atlas_core::sys::owner_only(
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true),
    )
    .open(&log_path)
    .map_err(|e| format!("cannot write {}: {e}", log_path.display()))?;
    let reader = File::open(&log_path).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(&exe);
    // Never `--downgrade`: a phone installs what the channel moved forward to, nothing older.
    cmd.args(["update", "--yes"])
        .envs(login)
        .env(paths::DIR_ENV, &hub.dir)
        .env("NO_COLOR", "1")
        .env("ATLAS_UPDATE_PROGRESS", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    let child = atlas_core::sys::spawn_detached(&mut cmd)
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;

    // Only now is it an update that started.
    hub.mobile.audit(
        by.strip_prefix("phone:").unwrap_or(by),
        "atlas_update_started",
        &json!({"by": by, "from": plan.from, "to": plan.to}),
    );
    let reply = json!({"started": true, "from": plan.from, "to": plan.to});
    let hub = hub.clone();
    std::thread::spawn(move || watch(&hub, child, reader, &plan.to, running));
    Ok(reply)
}

/// Follow a running update: its output lines become events, its exit the last one.
fn watch(hub: &Hub, mut child: std::process::Child, mut out: File, to: &str, running: Running) {
    let mut progress = Progress::new(to);
    hub.broadcast(progress.event(progress.stage, progress.message()));
    let mut partial = Vec::new();
    loop {
        // Its exit is read first: whatever it printed before is then in the file for the read below.
        let status = child.try_wait();
        drain(&mut out, &mut partial, |line| {
            if let Some(stage) = progress.feed(line) {
                hub.broadcast(progress.event(stage, progress.message()));
            }
        });
        let (success, code) = match status {
            Ok(None) => {
                std::thread::sleep(POLL);
                continue;
            }
            Ok(Some(st)) => (st.success(), st.code()),
            Err(e) => {
                progress.error = Some(format!("lost track of the update: {e}"));
                (false, None)
            }
        };
        if !partial.is_empty() {
            progress.feed(&String::from_utf8_lossy(&partial));
        }
        let (stage, message) = progress.outcome(success, code);
        // Free before the last word, so a retry sent the moment a phone reads it is not refused.
        drop(running);
        hub.broadcast(progress.event(stage, message));
        return;
    }
}

/// Everything `out` has gained since the last call, as complete lines (an unfinished one waits in `partial`).
fn drain(out: &mut File, partial: &mut Vec<u8>, mut line: impl FnMut(&str)) {
    let _ = out.read_to_end(partial);
    while let Some(end) = partial.iter().position(|&b| b == b'\n') {
        let text: Vec<u8> = partial.drain(..=end).collect();
        line(&String::from_utf8_lossy(&text));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Downloading,
    Installing,
    HandingOff,
    Done,
    Failed,
}

impl Stage {
    fn id(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Installing => "installing",
            Self::HandingOff => "handing_off",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

/// What the CLI's output says about one `atlas update --yes`. The `@@atlas` lines are the same ones
/// Atlas Desktop reads (`update_core` in the CLI): `phase <id>`, `host <state> <n>`, `managed <how>`.
struct Progress {
    version: String,
    stage: Stage,
    managed: Option<String>,
    host: Option<(String, u64)>,
    /// The first `error:` line the CLI printed, and the last line of any kind.
    error: Option<String>,
    last: Option<String>,
}

impl Progress {
    fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            stage: Stage::Downloading,
            managed: None,
            host: None,
            error: None,
            last: None,
        }
    }

    /// One line of output. `Some(stage)` when it moved the update to a later one (stages never go back).
    fn feed(&mut self, line: &str) -> Option<Stage> {
        let line = atlas_tui::runtime::text::strip_ansi(line);
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let Some(rest) = line.strip_prefix("@@atlas ") else {
            if let Some(e) = line.strip_prefix("error:") {
                self.error.get_or_insert_with(|| e.trim().to_string());
            }
            self.last = Some(line.to_string());
            return None;
        };
        let mut words = rest.split_whitespace();
        match words.next()? {
            "phase" => {
                let next = match words.next()? {
                    "download" | "verify" => Stage::Downloading,
                    "install-cli" | "update-app" => Stage::Installing,
                    "terminal-host" => Stage::HandingOff,
                    _ => return None,
                };
                (next > self.stage).then(|| {
                    self.stage = next;
                    next
                })
            }
            "managed" => {
                self.managed = words.next().map(str::to_string);
                None
            }
            "host" => {
                let state = words.next()?.to_string();
                let live = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                self.host = Some((state, live));
                None
            }
            _ => None,
        }
    }

    /// What a running `stage` says.
    fn message(&self) -> String {
        let v = &self.version;
        match self.stage {
            Stage::Downloading => format!("Downloading Atlas v{v}"),
            Stage::Installing => format!("Installing Atlas v{v}"),
            Stage::HandingOff => {
                format!("Switching the terminal host to v{v}; terminals keep running")
            }
            Stage::Done | Stage::Failed => String::new(),
        }
    }

    /// The last stage and its message once the process ended.
    fn outcome(&self, success: bool, code: Option<i32>) -> (Stage, String) {
        let v = &self.version;
        if let Some(how) = &self.managed {
            // Nothing was installed and the CLI said so with a zero exit: never "done".
            let who = if how == "homebrew" {
                "Homebrew"
            } else {
                "a system package manager"
            };
            return (
                Stage::Failed,
                format!("Nothing was installed: {who} owns this Atlas, update it there"),
            );
        }
        if !success {
            let message = self
                .error
                .clone()
                .or_else(|| self.last.clone())
                .unwrap_or_else(|| match code {
                    Some(c) => format!("atlas update failed (exit code {c})"),
                    None => "atlas update was ended".to_string(),
                });
            return (Stage::Failed, message);
        }
        let message = match &self.host {
            Some((state, live)) if state == "needs-restart" => format!(
                "Atlas v{v} is installed. The terminal host keeps the old version until its {live} running terminal{} end; restart it then with `atlas hostd --restart`",
                if *live == 1 { "" } else { "s" }
            ),
            Some((state, _)) if state == "failed" => format!(
                "Atlas v{v} is installed, but the terminal host could not be switched: run `atlas hostd --check` on the computer"
            ),
            _ => format!("Atlas v{v} is installed"),
        };
        (Stage::Done, message)
    }

    /// The `atlas_update` push line.
    fn event(&self, stage: Stage, message: String) -> String {
        json!({
            "ev": "atlas_update",
            "stage": stage.id(),
            "message": message,
            "version": self.version,
        })
        .to_string()
    }
}

/// The CLI's own words for why it failed: its `error:` line, else its last line.
fn failure(text: &str, code: Option<i32>) -> String {
    let mut p = Progress::new("");
    for line in text.lines() {
        p.feed(line);
    }
    p.outcome(false, code).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(p: &mut Progress, lines: &[&str]) -> Vec<Stage> {
        lines.iter().filter_map(|l| p.feed(l)).collect()
    }

    #[test]
    fn the_cli_phases_become_stages_in_order_and_never_go_back() {
        let mut p = Progress::new("1.6.0");
        let stages = feed_all(
            &mut p,
            &[
                "◆ Downloading v1.6.0...",
                "@@atlas phase download",
                "@@atlas bytes 10 100",
                "@@atlas phase verify",
                "@@atlas phase install-cli",
                "@@atlas phase update-app",
                "@@atlas phase terminal-host",
                // A late or repeated phase never moves it back.
                "@@atlas phase verify",
                "@@atlas phase made-up",
            ],
        );
        assert_eq!(stages, [Stage::Installing, Stage::HandingOff]);
        assert_eq!(p.stage, Stage::HandingOff);
    }

    #[test]
    fn a_failure_is_worded_by_the_cli_error_line_not_its_chatter() {
        let mut p = Progress::new("1.6.0");
        feed_all(
            &mut p,
            &[
                "@@atlas phase download",
                "\x1b[1;31merror:\x1b[0m Download failed. Releases: https://x/releases",
                "  caused by: curl exited 22",
            ],
        );
        let (stage, message) = p.outcome(false, Some(1));
        assert_eq!(stage, Stage::Failed);
        assert_eq!(message, "Download failed. Releases: https://x/releases");
        // No error line: the last thing it said; nothing at all: the exit code.
        let mut p = Progress::new("1.6.0");
        feed_all(&mut p, &["Checksum mismatch"]);
        assert_eq!(p.outcome(false, Some(2)).1, "Checksum mismatch");
        assert_eq!(
            Progress::new("1.6.0").outcome(false, Some(3)).1,
            "atlas update failed (exit code 3)"
        );
    }

    /// The CLI exits 0 when Homebrew or a package manager owns the install (it printed what to run).
    /// Telling the phone "done" would be a lie: nothing was installed.
    #[test]
    fn a_managed_install_is_failed_even_though_the_cli_exits_zero() {
        let mut p = Progress::new("1.6.0");
        feed_all(&mut p, &["@@atlas managed homebrew"]);
        let (stage, message) = p.outcome(true, Some(0));
        assert_eq!(stage, Stage::Failed);
        assert!(message.contains("Homebrew"), "{message}");
    }

    #[test]
    fn done_says_when_the_terminal_host_could_not_follow() {
        let mut p = Progress::new("1.6.0");
        assert_eq!(p.outcome(true, Some(0)).1, "Atlas v1.6.0 is installed");
        feed_all(&mut p, &["@@atlas host needs-restart 2"]);
        let (stage, message) = p.outcome(true, Some(0));
        assert_eq!(stage, Stage::Done);
        assert!(message.contains("2 running terminals"), "{message}");
        feed_all(&mut p, &["@@atlas host needs-restart 1"]);
        assert!(p.outcome(true, Some(0)).1.contains("1 running terminal "));
        feed_all(&mut p, &["@@atlas host failed 0"]);
        assert!(p.outcome(true, Some(0)).1.contains("could not be switched"));
        // A live upgrade or a restart is a plain success.
        feed_all(&mut p, &["@@atlas host upgraded 3"]);
        assert_eq!(p.outcome(true, Some(0)).1, "Atlas v1.6.0 is installed");
    }

    #[test]
    fn only_an_update_action_may_start_and_the_others_say_why() {
        let report = |action: &str, extra: Value| {
            let mut r = json!({"installed": "1.5.0", "channel": "alpha", "offered": "1.6.0", "tag": "v1.6.0", "action": action});
            r.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            r
        };
        assert_eq!(
            plan(&report("update", json!({}))),
            Ok(Plan {
                from: "1.5.0".into(),
                to: "1.6.0".into()
            })
        );
        let brew = plan(&report(
            "managed",
            json!({"managed": true, "manager": null, "command": "brew upgrade atlas"}),
        ))
        .unwrap_err();
        assert!(
            brew.contains("Homebrew") && brew.contains("`brew upgrade atlas`"),
            "{brew}"
        );
        let pkg = plan(&report(
            "package",
            json!({"managed": true, "manager": "pacman", "command": "sudo pacman -U x"}),
        ))
        .unwrap_err();
        assert!(pkg.contains("pacman") && pkg.contains("v1.6.0"), "{pkg}");
        let down = plan(&report("downgrade", json!({}))).unwrap_err();
        assert!(down.contains("older"), "{down}");
        let none =
            plan(&json!({"installed": "1.5.0", "offered": null, "action": "none"})).unwrap_err();
        assert_eq!(none, "Atlas v1.5.0 is up to date");
        // An `update` with nothing offered is not a thing to install.
        assert!(plan(&json!({"installed": "1.5.0", "offered": null, "action": "update"})).is_err());
    }

    #[test]
    fn a_binary_replaced_under_a_running_daemon_is_found_at_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("atlas");
        std::fs::write(&exe, "x").unwrap();
        let deleted = PathBuf::from(format!("{} (deleted)", exe.display()));
        assert_eq!(live_path(deleted), exe);
        // Nothing at that path: left as it is (the spawn then says why).
        let gone = PathBuf::from(format!("{} (deleted)", dir.path().join("none").display()));
        assert_eq!(live_path(gone.clone()), gone);
        assert_eq!(live_path(exe.clone()), exe);
    }

    #[test]
    fn lines_are_cut_at_newlines_and_an_unfinished_one_waits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        let mut w = File::create(&path).unwrap();
        let mut r = File::open(&path).unwrap();
        let (mut partial, mut seen) = (Vec::new(), Vec::new());
        use std::io::Write;
        w.write_all(b"one\ntw").unwrap();
        drain(&mut r, &mut partial, |l| seen.push(l.to_string()));
        assert_eq!(seen, ["one\n"]);
        w.write_all(b"o\r\nthree").unwrap();
        drain(&mut r, &mut partial, |l| seen.push(l.to_string()));
        assert_eq!(seen, ["one\n", "two\r\n"]);
        assert_eq!(partial, b"three");
    }
}
