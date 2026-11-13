//! Session hygiene: every process Atlas starts for a terminal, an agent, a service or a browser
//! belongs to a session, and Atlas keeps its own registry of the roots it started, with their
//! start times (a pid alone is reused). When the session's owner is gone, its tree is orphaned
//! and is reaped: SIGTERM, a short grace, then SIGKILL, on exact pids only, re-checking
//! `(pid, start)` before every signal.
//!
//! How the tree is found depends on what the OS lets us see:
//!
//! - **Linux** reads `ATLAS_SESSION=<id>` from every process's environment (inherited by every
//!   descendant, even one that `setsid`s or double-forks away from the parent chain).
//! - **macOS** (SIP) and **Windows** do not let a process read another's environment, so the tag
//!   cannot be read back. Membership is **tracked while the owner lives**: [`Registry::track`]
//!   snapshots each live root's descendants by parent links and records every member as
//!   `(pid, start)`, at spawn and every few seconds. A grandchild that daemonized and was
//!   re-parented to launchd is still known because it was seen under its root first. **The
//!   window:** a process that forks and daemonizes between two snapshots escapes (there is no
//!   job object on macOS); `atlas doctor --hygiene` says so.
//!
//! Directories Atlas creates for a session (agent temp dirs, browser profiles) are registered
//! with a size cap; [`enforce_caps`] drops dirs whose owner is gone and trims the oldest entries
//! of the rest, so repeated runs never grow the disk past the cap.
//!
//! This is the one core behind `atlas doctor --hygiene`, the periodic sweeps of hostd and
//! `atlas dev`, the MCP tool and the desktop panel. Nothing here ever touches a process that was
//! not recorded under a session (or tagged on Linux) or a file outside a registered directory.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::procs::{self, Proc};

/// The environment variable that names a process's owner session.
pub const SESSION_ENV: &str = "ATLAS_SESSION";

/// SIGTERM-to-SIGKILL grace of a reap.
pub const GRACE: Duration = Duration::from_secs(3);

/// Default cap of a registered directory.
pub const DEFAULT_DIR_CAP: u64 = 512 * 1024 * 1024;

/// The process whose death ends a session: `(pid, start)` names it for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub pid: u32,
    pub start: u64,
}

impl Owner {
    /// This process.
    pub fn current() -> Self {
        let pid = std::process::id();
        Self {
            pid,
            start: procs::start_time(pid).unwrap_or(0),
        }
    }

    /// The owner still runs: its pid exists AND started when it did (a reused pid is a stranger)
    /// AND it has not exited (a Unix zombie, or a Windows process kept in the table by a handle).
    pub fn alive(&self) -> bool {
        match procs::start_time(self.pid) {
            Some(start) => start == self.start && !zombie(self.pid),
            // Unreadable start (Windows access denied): fall back to the bare pid.
            None => self.start == 0 && crate::sys::pid_alive(self.pid),
        }
    }
}

/// What a registered root is, for the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Terminal,
    Service,
    Agent,
    Browser,
    Other,
}

/// One process Atlas started for a session: the root of a tagged tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub session: String,
    /// Whose death orphans the tree.
    pub owner: Owner,
    pub pid: u32,
    pub start: u64,
    pub kind: Kind,
    pub label: String,
    pub project: Option<String>,
    pub started_at: u64,
}

impl Root {
    /// The root process still runs (same pid, same start).
    pub fn alive(&self) -> bool {
        Owner {
            pid: self.pid,
            start: self.start,
        }
        .alive()
    }
}

impl Member {
    /// The process still runs and is the one that was recorded (same pid, same start).
    pub fn alive(&self) -> bool {
        Owner {
            pid: self.pid,
            start: self.start,
        }
        .alive()
    }
}

/// A directory Atlas created for a session, with the size it may grow to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dir {
    pub path: PathBuf,
    pub cap_bytes: u64,
    pub kind: DirKind,
    /// The session that owns it; the dir goes with the session. `None`: a standing cache that is
    /// only trimmed to its cap.
    pub session: Option<String>,
    pub owner: Option<Owner>,
    pub project: Option<String>,
    pub created_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirKind {
    /// Scratch an agent session writes (temp files, traces).
    Temp,
    /// A browser profile Atlas created.
    BrowserProfile,
    /// A cache that outlives sessions and is only trimmed.
    Cache,
    /// The whole directory of a running app instance (its HOME, daemon dir, log): structured, so
    /// it is never trimmed entry by entry while its owner lives (that would delete files the
    /// app is using); it goes whole once the owner is gone, and its cap is only reported.
    Instance,
}

