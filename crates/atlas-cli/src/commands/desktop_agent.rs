//! `atlas desktop --agent`: an isolated Atlas Desktop for an agent to debug, screenshot and record, which never
//! appears on the user's desktop.
//!
//! The instance runs with `ATLAS_AGENT=1` (debug builds only: no Dock icon or tray, never focused, its window parked
//! on a private virtual display, see `crates/atlas-desktop/src/agent_mode.rs`), its own `HOME`, `ATLAS_HOSTD_DIR`
//! and `ATLAS_DEBUG_SOCK`, so any number of agents run side by side without sharing a bridge, a hostd or a config.
//! Everything of one instance lives in one short directory (`/tmp/atlas-agent-<name>`; a unix socket path is limited
//! to 104 bytes). `--stop` ends exactly the processes recorded there, never a process it did not start.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Longest a unix socket path may be on macOS (`sun_path` is 104 bytes including the NUL).
const MAX_SOCKET_PATH: usize = 103;
const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Where one instance keeps its files.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub name: String,
    pub dir: PathBuf,
}

impl Session {
    pub fn home(&self) -> PathBuf {
        self.dir.join("home")
    }
    pub fn hostd(&self) -> PathBuf {
        self.dir.join("hostd")
    }
    pub fn socket(&self) -> PathBuf {
        self.dir.join("debug.sock")
    }
    pub fn log(&self) -> PathBuf {
        self.dir.join("app.log")
    }
    pub fn record(&self) -> PathBuf {
        self.dir.join("agent.json")
    }
}

/// What `agent.json` remembers, so `--stop` finds exactly what was started.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub name: String,
    pub pid: u32,
    pub socket: PathBuf,
    pub hostd: PathBuf,
    pub exe: PathBuf,
}

/// A name is a short directory component: letters, digits, `-`, `_`.
pub fn valid_name(n: &str) -> Result<(), String> {
    if n.is_empty() || n.len() > 24 {
        return Err("name must be 1..=24 characters".into());
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("name may only contain letters, digits, - and _".into());
    }
    Ok(())
}

/// The session for `name` under `root` (the temp root; a parameter so tests use their own).
pub fn session_in(root: &Path, name: &str) -> Result<Session> {
    valid_name(name).map_err(anyhow::Error::msg)?;
    let s = Session {
        name: name.to_string(),
        dir: root.join(format!("atlas-agent-{name}")),
    };
    let sock = s.socket();
    if sock.as_os_str().len() > MAX_SOCKET_PATH {
        bail!(
            "the socket path {} is {} bytes; macOS allows {MAX_SOCKET_PATH}. Use a shorter name",
            sock.display(),
            sock.as_os_str().len()
        );
    }
    Ok(s)
}

/// A name nobody uses yet: `a1`, `a2`, …
pub fn next_name(root: &Path) -> String {
    (1..)
        .map(|n| format!("a{n}"))
        .find(|n| !root.join(format!("atlas-agent-{n}")).exists())
        .expect("an unbounded range has a free name")
}

/// The environment of the instance. `PATH`, `SHELL`, `LANG` come from the caller so terminals inside it work; the
/// five variables that isolate it are set here.
pub fn app_env(s: &Session, atlas_bin: &Path) -> Vec<(String, String)> {
    let p = |p: PathBuf| p.to_string_lossy().into_owned();
    vec![
        ("HOME".into(), p(s.home())),
        ("ATLAS_HOSTD_DIR".into(), p(s.hostd())),
        ("ATLAS_DEBUG_SOCK".into(), p(s.socket())),
        ("ATLAS_AGENT".into(), "1".into()),
        ("ATLAS_BIN".into(), p(atlas_bin.to_path_buf())),
    ]
}

/// The caller's variables an instance inherits despite the cleared environment: what terminals inside it need, and
/// the display scale knob. Everything else (tokens, the user's `ATLAS_*` paths, `HOME`) stays out.
pub const FORWARDED: [&str; 7] = [
    "PATH",
    "SHELL",
    "LANG",
    "TERM",
    "ATLAS_AGENT_SCALE",
    "ATLAS_AGENT_DRYRUN",
    "ATLAS_MIGRATE_E2E_FROM", // scripts/migration-e2e.sh (debug builds only)
];

