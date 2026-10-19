use std::collections::HashMap;
use std::time::Duration;

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// The two stops a task tree gets: asked (`SIGTERM`), then forced (`SIGKILL`). Windows has no
/// polite stop for a console program it does not share a console with, so both end the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Term,
    Kill,
}

/// A service or action command: `script` through the platform shell (`sh -c`, `cmd /d /s /c` on
/// Windows), in a process group of its own on Unix (so a stop reaches its whole tree) and without
/// a console window on Windows (the desktop app has none to share).
pub fn task_command(script: &str) -> tokio::process::Command {
    let mut cmd = atlas_core::sys::shell_tokio(script);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    atlas_core::sys::CommandExt::no_window(&mut cmd);
    cmd
}

/// Test-only: a test's POSIX one-liner (`a; b`, `a && b`, `sleep N`, `mkdir -p d`,
/// `echo x >&2`, `$VAR`, `exit N`) in the dialect of the platform shell, so scheduler, action and
/// pre_command tests run real processes on Windows too (`cmd`: `&` sequencing, `ping` as sleep,
/// `%VAR%`, redirections first so echo keeps no trailing space).
#[cfg(test)]
pub(crate) fn test_sh(script: &str) -> String {
    if cfg!(unix) {
        return script.to_string();
    }
    let var = regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("regex");
    let one = |c: &str| -> String {
        let c = c.trim();
        if let Some(n) = c
            .strip_prefix("sleep ")
            .and_then(|n| n.trim().parse::<u64>().ok())
        {
            return format!("ping -n {} 127.0.0.1 >NUL", n + 1);
        }
        if let Some(d) = c.strip_prefix("mkdir -p ") {
            return format!("(if not exist {d} mkdir {d})");
        }
        match c {
            "true" => return "exit 0".into(),
            "false" => return "exit 1".into(),
            _ => {}
        }
        let c = var.replace_all(c, "%$1%").into_owned();
        match c.split_once(" >") {
            Some((cmd, redir)) => format!(">{} {}", redir.trim(), cmd.trim()),
            None => c,
        }
    };
    script
        .split(';')
        .map(|part| part.split("&&").map(one).collect::<Vec<_>>().join("&&"))
        .collect::<Vec<_>>()
        .join("&")
}

/// One process-table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: i32,
    pub ppid: i32,
    pub pgid: i32,
}

/// Snapshot of the whole process table (`ps -ax -o pid=,ppid=,pgid=`).
/// Empty when `ps` is unavailable — callers then fall back to group-only kills.
#[cfg(unix)]
pub fn process_table() -> Vec<ProcEntry> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=,ppid=,pgid="])
        .output()
    else {
        return Vec::new();
    };
    parse_process_table(&String::from_utf8_lossy(&out.stdout))
}

/// Snapshot of the whole process table (Toolhelp). Windows has no process groups: `pgid` is 0,
/// which [`KillTree`] never signals. A recorded parent pid outlives its process and pids are
/// reused, so a row whose parent was created after it is cut loose (`ppid` 0).
#[cfg(windows)]
pub fn process_table() -> Vec<ProcEntry> {
    let rows = atlas_core::sys::process_rows();
    let born: HashMap<u32, u64> = rows
        .iter()
        .filter_map(|r| Some((r.pid, atlas_core::sys::start_time_secs(r.pid)?)))
        .collect();
    rows.iter()
        .map(|r| {
            let stale_parent = matches!(
                (born.get(&r.ppid), born.get(&r.pid)),
                (Some(p), Some(c)) if p > c
            );
            ProcEntry {
                pid: r.pid as i32,
                ppid: if stale_parent { 0 } else { r.ppid as i32 },
                pgid: 0,
            }
        })
        .collect()
}

/// `process_table` off the async runtime: `ps` blocks for tens of ms.
pub async fn process_table_async() -> Vec<ProcEntry> {
    tokio::task::spawn_blocking(process_table)
        .await
        .unwrap_or_default()
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_process_table(text: &str) -> Vec<ProcEntry> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().map(|f| f.parse::<i32>());
            Some(ProcEntry {
                pid: it.next()?.ok()?,
                ppid: it.next()?.ok()?,
                pgid: it.next()?.ok()?,
            })
        })
        .collect()
}

/// Pure: every descendant of `root` in `table` (BFS over ppid), excluding `root`
/// itself. Cycle-safe.
pub fn descendants(root: i32, table: &[ProcEntry]) -> Vec<ProcEntry> {
    let mut children: HashMap<i32, Vec<&ProcEntry>> = HashMap::new();
    for e in table {
        children.entry(e.ppid).or_default().push(e);
    }
    let mut seen = std::collections::HashSet::from([root]);
    let mut queue = std::collections::VecDeque::from([root]);
    let mut out = Vec::new();
    while let Some(p) = queue.pop_front() {
        for e in children.get(&p).into_iter().flatten() {
            if seen.insert(e.pid) {
                out.push(**e);
                queue.push_back(e.pid);
            }
        }
    }
    out
}