/// A process seen below a session's root: `(pid, start)` names it for good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub session: String,
    pub pid: u32,
    pub start: u64,
    pub name: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    roots: Vec<Root>,
    #[serde(default)]
    members: Vec<Member>,
    #[serde(default)]
    dirs: Vec<Dir>,
}

/// Where the registry lives: `<base>/hygiene/registry.json`, plus a lock file beside it.
#[derive(Clone, Debug)]
pub struct Registry {
    dir: PathBuf,
    /// Create the base folder when missing. False for a daemon's own dir: when the dir is
    /// removed under it, a write must not bring it back (see [`Registry::in_existing`]).
    create_base: bool,
}

impl Registry {
    /// The user's registry under `~/.atlas`. `ATLAS_HYGIENE_DIR` (absolute) moves it, and so does
    /// `ATLAS_HOSTD_DIR` (an agent instance's daemon dir): an instance and a test never write to
    /// the real one.
    pub fn open() -> Self {
        let abs = |k: &str| {
            std::env::var_os(k)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let base = abs("ATLAS_HYGIENE_DIR")
            .or_else(|| abs("ATLAS_HOSTD_DIR"))
            .unwrap_or_else(crate::paths::atlas_home);
        Self::at(&base)
    }

    /// The registry under `base`, created (with `base`) on the first write.
    pub fn at(base: &Path) -> Self {
        Self {
            dir: base.join("hygiene"),
            create_base: true,
        }
    }

    /// The registry under a daemon's own dir: writes create only the `hygiene` folder, never a
    /// missing `base`. hostd exits when its dir is removed; a terminal spawning at that moment
    /// used to recreate the dir and race the deletion ("Directory not empty").
    pub fn in_existing(base: &Path) -> Self {
        Self {
            dir: base.join("hygiene"),
            create_base: false,
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("registry.json")
    }

    /// Read-modify-write under an exclusive lock (several daemons and CLIs share the file).
    fn update<T>(&self, f: impl FnOnce(&mut Store) -> T) -> std::io::Result<T> {
        self.update_in(true, f)
    }

    /// [`update`](Self::update) for maintenance (track, prune, reap, remove, caps): it never
    /// creates anything. A registry whose folder is gone has nothing to maintain, and a daemon
    /// whose dir was removed (hostd exits when it is) must not resurrect it on its next tick; a
    /// `remove_dir_all` racing such a write fails with "Directory not empty".
    fn maintain<T>(&self, f: impl FnOnce(&mut Store) -> T) -> std::io::Result<Option<T>> {
        if !self.dir.is_dir() {
            return Ok(None);
        }
        match self.update_in(false, f) {
            Ok(v) => Ok(Some(v)),
            // Removed between the check and the open: the same "nothing to maintain".
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn update_in<T>(&self, create: bool, f: impl FnOnce(&mut Store) -> T) -> std::io::Result<T> {
        if create && self.create_base {
            std::fs::create_dir_all(&self.dir)?;
        } else if create {
            // Only the registry's own folder: a missing base is NotFound to the caller.
            match std::fs::create_dir(&self.dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        let lock_path = self.dir.join("registry.lock");
        let lock = std::fs::OpenOptions::new()
            .create(create)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        // Short critical section: spin briefly instead of blocking a runtime thread.
        let mut tries = 0;
        while !crate::sys::try_lock(&lock)? {
            tries += 1;
            if tries > 200 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "hygiene registry is locked",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut store = self.load();
        let before = serde_json::to_vec_pretty(&store).unwrap_or_default();
        let out = f(&mut store);
        // `track` runs every couple of seconds per owner: an unchanged registry is not rewritten
        // (and fsynced) each time.
        if serde_json::to_vec_pretty(&store).unwrap_or_default() != before {
            self.save(&store)?;
        }
        Ok(out)
    }

    fn load(&self) -> Store {
        std::fs::read(self.file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, store: &Store) -> std::io::Result<()> {
        let tmp = self
            .dir
            .join(format!("registry.json.{}", std::process::id()));
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(store).unwrap_or_default())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, self.file())
    }

    /// Record a started root. A repeat of the same `(pid, start)` replaces the old record.
    pub fn add_root(&self, root: Root) -> std::io::Result<()> {
        self.update(|s| {
            s.roots
                .retain(|r| !(r.pid == root.pid && r.start == root.start));
            s.roots.push(root);
        })
    }

    /// Forget a session's roots (it ended cleanly and its tree was reaped).
    pub fn remove_session(&self, session: &str) -> std::io::Result<()> {
        self.maintain(|s| {
            s.roots.retain(|r| r.session != session);
            s.members.retain(|m| m.session != session);
            s.dirs.retain(|d| d.session.as_deref() != Some(session));
        })
        .map(drop)
    }

    /// A new process takes over `sessions` (hostd's live upgrade hands the terminals to its
    /// successor): their roots and dirs now die with `owner`, not with the daemon that retires.
    /// Sessions the registry does not know are left alone; `add_root` them first.
    pub fn set_owner(&self, sessions: &[String], owner: Owner) -> std::io::Result<()> {
        self.maintain(|s| {
            for r in s.roots.iter_mut().filter(|r| sessions.contains(&r.session)) {
                r.owner = owner;
            }
            for d in s
                .dirs
                .iter_mut()
                .filter(|d| d.session.as_ref().is_some_and(|x| sessions.contains(x)))
            {
                d.owner = Some(owner);
            }
        })
        .map(drop)
    }

    pub fn roots(&self) -> Vec<Root> {
        self.load().roots
    }

    pub fn dirs(&self) -> Vec<Dir> {
        self.load().dirs
    }

    pub fn members(&self) -> Vec<Member> {
        self.load().members
    }

    /// A recorded process still runs under a session whose owner is gone: an orphaned tree waits
    /// to be reaped. One file read and a start-time probe per record, so a daemon can poll it
    /// every few seconds and run the full [`sweep`] (a scan of the process table) only when it
    /// says so.
    pub fn has_orphaned_root(&self) -> bool {
        let store = self.load();
        let live = live_sessions(&store.roots);
        store.roots.iter().any(|r| !r.owner.alive() && r.alive())
            || store
                .members
                .iter()
                .any(|m| !live.contains(&m.session) && m.alive())
    }

    /// Record the current descendants of every live root as members of its session, and forget
    /// members that exited. Call it when a root is spawned and every few seconds while the owner
    /// runs: this is how a process that later daemonizes (re-parented to launchd, out of every
    /// parent chain) is still known to be a session's. One process-table read; cheap.
    pub fn track(&self) -> std::io::Result<()> {
        // Nothing registered (an idle daemon, a lab): no process-table read, and no registry or
        // lock file created just to find that out.
        let store = self.load();
        if store.roots.is_empty() && store.members.is_empty() {
            return Ok(());
        }
        let table = procs::table();
        let names: HashMap<u32, &Proc> = table.iter().map(|p| (p.pid, p)).collect();
        self.maintain(|s| {
            s.members.retain(Member::alive);
            // A child seen between fork and exec carries its parent's name; it is the same process
            // (same pid and start) once it has exec'd, so the name follows.
            for m in s.members.iter_mut() {
                if let Some(p) = names.get(&m.pid)
                    && p.start == m.start
                {
                    m.name.clone_from(&p.name);
                }
            }
            let known: HashSet<(u32, u64)> = s.members.iter().map(|m| (m.pid, m.start)).collect();
            let mut add = Vec::new();
            for r in s.roots.iter().filter(|r| r.alive()) {
                for pid in procs::tree(r.pid, &table) {
                    let Some(p) = names.get(&pid) else { continue };
                    if !known.contains(&(p.pid, p.start)) {
                        add.push(Member {
                            session: r.session.clone(),
                            pid: p.pid,
                            start: p.start,
                            name: p.name.clone(),
                        });
                    }
                }
            }
            s.members.extend(add);
        })
        .map(drop)
    }

    /// Register `dir` with a cap. A repeat of the same path replaces the old record, so a
    /// session that registers on every start never grows the list.
    pub fn add_dir(&self, dir: Dir) -> std::io::Result<()> {
        // `enforce_caps` deletes a registered dir whole when its owner is gone, so only a path that
        // is plainly a scratch directory may enter: absolute, below the root, not the user's home.
        if !is_deletable_dir(&dir.path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to register {}: not a scratch directory",
                    dir.path.display()
                ),
            ));
        }
        // Heavy, rewritten-constantly dirs must never feed Spotlight (a freeze on a Mac indexing
        // several `target/` trees at once): the marker goes in before anything is written.
        mark_never_index(&dir.path);
        self.update(|s| {
            s.dirs.retain(|d| d.path != dir.path);
            s.dirs.push(dir);
        })
    }

    /// Drop records whose process or directory no longer exists.
    pub fn prune(&self) -> std::io::Result<()> {
        self.maintain(|s| {
            s.roots.retain(Root::alive);
            s.members.retain(Member::alive);
            s.dirs.retain(|d| d.path.exists());
        })
        .map(drop)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// A path that may be registered and later deleted whole: absolute, at least two components deep
/// (never `/` or `/tmp`-level), and neither the user's home nor one of its ancestors.
fn is_deletable_dir(p: &Path) -> bool {
    if !p.is_absolute() || p.components().count() < 3 {
        return false;
    }
    match dirs::home_dir() {
        Some(h) => !h.starts_with(p),
        None => true,
    }
}

/// The file whose presence in a directory makes Spotlight skip it and everything below.
pub const NEVER_INDEX: &str = ".metadata_never_index";

/// Keep Spotlight out of `dir` (an Atlas-created worktree root, build dir or cache): write
/// [`NEVER_INDEX`] there. macOS only (elsewhere nothing indexes it and nothing is written), and
/// only an existing directory without the marker gets one. Returns whether it wrote the file.
/// Never deletes or changes anything else.
pub fn mark_never_index(dir: &Path) -> bool {
    if !cfg!(target_os = "macos") || !dir.is_dir() {
        return false;
    }
    let marker = dir.join(NEVER_INDEX);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .is_ok()
}

/// Spotlight indexes `dir`: no [`NEVER_INDEX`] in it or in any ancestor (the marker at a
/// worktree root covers every checkout below it). Always `false` off macOS.
pub fn spotlight_indexes(dir: &Path) -> bool {
    cfg!(target_os = "macos") && !dir.ancestors().any(|a| a.join(NEVER_INDEX).exists())
}

/// A new session id: 16 hex characters from the OS CSPRNG (never reused, never guessable).
pub fn new_session_id() -> String {
    let mut b = [0u8; 8];
    if crate::sys::random_bytes(&mut b).is_err() {
        b = now_secs().to_be_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A process the scan found carrying the tag of a session that no longer lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Orphan {
    pub pid: u32,
    pub start: u64,
    pub name: String,
    pub session: String,
    pub kind: Option<Kind>,
    pub label: Option<String>,
    pub project: Option<String>,
}

/// What a scan saw.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Scan {
    pub orphans: Vec<Orphan>,
    /// Tagged processes whose owner is alive (left alone).
    pub live: usize,
}

/// Sessions the registry knows to be alive: an owner that still runs.
fn live_sessions(roots: &[Root]) -> HashSet<String> {
    roots
        .iter()
        .filter(|r| r.owner.alive())
        .map(|r| r.session.clone())
        .collect()
}

/// A tagged process whose session the registry does not know is only an orphan once it is older
/// than this: a daemon spawns the child first and registers it a few milliseconds later, and a
/// sweep in between must not end a terminal that is still being created.
pub const REGISTRATION_GRACE_SECS: u64 = 60;

/// Group the processes found for sessions into orphans and live ones at time `now`. Pure over
/// its inputs, so the rules are testable without processes:
///
/// - `tagged`: processes whose environment names a session (Linux; empty where it cannot be
///   read). A tag whose session has no live owner is an orphan; one whose session nobody
///   registered is an orphan only after [`REGISTRATION_GRACE_SECS`].
/// - `members`: recorded descendants (macOS, Windows, and Linux too). A member that still
///   matches its recorded `(pid, start)` under a session whose owner is gone is an orphan.
/// - a registered root whose owner died brings its current parent-chain tree in.
pub fn classify(
    tagged: &[(Proc, String)],
    roots: &[Root],
    members: &[Member],
    table: &[Proc],
    now: u64,
) -> Scan {
    let live = live_sessions(roots);
    let known: HashSet<&str> = roots.iter().map(|r| r.session.as_str()).collect();
    let by_pid: HashMap<u32, &Root> = roots.iter().map(|r| (r.pid, r)).collect();
    let by_session: HashMap<&str, &Root> = roots.iter().map(|r| (r.session.as_str(), r)).collect();
    let mut scan = Scan::default();
    let mut seen: HashSet<u32> = HashSet::new();
    let orphan = |p: &Proc, session: &str| {
        let r = by_pid.get(&p.pid).or_else(|| by_session.get(session));
        Orphan {
            pid: p.pid,
            start: p.start,
            name: p.name.clone(),
            session: session.to_string(),
            kind: r.map(|r| r.kind),
            label: r.map(|r| r.label.clone()),
            project: r.and_then(|r| r.project.clone()),
        }
    };
    for (p, session) in tagged {
        if !seen.insert(p.pid) {
            continue;
        }
        if live.contains(session) {
            scan.live += 1;
        } else if known.contains(session.as_str())
            || now.saturating_sub(p.start) >= REGISTRATION_GRACE_SECS
        {
            scan.orphans.push(orphan(p, session));
        } else {
            // Born a moment ago under a session nobody registered yet.
            scan.live += 1;
        }
    }
    // Recorded members: only a process that is still the one recorded (a reused pid is not).
    let rows: HashMap<u32, &Proc> = table.iter().map(|p| (p.pid, p)).collect();
    for m in members {
        let Some(p) = rows.get(&m.pid) else { continue };
        if p.start != m.start && m.start != 0 {
            continue;
        }
        if !seen.insert(m.pid) {
            continue;
        }
        if live.contains(&m.session) {
            scan.live += 1;
        } else {
            scan.orphans.push(orphan(p, &m.session));
        }
    }
    // Roots whose owner died: their whole parent-chain tree (processes not recorded yet).
    for r in roots.iter().filter(|r| !r.owner.alive() && r.alive()) {
        for pid in procs::tree(r.pid, table) {
            if let Some(p) = rows.get(&pid)
                && seen.insert(pid)
            {
                scan.orphans.push(orphan(p, &r.session));
            }
        }
    }
    scan.orphans.sort_by_key(|o| o.pid);
    scan
}

/// Scan the machine against `registry`.
pub fn scan(registry: &Registry) -> Scan {
    let table = procs::table();
    let tagged = procs::tagged_any(SESSION_ENV);
    let store = registry.load();
    classify(&tagged, &store.roots, &store.members, &table, now_secs())
}

/// End `orphans`: SIGTERM all, wait up to `grace`, SIGKILL what remains. Each pid is re-checked
/// against its recorded start right before every signal, so a pid reused meanwhile is never hit.
/// Returns the pids that were still alive after the SIGKILL (normally none).
pub fn reap(orphans: &[Orphan], grace: Duration) -> Vec<u32> {
    let still = |o: &Orphan| procs::start_time(o.pid).is_some_and(|s| s == o.start || o.start == 0);
    for o in orphans.iter().filter(|o| still(o)) {
        crate::sys::terminate(o.pid);
    }
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        if !orphans.iter().any(&still) {
            return Vec::new();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    for o in orphans.iter().filter(|o| still(o)) {
        crate::sys::force_kill(o.pid);
    }
    // SIGKILL is not instant and a killed child stays a zombie until its parent reaps it.
    let end = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < end && orphans.iter().any(|o| still(o) && !zombie(o.pid)) {
        std::thread::sleep(Duration::from_millis(20));
    }
    orphans
        .iter()
        .filter(|o| still(o) && !zombie(o.pid))
        .map(|o| o.pid)
        .collect()
}

/// A process that has exited and waits for its parent to collect it: no longer running.
fn zombie(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .map(|(_, r)| r.trim_start().starts_with('Z'))
            })
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: zeroed plain-old-data filled by proc_pidinfo.
        let mut i: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // SAFETY: `i` is a valid, correctly sized out buffer.
        let ret = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut i as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        // SZOMB = 5.
        ret == size && i.pbi_status == 5
    }
    // Windows has no zombies, but an exited process stays in the table (with its start time)
    // while anyone holds a handle to it, e.g. its parent's `Child`: it has exited when its exit
    // code is no longer STILL_ACTIVE. Without this a reaped process looked alive (CI: its records
    // were never dropped).
    #[cfg(windows)]
    {
        !crate::sys::pid_alive(pid)
    }
}

impl Registry {
    /// End every live process of `session`, whoever its owner is: what a session does when it
    /// closes (a terminal's close, a service stop, an action's end). It is found three ways: by
    /// its tag in the environment (Linux), as a recorded member that still matches its
    /// `(pid, start)`, and as the current parent-chain tree of a root still running (a root
    /// whose pid now belongs to a process that started at another time is a stranger and is
    /// skipped). Then the session's records are dropped. Returns the pids that survived SIGKILL.
    pub fn reap_session(&self, session: &str, grace: Duration) -> Vec<u32> {
        let table = procs::table();
        let store = self.load();
        let mut found: BTreeMap<u32, Proc> = procs::tagged(SESSION_ENV, session)
            .into_iter()
            .map(|p| (p.pid, p))
            .collect();
        let rows: HashMap<u32, &Proc> = table.iter().map(|p| (p.pid, p)).collect();
        let me = std::process::id();
        for m in store.members.iter().filter(|m| m.session == session) {
            if let Some(p) = rows.get(&m.pid)
                && (p.start == m.start || m.start == 0)
                && p.pid != me
            {
                found.entry(p.pid).or_insert_with(|| (*p).clone());
            }
        }
        for r in store.roots.iter().filter(|r| r.session == session) {
            if procs::start_time(r.pid).is_none_or(|s| s != r.start && r.start != 0) {
                continue;
            }
            for pid in procs::tree(r.pid, &table) {
                if let Some(p) = rows.get(&pid) {
                    found.entry(pid).or_insert_with(|| (*p).clone());
                }
            }
        }
        let orphans: Vec<Orphan> = found
            .into_values()
            .map(|p| Orphan {
                pid: p.pid,
                start: p.start,
                name: p.name,
                session: session.to_string(),
                kind: None,
                label: None,
                project: None,
            })
            .collect();
        let left = reap(&orphans, grace);
        let _ = self.remove_session(session);
        left
    }
}

/// Make `cmd` start inside `session`: the tag every descendant inherits.
pub fn tag_command(cmd: &mut std::process::Command, session: &str) {
    cmd.env(SESSION_ENV, session);
}

/// The size in bytes of everything under `path` (symlinks are counted as themselves, never
/// followed). Unreadable entries count as 0.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| dir_size(&e.path()))
        .sum()
}

/// What [`enforce_caps`] freed.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Freed {
    pub dirs_removed: usize,
    pub entries_removed: usize,
    pub bytes: u64,
}

/// Apply every registered directory's rules: a dir whose session owner is gone is deleted whole;
/// one over its cap loses its oldest entries (by modification time) until it fits (an
/// [`DirKind::Instance`] dir is never trimmed). A directory
/// that no longer exists is dropped from the registry. Never touches a path outside the registry.
/// With `dry_run` nothing is deleted and the result says what would be.
pub fn enforce_caps(registry: &Registry, dry_run: bool) -> Freed {
    let mut freed = Freed::default();
    let roots = registry.roots();
    let live = live_sessions(&roots);
    let mut keep: Vec<Dir> = Vec::new();
    for d in registry.dirs() {
        if !d.path.exists() {
            continue;
        }
        // A dir is registered before its session's first root is: a sweep of another process in
        // that gap must not take a brand-new dir for the leftover of a dead one.
        let settled = now_secs().saturating_sub(d.created_at) >= REGISTRATION_GRACE_SECS;
        let owner_gone = settled
            && match (&d.session, d.owner) {
                (Some(s), Some(o)) => !o.alive() && !live.contains(s),
                (Some(s), None) => !live.contains(s),
                (None, _) => false,
            };
        if owner_gone {
            freed.dirs_removed += 1;
            freed.bytes += dir_size(&d.path);
            if !dry_run {
                let _ = std::fs::remove_dir_all(&d.path);
            }
            continue;
        }
        if d.kind != DirKind::Instance {
            let (n, bytes) = trim_to_cap(&d.path, d.cap_bytes, dry_run);
            freed.entries_removed += n;
            freed.bytes += bytes;
        }
        keep.push(d);
    }
    if !dry_run {
        let _ = registry.maintain(|s| s.dirs.retain(|d| keep.iter().any(|k| k.path == d.path)));
    }
    freed
}

/// Delete the oldest top-level entries of `dir` until it is at most `cap` bytes.
fn trim_to_cap(dir: &Path, cap: u64, dry_run: bool) -> (usize, u64) {
    let mut entries: Vec<(SystemTime, PathBuf, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        // The never-index marker is not scratch: dropping it (it is the oldest entry of a
        // long-lived dir) would let Spotlight back in.
        .filter(|e| e.file_name() != NEVER_INDEX)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?;
            Some((t, e.path(), dir_size(&e.path())))
        })
        .collect();
    let mut total: u64 = entries.iter().map(|e| e.2).sum();
    entries.sort_by_key(|e| e.0);
    let (mut n, mut bytes) = (0, 0);
    for (_, path, size) in entries {
        if total <= cap {
            break;
        }
        if !dry_run {
            let r = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if r.is_err() {
                continue;
            }
        }
        total -= size;
        n += 1;
        bytes += size;
    }
    (n, bytes)
}

/// Disk used by one project, by what it is. Only `.logs` and Atlas's own registered dirs are
/// ever freed; the rest is reported so the user can decide.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ProjectDisk {
    pub root: String,
    pub name: String,
    pub target: u64,
    pub node_modules: u64,
    pub logs: u64,
    /// Git worktrees under `.atlas/worktrees` or `<root>/../<name>-wt-*` style siblings are the
    /// user's: listed with their size, never removed here.
    pub worktrees: Vec<(String, u64)>,
    /// `target/` and `node_modules` (of the project and of its `.atlas/worktrees`) that Spotlight
    /// indexes (macOS; always empty elsewhere): `--fix` adds the [`NEVER_INDEX`] marker.
    pub indexed: Vec<String>,
}