pub fn forwarded_env(
    vars: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    vars.into_iter()
        .filter(|(k, _)| FORWARDED.iter().any(|f| k == f))
        .collect()
}

/// `--scratch`: seed the instance's HOME with the user's `~/.atlas/desktop` (settings, theme, layout) and nothing
/// else: no projects registry, no tokens, no sockets, no hook endpoints. Returns how many files were copied.
pub fn seed_scratch(user_home: &Path, s: &Session) -> Result<usize> {
    let from = user_home.join(".atlas/desktop");
    let to = s.home().join(".atlas/desktop");
    std::fs::create_dir_all(&to)?;
    let mut n = 0;
    let Ok(rd) = std::fs::read_dir(&from) else {
        return Ok(0);
    };
    for e in rd.flatten() {
        let ft = e.file_type()?;
        // Regular files only: no symlinks out of the tree, no sockets, no nested state.
        if ft.is_file() && e.file_name() != "projects.json" {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
            n += 1;
        }
    }
    Ok(n)
}

/// The line agents wait for; the socket is what they export as `ATLAS_DEBUG_SOCK`.
pub fn ready_line(s: &Session, pid: u32) -> String {
    format!(
        "atlas-agent ready name={} pid={pid} ATLAS_DEBUG_SOCK={}",
        s.name,
        s.socket().display()
    )
}

/// The debug app executable of this checkout: the bundle (what `desktop:build:app` makes), else the bare binary.
pub fn find_app(repo: &Path) -> Result<PathBuf> {
    let candidates = [
        repo.join("target/debug/bundle/macos/Atlas.app/Contents/MacOS/atlas-desktop"),
        repo.join("target/debug/atlas-desktop"),
    ];
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .with_context(|| {
            format!(
                "no debug build of Atlas Desktop under {}/target/debug; build it: `cargo build -p atlas-cli -p atlas-desktop` (the frontend is served by `bun run desktop:agent` through Vite) or `bun run desktop:build:app`",
                repo.display()
            )
        })
}

/// Whether a process executable is the app this record started (guards `--stop` against a recycled pid).
pub fn exe_matches(running: &Path, recorded: &Path) -> bool {
    running == recorded
}

/// `<repo>/target/{debug,release}/atlas` -> `<repo>`, when that is a Cargo workspace.
pub fn repo_of_exe(exe: &Path) -> Option<PathBuf> {
    let repo = exe.parent()?.parent()?.parent()?;
    let is_target = exe.parent()?.parent()?.file_name()? == "target";
    (is_target
        && std::fs::read_to_string(repo.join("Cargo.toml"))
            .is_ok_and(|t| t.contains("[workspace]")))
    .then(|| repo.to_path_buf())
}

fn temp_root() -> Result<PathBuf> {
    // /tmp -> /private/tmp: canonical and short.
    Ok(std::fs::canonicalize("/tmp")?)
}