/// A whole task tree to kill: the root's process group plus every descendant
/// (pid and pgid), snapshotted once while the tree is still connected.
#[derive(Default, Debug)]
pub struct KillTree {
    pids: Vec<i32>,
    pgids: Vec<i32>,
}

impl KillTree {
    /// Collect the tree under `root` (whose group is `root_pgid`, or `None` for a
    /// stale root whose group we do not trust). Never includes pid <= 1, ourselves
    /// or our ancestors, nor the pgid of any of those.
    pub fn collect(root: i32, root_pgid: Option<i32>, table: &[ProcEntry]) -> Self {
        let protected = protected_pids(table);
        let mut tree = KillTree::default();
        if let Some(g) = root_pgid {
            tree.add_pgid(g, &protected);
        }
        tree.add_pid(root, &protected);
        for e in descendants(root, table) {
            tree.add_pgid(e.pgid, &protected);
            tree.add_pid(e.pid, &protected);
        }
        tree
    }

    fn add_pid(&mut self, pid: i32, protected: &[i32]) {
        if pid > 1 && !protected.contains(&pid) && !self.pids.contains(&pid) {
            self.pids.push(pid);
        }
    }

    fn add_pgid(&mut self, pgid: i32, protected: &[i32]) {
        if pgid > 1 && !protected.contains(&pgid) && !self.pgids.contains(&pgid) {
            self.pgids.push(pgid);
        }
    }

    pub fn merge(&mut self, other: KillTree) {
        self.pids.extend(other.pids);
        self.pgids.extend(other.pgids);
    }

    pub fn is_empty(&self) -> bool {
        self.pids.is_empty() && self.pgids.is_empty()
    }

    #[cfg(unix)]
    pub fn signal(&self, stop: Stop) {
        let sig = match stop {
            Stop::Term => Signal::SIGTERM,
            Stop::Kill => Signal::SIGKILL,
        };
        for &g in &self.pgids {
            let _ = signal::kill(Pid::from_raw(-g), sig);
        }
        for &p in &self.pids {
            let _ = signal::kill(Pid::from_raw(p), sig);
        }
    }

    /// Windows: end every member (there are no groups; `collect` snapshotted the descendants).
    #[cfg(windows)]
    pub fn signal(&self, _stop: Stop) {
        for &p in &self.pids {
            atlas_core::sys::force_kill(p as u32);
        }
    }

    /// True while any member pid still exists.
    #[cfg(windows)]
    pub fn alive(&self) -> bool {
        self.pids
            .iter()
            .any(|&p| atlas_core::sys::pid_alive(p as u32))
    }

    /// True while any member pid or group still exists.
    #[cfg(unix)]
    pub fn alive(&self) -> bool {
        self.pgids.iter().any(|&g| group_alive(g))
            || self.pids.iter().any(|&p| {
                !matches!(
                    signal::kill(Pid::from_raw(p), None),
                    Err(nix::errno::Errno::ESRCH)
                )
            })
    }
}

/// Pids that must never be signalled: init, ourselves, and our ancestors.
fn protected_pids(table: &[ProcEntry]) -> Vec<i32> {
    let parent: HashMap<i32, i32> = table.iter().map(|e| (e.pid, e.ppid)).collect();
    let me = std::process::id() as i32;
    let mut out = vec![1, me];
    let mut p = me;
    while let Some(&pp) = parent.get(&p) {
        if pp <= 1 || out.contains(&pp) {
            break;
        }
        out.push(pp);
        p = pp;
    }
    // `ps` may be unavailable/empty: the direct parent is always protected.
    out.push(atlas_core::sys::parent_pid() as i32);
    out
}

/// True while any process remains in the process group `pgid`.
#[cfg(unix)]
fn group_alive(pgid: i32) -> bool {
    // EPERM still means the group exists; only ESRCH means it is gone.
    !matches!(
        signal::kill(Pid::from_raw(-pgid), None),
        Err(nix::errno::Errno::ESRCH)
    )
}