/// Sizes of the well-known heavy directories of the project at `root`.
pub fn project_disk(root: &Path) -> ProjectDisk {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    ProjectDisk {
        root: root.to_string_lossy().into_owned(),
        name,
        target: dir_size(&root.join("target")),
        node_modules: dir_size(&root.join("node_modules")),
        logs: dir_size(&root.join(".logs")),
        worktrees: worktree_sizes(root),
        indexed: indexed_heavy_dirs(root),
    }
}

/// The existing heavy dirs of `root` and of the worktrees under its `.atlas/worktrees` that
/// Spotlight would index.
fn indexed_heavy_dirs(root: &Path) -> Vec<String> {
    let mut checkouts = vec![root.to_path_buf()];
    checkouts.extend(
        std::fs::read_dir(root.join(".atlas").join("worktrees"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir()),
    );
    checkouts
        .iter()
        .flat_map(|c| HEAVY_DIRS.iter().map(|h| c.join(h)))
        .filter(|d| d.is_dir() && spotlight_indexes(d))
        .map(|d| d.to_string_lossy().into_owned())
        .collect()
}

/// The heavy dirs of a project: rewritten constantly, never worth indexing.
const HEAVY_DIRS: [&str; 2] = ["target", "node_modules"];

/// `.atlas/worktrees/*` children of the project: where Atlas Desktop creates worktrees.
fn worktree_sizes(root: &Path) -> Vec<(String, u64)> {
    let dir = root.join(".atlas").join("worktrees");
    let mut v: Vec<(String, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                dir_size(&e.path()),
            )
        })
        .collect();
    v.sort();
    v
}