fn running_exe(pid: u32) -> Option<PathBuf> {
    let out = Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// Longest a start waits for another instance that is starting (it holds the lock for its display setup).
const START_PATIENCE: Duration = Duration::from_secs(120);
/// After the bridge answers, the instance keeps the lock this long: its window placement and guard have run.
const START_SETTLE: Duration = Duration::from_secs(2);

/// Only one agent instance may be STARTING at a time: a start creates and configures a virtual display, which
/// changes the machine's display layout, and two starts doing it together left a window on the user's screen
/// (concurrent starts also got empty mode lists). Exclusive `flock` on a file in the temp root, released on drop
/// and by the OS if the process dies.
pub struct StartLock {
    /// Held for its `Drop`: closing the descriptor releases the lock.
    _file: std::fs::File,
}

impl StartLock {
    /// Wait up to `patience` for the lock, calling `waiting` (with the lock path) about once a second while it is
    /// held by another start; an error names the lock so a stuck one can be diagnosed.
    pub fn acquire(
        path: &Path,
        patience: Duration,
        mut waiting: impl FnMut(&Path),
    ) -> Result<Self> {
        use std::os::fd::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        let t0 = Instant::now();
        let mut told = None::<Instant>;
        loop {
            // SAFETY: a valid open fd; LOCK_NB makes it return at once instead of blocking.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Self { _file: file });
            }
            if told.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                waiting(path);
                told = Some(Instant::now());
            }
            if t0.elapsed() >= patience {
                bail!(
                    "another agent instance is starting (it holds {}); waited {}s. If none is starting, delete the file",
                    path.display(),
                    patience.as_secs()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// How long a start waits for the lock: `ATLAS_AGENT_START_WAIT` seconds (at most an hour) or [`START_PATIENCE`].
fn start_patience(var: Option<&str>) -> Duration {
    var.and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s <= 3600)
        .map_or(START_PATIENCE, Duration::from_secs)
}

pub struct StartOpts {
    pub name: Option<String>,
    pub scratch: bool,
    pub repo: PathBuf,
    pub atlas_bin: PathBuf,
    pub extra_env: Vec<(String, String)>,
}

/// Start an instance and wait until its bridge answers. Prints the ready line.
pub fn start(o: StartOpts) -> Result<()> {
    let root = temp_root()?;
    // One start at a time (see `StartLock`): held until this instance's window has been placed.
    let _turn = StartLock::acquire(
        &root.join("atlas-agent.lock"),
        start_patience(std::env::var("ATLAS_AGENT_START_WAIT").ok().as_deref()),
        |p| {
            eprintln!(
                "atlas-agent: another instance is starting (lock {}); waiting for its turn",
                p.display()
            );
        },
    )?;
    let name = o.name.unwrap_or_else(|| next_name(&root));
    let s = session_in(&root, &name)?;
    if s.dir.exists() {
        bail!(
            "agent instance {name:?} already exists ({}); `atlas desktop --agent --stop {name}` first, or pick another --name",
            s.dir.display()
        );
    }
    let app = find_app(&o.repo)?;
    std::fs::create_dir_all(s.home().join(".atlas/desktop"))?;
    std::fs::create_dir_all(s.hostd())?;
    if o.scratch {
        let user_home = dirs::home_dir().context("no home directory")?;
        seed_scratch(&user_home, &s)?;
    }
    let log = std::fs::File::create(s.log())?;
    let mut cmd = Command::new(&app);
    cmd.env_clear()
        .envs(app_env(&s, &o.atlas_bin))
        .envs(o.extra_env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    for (k, v) in forwarded_env(std::env::vars_os()) {
        cmd.env(k, v);
    }
    let child = cmd
        .spawn()
        .with_context(|| format!("starting {}", app.display()))?;
    let pid = child.id();
    let rec = Record {
        name: name.clone(),
        pid,
        socket: s.socket(),
        hostd: s.hostd(),
        exe: app,
    };
    std::fs::write(s.record(), serde_json::to_string_pretty(&rec)?)?;
    let dry = o
        .extra_env
        .iter()
        .any(|(k, v)| k == "ATLAS_AGENT_DRYRUN" && v == "1")
        || std::env::var("ATLAS_AGENT_DRYRUN").is_ok_and(|v| v == "1");
    let t0 = Instant::now();
    let mut child = child;
    loop {
        if let Ok(info) = bridge_info(&s.socket()) {
            match readiness(&info, pid) {
                Readiness::Ready => break,
                Readiness::NotAgent => {
                    let _ = stop_one(&s);
                    bail!(
                        "the app started but is not in agent mode (is this a release build?): refusing to leave it running"
                    );
                }
                Readiness::Waiting => {}
            }
        }
        // The app ends itself on a refusal (exit 3: its window would not have been provably on its own display) and
        // after a dry run (exit 0, nothing shown). Report either from its log, never as a mystery.
        if let Some(status) = child.try_wait()? {
            let log = std::fs::read_to_string(s.log()).unwrap_or_default();
            let line = |tag: &str| log.lines().find(|l| l.contains(tag)).map(str::to_owned);
            let _ = std::fs::remove_dir_all(&s.dir);
            if let Some(l) = line("REFUSED to show the window") {
                bail!("the app refused to show its window and exited ({status}): {l}");
            }
            if dry && status.success() {
                let l = line("DRYRUN decision")
                    .context("the dry run exited without logging its decision")?;
                println!("{l}");
                return Ok(());
            }
            bail!(
                "the app exited ({status}) before it was ready; see {}",
                s.log().display()
            );
        }
        if t0.elapsed() > READY_TIMEOUT {
            let _ = stop_one(&s);
            bail!(
                "the window was not shown after {}s; see {}",
                READY_TIMEOUT.as_secs(),
                s.log().display()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // The window is shown (the lock is held until now), but give the display a moment to settle before the next
    // instance reconfigures the layout.
    std::thread::sleep(START_SETTLE);
    println!("{}", ready_line(&s, pid));
    Ok(())
}

/// What `atlas desktop --agent` makes of one answer of the instance's bridge.
#[derive(Debug, PartialEq, Eq)]
enum Readiness {
    Waiting,
    Ready,
    NotAgent,
}

/// Ready = the bridge of THIS pid answers, runs in agent mode and its window is shown (the app shows it only after
/// the placement decision passed, so a visible window is one that is on its own virtual display).
fn readiness(info: &serde_json::Value, pid: u32) -> Readiness {
    if info["pid"].as_u64() != Some(u64::from(pid)) {
        return Readiness::Waiting;
    }
    if info["agent"] != true {
        return Readiness::NotAgent;
    }
    if info["visible"] == true {
        Readiness::Ready
    } else {
        Readiness::Waiting
    }
}

fn bridge_info(sock: &Path) -> Result<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    let stream = std::os::unix::net::UnixStream::connect(sock)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    (&stream).write_all(b"{\"cmd\":\"info\"}\n")?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let v: serde_json::Value = serde_json::from_str(&line)?;
    if v["ok"] != true {
        bail!("{}", v["error"]);
    }
    Ok(v["result"].clone())
}

/// Stop one instance: its recorded pid (only if that pid still runs the recorded executable), its hostd, its files.
fn stop_one(s: &Session) -> Result<()> {
    if let Ok(raw) = std::fs::read_to_string(s.record()) {
        let rec: Record = serde_json::from_str(&raw).context("agent.json is corrupt")?;
        if running_exe(rec.pid).is_some_and(|e| exe_matches(&e, &rec.exe)) {
            // SAFETY: plain signal to the pid we recorded and just verified.
            unsafe { libc::kill(rec.pid as libc::pid_t, libc::SIGTERM) };
            let t0 = Instant::now();
            while running_exe(rec.pid).is_some() && t0.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(100));
            }
            if running_exe(rec.pid).is_some_and(|e| exe_matches(&e, &rec.exe)) {
                // SAFETY: same verified pid.
                unsafe { libc::kill(rec.pid as libc::pid_t, libc::SIGKILL) };
            }
        }
        // Its own hostd (the dir is private to this instance), through the CLI so the daemon stops cleanly.
        let atlas = std::env::current_exe()?;
        let _ = Command::new(atlas)
            .args(["hostd", "--stop", "--dir"])
            .arg(&rec.hostd)
            .env("ATLAS_HOSTD_DIR", &rec.hostd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // Its WebKit store: WKWebView keeps it in the real ~/Library, not in the instance's HOME.
    if let Some(home) = dirs::home_dir() {
        for p in atlas_core::agent_store::paths(&home, &s.socket().to_string_lossy()) {
            let _ = std::fs::remove_dir_all(p);
        }
    }
    std::fs::remove_dir_all(&s.dir).or_else(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    Ok(())
}

/// `--gc`: delete the WebKit stores of agent instances that no longer exist (left by crashes, or by
/// instances stopped before `--stop` cleaned them up). Live instances and the user's own store are kept.
pub fn gc() -> Result<()> {
    let root = temp_root()?;
    let live: Vec<String> = existing(&root)
        .iter()
        .filter_map(|n| session_in(&root, n).ok())
        .map(|s| s.socket().to_string_lossy().into_owned())
        .collect();
    let home = dirs::home_dir().context("no home directory")?;
    let gone = atlas_core::agent_store::orphans(&home, &live);
    for p in &gone {
        std::fs::remove_dir_all(p).with_context(|| format!("remove {}", p.display()))?;
        println!("removed {}", p.display());
    }
    println!(
        "{} leftover agent store(s) removed; {} live instance(s) kept",
        gone.len(),
        live.len()
    );
    Ok(())
}

/// Names of the instances that exist.
fn existing(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_prefix("atlas-agent-")
                .map(String::from)
        })
        .filter(|n| valid_name(n).is_ok())
        .collect();
    v.sort();
    v
}

/// `--stop [name]`: that instance, or all of them.
pub fn stop(name: Option<&str>) -> Result<()> {
    let root = temp_root()?;
    let names = match name {
        Some(n) => vec![n.to_string()],
        None => existing(&root),
    };
    if names.is_empty() {
        println!("no agent instances");
    }
    for n in names {
        let s = session_in(&root, &n)?;
        if !s.dir.exists() {
            bail!("no agent instance {n:?}");
        }
        stop_one(&s)?;
        println!("stopped {n}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Incident (first 1x run): `ATLAS_AGENT_SCALE=1 atlas desktop --agent` still made a retina display, because the
    /// environment is cleared and the variable was not in the forwarded set.
    #[test]
    fn the_display_scale_reaches_the_instance_and_the_users_secrets_and_paths_do_not() {
        let os = |s: &str| std::ffi::OsString::from(s);
        let got = forwarded_env([
            (os("ATLAS_AGENT_SCALE"), os("1")),
            (os("PATH"), os("/usr/bin")),
            (os("HOME"), os("/Users/me")),
            (os("ATLAS_HOSTD_DIR"), os("/Users/me/.atlas")),
            (
                os("ATLAS_DEBUG_SOCK"),
                os("/Users/me/.atlas/desktop-debug.sock"),
            ),
            (os("GITHUB_TOKEN"), os("secret")),
            (os("ELEVENLABS_API_KEY"), os("secret")),
        ]);
        let keys: Vec<_> = got.iter().map(|(k, _)| k.to_str().unwrap()).collect();
        assert_eq!(keys, ["ATLAS_AGENT_SCALE", "PATH"]);
    }

    /// The dry-run switch must reach the app too, or "prove it before a real start" proves nothing.
    #[test]
    fn the_dry_run_switch_reaches_the_instance() {
        let os = |s: &str| std::ffi::OsString::from(s);
        let got = forwarded_env([(os("ATLAS_AGENT_DRYRUN"), os("1"))]);
        assert_eq!(got, [(os("ATLAS_AGENT_DRYRUN"), os("1"))]);
    }

    /// Incident (window left on the user's display): the start lock was released as soon as the bridge answered, but
    /// the window is placed and shown later, at the page's first frame, so the next start could reconfigure the
    /// displays in between. An instance is ready, and the lock released, only once its window is shown.
    #[test]
    fn an_instance_is_ready_only_when_its_window_is_shown_in_agent_mode() {
        let info = |pid: u64, agent: bool, visible: bool| serde_json::json!({"pid": pid, "agent": agent, "visible": visible});
        assert_eq!(readiness(&info(5, true, false), 5), Readiness::Waiting);
        assert_eq!(readiness(&info(5, true, true), 5), Readiness::Ready);
        assert_eq!(readiness(&info(5, false, true), 5), Readiness::NotAgent);
        assert_eq!(readiness(&info(6, true, true), 5), Readiness::Waiting);
        assert_eq!(readiness(&serde_json::json!({}), 5), Readiness::Waiting);
    }

    #[test]
    fn every_file_of_an_instance_is_under_its_own_directory() {
        let root = Path::new("/private/tmp");
        let s = session_in(root, "a1").unwrap();
        for p in [s.home(), s.hostd(), s.socket(), s.log(), s.record()] {
            assert!(p.starts_with("/private/tmp/atlas-agent-a1"), "{p:?}");
        }
        let t = session_in(root, "a2").unwrap();
        assert_ne!(
            s.socket(),
            t.socket(),
            "concurrent agents never share a bridge"
        );
        assert_ne!(s.hostd(), t.hostd(), "nor a hostd");
    }

    /// `ATLAS_AGENT_START_WAIT` = how many seconds a start waits for another one that is starting; anything that is
    /// not a whole number of seconds is the default, never an unbounded wait.
    #[test]
    fn the_start_wait_is_a_bounded_number_of_seconds_from_the_environment() {
        assert_eq!(start_patience(None), START_PATIENCE);
        assert_eq!(start_patience(Some("5")), Duration::from_secs(5));
        assert_eq!(start_patience(Some("0")), Duration::from_secs(0));
        for bad in ["", "-1", "abc", "1.5", "99999999"] {
            assert_eq!(start_patience(Some(bad)), START_PATIENCE, "{bad:?}");
        }
    }

    /// Incident (three agent instances started together): one window showed on the user's built-in display.
    /// Starts configure the machine's displays, so only one instance may be starting at a time.
    #[test]
    fn a_second_start_waits_for_the_first_to_finish_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atlas-agent.lock");
        let first = StartLock::acquire(&path, Duration::from_secs(1), |_| {}).unwrap();
        // While the first holds it, a second one cannot get it within its patience, and the error says why.
        let waited = std::cell::Cell::new(0);
        let err = StartLock::acquire(&path, Duration::from_millis(300), |_| {
            waited.set(waited.get() + 1)
        })
        .err()
        .expect("the lock is held")
        .to_string();
        assert!(err.contains("another agent instance is starting"), "{err}");
        assert!(
            err.contains("atlas-agent.lock"),
            "names the lock file: {err}"
        );
        assert!(waited.get() >= 1, "it told the caller it was waiting");
        // Once the first lets go, the next start proceeds.
        drop(first);
        assert!(StartLock::acquire(&path, Duration::from_secs(1), |_| {}).is_ok());
    }

    #[test]
    fn a_waiting_start_proceeds_the_moment_the_holder_releases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atlas-agent.lock");
        let first = StartLock::acquire(&path, Duration::from_secs(1), |_| {}).unwrap();
        let p2 = path.clone();
        let t = std::thread::spawn(move || {
            StartLock::acquire(&p2, Duration::from_secs(10), |_| {}).map(|_| ())
        });
        std::thread::sleep(Duration::from_millis(250));
        drop(first);
        assert!(
            t.join().unwrap().is_ok(),
            "the waiter got the lock after the release"
        );
    }

    #[test]
    fn the_isolating_environment_points_at_the_instance_and_turns_agent_mode_on() {
        let s = session_in(Path::new("/private/tmp"), "a1").unwrap();
        let env = app_env(&s, Path::new("/x/atlas"));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HOME"), Some("/private/tmp/atlas-agent-a1/home"));
        assert_eq!(
            get("ATLAS_HOSTD_DIR"),
            Some("/private/tmp/atlas-agent-a1/hostd")
        );
        assert_eq!(
            get("ATLAS_DEBUG_SOCK"),
            Some("/private/tmp/atlas-agent-a1/debug.sock")
        );
        assert_eq!(get("ATLAS_AGENT"), Some("1"));
    }

    /// The agent display exists only if the socket (and so the bridge) can bind: macOS refuses paths over 103 bytes.
    #[test]
    fn a_socket_path_macos_cannot_bind_is_refused_up_front() {
        let long = "x".repeat(24);
        let deep = PathBuf::from(format!("/private/tmp/{}", "d".repeat(80)));
        assert!(session_in(&deep, &long).is_err());
        assert!(session_in(Path::new("/private/tmp"), &long).is_ok());
    }

    #[test]
    fn names_are_short_safe_directory_components() {
        for bad in ["", "../x", "a/b", "a b", "ü", &"x".repeat(25), ".."] {
            assert!(valid_name(bad).is_err(), "{bad:?}");
        }
        for ok in ["a1", "capture-1", "My_agent"] {
            assert!(valid_name(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn the_next_name_skips_instances_that_exist() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(next_name(d.path()), "a1");
        std::fs::create_dir(d.path().join("atlas-agent-a1")).unwrap();
        std::fs::create_dir(d.path().join("atlas-agent-a2")).unwrap();
        assert_eq!(next_name(d.path()), "a3");
        assert_eq!(existing(d.path()), ["a1", "a2"]);
    }

    /// Verifies the invariant the user cares about: `--scratch` never leaks his projects or secrets into an instance.
    #[test]
    fn scratch_copies_only_the_desktop_config_files_never_projects_tokens_or_sockets() {
        let user = tempfile::tempdir().unwrap();
        let atlas = user.path().join(".atlas");
        std::fs::create_dir_all(atlas.join("desktop/nested")).unwrap();
        std::fs::write(atlas.join("desktop/settings.json"), "{\"theme\":\"dark\"}").unwrap();
        std::fs::write(atlas.join("desktop/projects.json"), "[\"/secret/project\"]").unwrap();
        std::fs::write(atlas.join("desktop/nested/x.json"), "{}").unwrap();
        std::fs::write(atlas.join("hostd.token"), "TOKEN").unwrap();
        std::fs::write(atlas.join("projects.json"), "[\"/secret\"]").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", atlas.join("desktop/leak")).unwrap();

        let root = tempfile::tempdir().unwrap();
        let s = session_in(root.path(), "a1").unwrap();
        assert_eq!(seed_scratch(user.path(), &s).unwrap(), 1);
        let copied: Vec<_> = walk(&s.home());
        assert_eq!(copied, [".atlas/desktop/settings.json"], "{copied:?}");
    }

    #[test]
    fn scratch_without_a_user_config_is_an_empty_home_not_an_error() {
        let user = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let s = session_in(root.path(), "a1").unwrap();
        assert_eq!(seed_scratch(user.path(), &s).unwrap(), 0);
        assert!(s.home().join(".atlas/desktop").is_dir());
    }

    fn walk(dir: &Path) -> Vec<String> {
        let mut out = vec![];
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(dir).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn the_ready_line_carries_the_socket_an_agent_exports() {
        let s = session_in(Path::new("/private/tmp"), "a1").unwrap();
        assert_eq!(
            ready_line(&s, 42),
            "atlas-agent ready name=a1 pid=42 ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-a1/debug.sock"
        );
    }

    #[test]
    fn stop_only_signals_a_pid_that_still_runs_the_recorded_executable() {
        // a recycled pid belongs to some other program: never the one we started
        assert!(exe_matches(
            Path::new("/a/atlas-desktop"),
            Path::new("/a/atlas-desktop")
        ));
        assert!(!exe_matches(
            Path::new("/usr/bin/vim"),
            Path::new("/a/atlas-desktop")
        ));
    }

    /// Incident (first agent run): `--agent` failed with "No Cargo workspace found" from any directory but the repo,
    /// although the CLI itself is `<repo>/target/debug/atlas` and so knows where the debug app is.
    #[test]
    fn the_repo_is_found_from_the_cli_binary_when_cwd_is_elsewhere() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("target/debug")).unwrap();
        std::fs::write(d.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let exe = d.path().join("target/debug/atlas");
        assert_eq!(repo_of_exe(&exe).unwrap(), d.path());
        // a release-installed CLI is not inside a checkout
        assert!(repo_of_exe(Path::new("/Users/x/.atlas/bin/atlas")).is_none());
        // a `target/debug/atlas` whose parent is not a workspace is not a checkout either
        let e = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(e.path().join("target/debug")).unwrap();
        assert!(repo_of_exe(&e.path().join("target/debug/atlas")).is_none());
    }

    #[test]
    fn a_missing_debug_build_says_how_to_build_it() {
        let d = tempfile::tempdir().unwrap();
        let e = find_app(d.path()).unwrap_err().to_string();
        assert!(
            e.contains("cargo build") && e.contains("desktop:build"),
            "{e}"
        );
        std::fs::create_dir_all(d.path().join("target/debug")).unwrap();
        std::fs::write(d.path().join("target/debug/atlas-desktop"), "").unwrap();
        assert!(
            find_app(d.path())
                .unwrap()
                .ends_with("target/debug/atlas-desktop")
        );
    }
}