/// SIGTERM `tree`, poll every 50ms until it is gone or `grace` expires, then
/// SIGKILL the survivors. `reap` runs on every poll (reaping zombie leaders).
pub async fn kill_tree_gracefully(tree: &KillTree, grace: Duration, mut reap: impl FnMut()) {
    tree.signal(Stop::Term);
    if grace.is_zero() {
        return;
    }
    let deadline = tokio::time::Instant::now() + grace;
    loop {
        reap();
        if !tree.alive() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            tree.signal(Stop::Kill);
            reap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// SIGTERM the whole task tree of a child WE spawned this run (`pgid` == child
/// pid via `process_group(0)`): its process group plus every descendant, which
/// tools like turbo move into new groups. Polls every 50ms until all are gone or
/// `grace` expires, then SIGKILLs survivors. The leader is reaped on each poll,
/// since an unreaped zombie leader would keep the group visible.
/// Never use for stale pids — see `kill_stale_tree`.
pub async fn kill_gracefully(child: &mut tokio::process::Child, grace: Duration) {
    let Some(pid) = child.id() else { return };
    let tree = KillTree::collect(pid as i32, Some(pid as i32), &process_table_async().await);
    kill_tree_gracefully(&tree, grace, || {
        let _ = child.try_wait();
    })
    .await;
}

/// Kill a verified stale root (from a previous run's `.pids`) and all its
/// descendants. Its process group is not trusted (pgid may have been recycled),
/// so only exact pids plus descendants' own groups are signalled.
pub async fn kill_stale_tree(roots: &[i32], grace: Duration) {
    let table = process_table_async().await;
    let mut tree = KillTree::default();
    for &r in roots {
        tree.merge(KillTree::collect(r, None, &table));
    }
    if !tree.is_empty() {
        kill_tree_gracefully(&tree, grace, || {}).await;
    }
}

/// Kill a single exact PID (not the group) gracefully: SIGTERM, poll up to
/// `grace` for it to exit, SIGKILL if it is still alive.
/// Used for orphan cleanup where we don't trust the process group.
pub async fn kill_exact_gracefully(pid: i32, grace: Duration) {
    if pid <= 0 {
        return;
    }
    atlas_core::sys::terminate(pid as u32);
    if grace.is_zero() {
        return;
    }
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline {
        if !crate::runtime::health::check_process_alive(pid as u32).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100).min(grace)).await;
    }
    if crate::runtime::health::check_process_alive(pid as u32).await {
        atlas_core::sys::force_kill(pid as u32);
    }
}

/// Process names that must never be killed on the strength of a stale pid
/// (recycled into a shell, container runtime, init, the desktop shell, or atlas itself).
/// `cmd.exe` is not here: on Windows it is the root of every service (`cmd /c`).
const PID_BLOCKLIST: [&str; 14] = [
    "fish",
    "zsh",
    "dash",
    "orb",
    "docker",
    "containerd",
    "launchd",
    "systemd",
    "atlas",
    "pwsh",
    "powershell",
    "explorer.exe",
    "svchost",
    "windowsterminal",
];

/// Pure: is a process with this (lowercase) name safe to kill as a leftover
/// service? Anything not blocklisted qualifies, since atlas.yaml commands are
/// arbitrary (bun, node, air, go, cargo, uvicorn, custom binaries …). An empty
/// name means dead/unreadable — skip.
pub fn name_is_killable(name: &str) -> bool {
    !name.is_empty() && !PID_BLOCKLIST.iter().any(|&s| name.contains(s))
}

/// Lowercase executable path/name of `pid`, or empty string.
pub fn proc_name(pid: i32) -> String {
    #[cfg(windows)]
    {
        atlas_core::sys::exe_path(pid as u32)
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: buf is valid for buf.len() bytes.
        let ret = unsafe {
            libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut libc::c_void, buf.len() as u32)
        };
        if ret > 0 {
            return std::ffi::CStr::from_bytes_until_nul(&buf)
                .map(|c| c.to_string_lossy().to_lowercase())
                .unwrap_or_default();
        }
        String::new()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default()
    }
}

/// True when `pid`'s name passes the blocklist.
pub fn pid_is_service_runtime(pid: i32) -> bool {
    name_is_killable(&proc_name(pid))
}