/// Rotated `.logs` the project may drop: the `*.prev.log` generations and anything over
/// `cap` bytes in total, oldest first. The live `<service>.log` files are never removed.
pub fn trim_logs(logs: &Path, cap: u64, dry_run: bool) -> Freed {
    let mut freed = Freed::default();
    let mut prev: Vec<(SystemTime, PathBuf, u64)> = std::fs::read_dir(logs)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".prev.log"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, e.path(), m.len()))
        })
        .collect();
    prev.sort_by_key(|e| e.0);
    let mut total = dir_size(logs);
    for (_, path, size) in prev {
        if total <= cap {
            break;
        }
        if !dry_run && std::fs::remove_file(&path).is_err() {
            continue;
        }
        total -= size;
        freed.entries_removed += 1;
        freed.bytes += size;
    }
    freed
}

/// What one hygiene pass found and did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Sweep {
    pub orphans: Vec<Orphan>,
    /// Tagged processes of live sessions, left alone.
    pub live: usize,
    /// Orphans still running after SIGKILL (a process in uninterruptible sleep).
    pub survivors: Vec<u32>,
    pub freed: Freed,
    /// Whether anything was changed (`false` for a dry run).
    pub applied: bool,
}

/// One pass of everything hygiene does: find the orphans, and unless `dry_run` end them, drop
/// the registry's dead records and apply the caps of the registered dirs. Cheap enough to run
/// every few seconds from a daemon (the caller gates the scan with [`Registry::has_orphaned_root`]).
pub fn sweep(registry: &Registry, dry_run: bool) -> Sweep {
    let found = scan(registry);
    let mut out = Sweep {
        live: found.live,
        applied: !dry_run,
        ..Sweep::default()
    };
    if !dry_run {
        out.survivors = reap(&found.orphans, GRACE);
    }
    out.orphans = found.orphans;
    out.freed = enforce_caps(registry, dry_run);
    if !dry_run {
        let _ = registry.prune();
    }
    out
}

/// A project's `.logs` may hold this much before its rotated `*.prev.log` generations are
/// dropped (the live `<service>.log` files are never touched).
pub const LOGS_CAP: u64 = 200 * 1024 * 1024;

/// One checkout the app knows: a repository or a linked worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    /// A linked worktree of another entry (its `target/` and `node_modules` are its own).
    pub is_worktree: bool,
}

/// The checkouts of `<base>/desktop/projects.json` (the desktop's registry) that live on this
/// machine and still exist. Missing or unreadable file: none.
pub fn projects_in(base: &Path) -> Vec<Project> {
    #[derive(Deserialize)]
    struct Entry {
        root: String,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        host: Option<String>,
    }
    let text =
        std::fs::read_to_string(base.join("desktop").join("projects.json")).unwrap_or_default();
    serde_json::from_str::<Vec<Entry>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.host.is_none())
        .map(|e| Project {
            root: PathBuf::from(e.root),
            is_worktree: e.parent.is_some(),
        })
        .filter(|p| p.root.is_dir())
        .collect()
}

/// Disk use of one registered directory.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DirUse {
    pub path: String,
    pub kind: DirKind,
    pub bytes: u64,
    pub cap_bytes: u64,
    /// Its owner session is gone: `--fix` removes it whole.
    pub orphaned: bool,
}