/// Process start time as Unix seconds, or None if unreadable.
pub fn proc_start_secs(pid: i32) -> Option<u64> {
    #[cfg(windows)]
    {
        atlas_core::sys::start_time_secs(pid as u32)
    }
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // SAFETY: info is a valid, correctly sized proc_bsdinfo out-buffer.
        let ret = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                size,
            )
        };
        (ret == size).then_some(info.pbi_start_tvsec)
    }
    #[cfg(target_os = "linux")]
    {
        // /proc/<pid> creation time is not the process start; use stat field 22
        // (clock ticks since boot) + btime from /proc/stat.
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // comm may contain spaces/parens: fields resume after the last ')'.
        let rest = stat.rsplit_once(')')?.1;
        let ticks: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
        let btime: u64 = std::fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())?;
        // SAFETY: sysconf has no preconditions.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
        Some(btime + ticks / hz)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocklist_rejects_shells_and_empty() {
        assert!(!name_is_killable(""));
        assert!(!name_is_killable("/opt/homebrew/bin/fish"));
        assert!(!name_is_killable("/bin/zsh"));
        assert!(!name_is_killable("/Applications/OrbStack.app/orbstack"));
        assert!(name_is_killable("/Users/x/.bun/bin/bun"));
        assert!(name_is_killable("node"));
    }

    fn e(pid: i32, ppid: i32, pgid: i32) -> ProcEntry {
        ProcEntry { pid, ppid, pgid }
    }

    #[test]
    fn descendants_walks_ppid_tree_across_groups() {
        // 100 (root) -> 101 -> 102(new pgid) -> 103(new pgid); 200 unrelated.
        let table = [
            e(100, 1, 100),
            e(101, 100, 100),
            e(102, 101, 102),
            e(103, 102, 103),
            e(200, 1, 200),
            e(201, 200, 200),
        ];
        let mut got: Vec<i32> = descendants(100, &table).iter().map(|d| d.pid).collect();
        got.sort();
        assert_eq!(got, vec![101, 102, 103]);
        assert!(descendants(300, &table).is_empty());
        // Cycle-safe.
        let cyc = [e(10, 11, 10), e(11, 10, 11)];
        assert_eq!(descendants(10, &cyc).len(), 1);
    }

    #[test]
    fn parse_ps_output() {
        let t = parse_process_table("  1     0     1\n 42    1   42\nbad line\n");
        assert_eq!(t, vec![e(1, 0, 1), e(42, 1, 42)]);
    }

    #[test]
    fn kill_tree_collects_descendant_groups_but_never_protected() {
        let me = std::process::id() as i32;
        // me's parent 500 (a shell); root 100 has a child that (oddly) is us.
        let table = [
            e(500, 1, 500),
            e(me, 500, 500),
            e(100, 1, 100),
            e(101, 100, 101),
            e(me + 1000, 101, me + 1000),
            e(102, 100, 500), // child sharing our ancestor's group
        ];
        let tree = KillTree::collect(100, Some(100), &table);
        assert!(tree.pgids.contains(&100) && tree.pgids.contains(&101));
        assert!(tree.pids.contains(&101) && tree.pids.contains(&(me + 1000)));
        assert!(!tree.pgids.contains(&500), "never our ancestor's group");
        assert!(!tree.pids.contains(&1) && !tree.pgids.contains(&1));
        let init_child = KillTree::collect(1, None, &table);
        assert!(!init_child.pids.contains(&1));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn kill_gracefully_reaches_setsid_grandchildren() {
        let pidfile = std::env::temp_dir().join(format!("atlas-tree-{}", std::process::id()));
        // The grandchild leaves the child's session/process group (like a turbo
        // task); perl's POSIX::setsid keeps this portable (no `setsid` on macOS).
        let script = format!(
            "perl -e 'use POSIX; POSIX::setsid(); open(F, \">{p}\"); print F $$; close F; \
             exec \"sleep\", \"300\"' & wait",
            p = pidfile.display()
        );
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", &script])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut gc = 0i32;
        for _ in 0..100 {
            if let Ok(s) = std::fs::read_to_string(&pidfile)
                && let Ok(p) = s.trim().parse()
            {
                gc = p;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(gc > 1, "grandchild started");
        // SAFETY: getpgid has no preconditions.
        let gc_pgid = unsafe { libc::getpgid(gc) };
        assert_eq!(gc_pgid, gc, "grandchild is in its own new group");
        assert_ne!(gc_pgid, child.id().unwrap() as i32);
        kill_gracefully(&mut child, Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let alive = !matches!(
            signal::kill(Pid::from_raw(gc), None),
            Err(nix::errno::Errno::ESRCH)
        );
        let _ = std::fs::remove_file(&pidfile);
        if alive {
            let _ = signal::kill(Pid::from_raw(gc), Signal::SIGKILL);
        }
        assert!(!alive, "setsid grandchild must die with its task tree");
    }

    /// Windows has no process groups: a service's tree is its descendants, and stopping the
    /// `cmd /c` root alone would leave the real program (node, bun…) running.
    #[cfg(windows)]
    #[tokio::test]
    async fn kill_gracefully_ends_the_program_under_the_shell() {
        let mut child = atlas_core::sys::shell_tokio("ping -n 60 127.0.0.1 >NUL")
            .spawn()
            .unwrap();
        let root = child.id().unwrap() as i32;
        let mut kids = Vec::new();
        for _ in 0..100 {
            kids = descendants(root, &process_table());
            if !kids.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!kids.is_empty(), "ping never showed up under cmd");
        kill_gracefully(&mut child, Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        for k in kids {
            assert!(
                !atlas_core::sys::pid_alive(k.pid as u32),
                "{} survived",
                k.pid
            );
        }
    }

    #[test]
    fn own_process_has_name_and_start_time() {
        let me = std::process::id() as i32;
        assert!(!proc_name(me).is_empty());
        let start = proc_start_secs(me).expect("start time of self");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(start <= now && now - start < 24 * 3600);
    }
}