/// Everything `atlas doctor --hygiene` shows, and the desktop panel after it.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub orphans: Vec<Orphan>,
    /// Tagged processes of live sessions.
    pub live: usize,
    pub projects: Vec<ProjectDisk>,
    pub dirs: Vec<DirUse>,
    /// What `--fix` would free (or just did): registered dirs and rotated logs, and the
    /// orphaned processes' count is `orphans.len()`.
    pub freeable: Freed,
}

/// Look, change nothing: orphans, disk use per project, registered dirs, and what a fix would
/// free. `base` is `~/.atlas` (it holds the desktop's project registry).
pub fn report(registry: &Registry, base: &Path) -> Report {
    let found = scan(registry);
    let live = live_sessions(&registry.roots());
    let mut out = Report {
        orphans: found.orphans,
        live: found.live,
        freeable: enforce_caps(registry, true),
        ..Report::default()
    };
    out.dirs = registry
        .dirs()
        .into_iter()
        .map(|d| DirUse {
            bytes: dir_size(&d.path),
            orphaned: d.session.as_ref().is_some_and(|s| !live.contains(s))
                && !d.owner.is_some_and(|o| o.alive()),
            path: d.path.to_string_lossy().into_owned(),
            kind: d.kind,
            cap_bytes: d.cap_bytes,
        })
        .collect();
    for p in projects_in(base) {
        let disk = project_disk(&p.root);
        let logs = trim_logs(&p.root.join(".logs"), LOGS_CAP, true);
        out.freeable.entries_removed += logs.entries_removed;
        out.freeable.bytes += logs.bytes;
        out.projects.push(disk);
    }
    out
}

/// What a fix did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Fixed {
    pub reaped: usize,
    pub survivors: Vec<u32>,
    pub freed: Freed,
    /// Directories that got a [`NEVER_INDEX`] marker (marker files only, nothing deleted).
    pub marked: Vec<String>,
}

/// Free what Atlas owns and nothing else: end the orphaned tagged processes, delete registered
/// dirs of dead sessions, trim registered dirs to their caps, drop rotated `.prev.log` of
/// projects over [`LOGS_CAP`] and keep Spotlight out of heavy dirs (an empty marker file each).
/// `target/`, `node_modules` and worktrees are the user's: reported, never removed here.
pub fn fix(registry: &Registry, base: &Path) -> Fixed {
    let s = sweep(registry, false);
    let mut freed = s.freed;
    let mut marked = Vec::new();
    for p in projects_in(base) {
        let l = trim_logs(&p.root.join(".logs"), LOGS_CAP, false);
        freed.entries_removed += l.entries_removed;
        freed.bytes += l.bytes;
        for d in indexed_heavy_dirs(&p.root) {
            if mark_never_index(Path::new(&d)) {
                marked.push(d);
            }
        }
    }
    Fixed {
        reaped: s.orphans.len().saturating_sub(s.survivors.len()),
        survivors: s.survivors,
        freed,
        marked,
    }
}

#[cfg(test)]
mod tests;
