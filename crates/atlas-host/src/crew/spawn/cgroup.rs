//! A memory cap per crewmate (issue #296): one runaway agent is killed by the kernel alone, instead of
//! pushing the whole crew service (hostd and every other crewmate) into the unit's `MemoryMax=80%`.
//!
//! The helper's unit delegates its cgroup (`Delegate=memory pids`, `DelegateSubgroup=helper`), so the
//! helper owns the subtree below its own cgroup. Every crewmate gets one **leaf** there,
//! `cm-<slug>/`, with `memory.max` = the cap, `memory.oom.group=1` (the kernel kills the whole group,
//! never one process of an agent's tree) and `pids.max`. The child joins its leaf in `pre_exec`,
//! before `setuid`, so there is no window in which an agent runs uncapped.
//!
//! Degrade, never break: without cgroup v2, without a delegated cgroup, off Linux or as a non-root
//! helper there is no cap, [`Pool::detect`] says why once, and spawning is untouched.
//!
//! The pure parts (cap arithmetic, leaf names, file parsing) compile everywhere and are unit tested;
//! the cgroup file work is `#[cfg(target_os = "linux")]`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::validate::slug_of;

/// Prefix of a crewmate's leaf cgroup (`cm-<12 hex>`).
const LEAF_PREFIX: &str = "cm-";
/// Name of the helper's own subgroup (`DelegateSubgroup=helper`): the helper process lives there so
/// the delegated cgroup itself is an inner node that may have children.
pub const HELPER_SUBGROUP: &str = "helper";
/// The smallest cap ever computed: below this an agent (a full CLI plus the tools it runs) cannot work.
pub const MIN_CAP: u64 = 1 << 30;
/// Processes one crewmate may have: far above any real agent, far below a fork bomb.
pub const PIDS_MAX: u32 = 4096;

/// What the crew reports about the cap (`crew_settings_get.memory_cap`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub available: bool,
    /// Why there is no cap, when `available` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The cap in bytes agents run under now, when `available`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

impl Status {
    pub fn off(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
            bytes: None,
        }
    }

    pub fn on(bytes: u64) -> Self {
        Self {
            available: true,
            reason: None,
            bytes: Some(bytes),
        }
    }
}

/// The cap one agent gets, in bytes: `explicit` (the `agent_memory_max` setting) when set, otherwise
/// 75 % of the machine's memory shared by `max_agents` agents, never below [`MIN_CAP`]. The 25 % left
/// is for hostd, the other services and the page cache.
pub fn compute_cap(explicit: Option<u64>, mem_total: u64, max_agents: usize) -> u64 {
    if let Some(n) = explicit {
        return n;
    }
    let agents = max_agents.max(1) as u64;
    (mem_total / 4 * 3 / agents).max(MIN_CAP)
}

/// `MemTotal` of `/proc/meminfo` text, in bytes (`MemTotal:  16384000 kB`).
pub fn mem_total_of(meminfo: &str) -> Option<u64> {
    let kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    kb.checked_mul(1024)
}

/// The leaf cgroup directory name of `user` (`atlas-cm-<slug>` becomes `cm-<slug>`). A name that is
/// not a crewmate's user is refused: it becomes a path component under a root helper.
pub fn leaf_name(user: &str) -> Option<String> {
    slug_of(user).ok().map(|s| format!("{LEAF_PREFIX}{s}"))
}

/// The value of `key` in a flat-keyed cgroup file (`memory.events`: `oom_kill 2`).
pub fn counter(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .filter_map(|l| l.split_once(' '))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.trim().parse().ok())
}

/// A byte count the way people read it: `1.5 GiB`, `512 MiB`.
pub fn human(bytes: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    const MIB: f64 = (1u64 << 20) as f64;
    let b = bytes as f64;
    if b >= GIB {
        let g = b / GIB;
        if (g - g.round()).abs() < 0.05 {
            format!("{} GiB", g.round() as u64)
        } else {
            format!("{g:.1} GiB")
        }
    } else {
        format!("{} MiB", (b / MIB).round() as u64)
    }
}

/// What a task says when the kernel killed its agent for memory.
pub fn oom_reason(cap: u64) -> String {
    format!(
        "ran out of memory (cap {}); raise agent_memory_max",
        human(cap)
    )
}

/// The cgroup the helper's `/proc/self/cgroup` says it is in (cgroup v2: one line, `0::/path`).
pub fn own_cgroup_of(proc_self_cgroup: &str) -> Option<&str> {
    proc_self_cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .map(str::trim)
        .filter(|p| p.starts_with('/'))
}

/// `/a/b/helper` becomes `/a/b` when the last component is [`HELPER_SUBGROUP`]: the delegated
/// cgroup, from the one the helper process sits in (`DelegateSubgroup=helper`).
pub fn delegated_root_of(own: &str) -> &str {
    own.strip_suffix(&format!("/{HELPER_SUBGROUP}"))
        .unwrap_or(own)
}

/// Where the cgroup file system is mounted.
pub const CGROUP_MOUNT: &str = "/sys/fs/cgroup";

#[cfg(target_os = "linux")]
pub use linux::{Leaf, Pool};

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::fd::{AsRawFd, RawFd};
    use std::path::{Path, PathBuf};

    use parking_lot::Mutex;

    use super::{
        CGROUP_MOUNT, HELPER_SUBGROUP, PIDS_MAX, Status, counter, delegated_root_of, leaf_name,
        own_cgroup_of,
    };

    /// One crewmate's leaf: the directory and the open `cgroup.procs` the child joins through.
    pub struct Leaf {
        pub dir: PathBuf,
        procs: File,
    }

    impl Leaf {
        /// The fd a child writes `0` to (its own pid) to join the leaf. Prepared before `fork`.
        pub fn procs_fd(&self) -> RawFd {
            self.procs.as_raw_fd()
        }

        /// How many processes the kernel killed in this leaf for memory, ever.
        pub fn oom_kills(&self) -> u64 {
            std::fs::read_to_string(self.dir.join("memory.events"))
                .ok()
                .and_then(|t| counter(&t, "oom_kill"))
                .unwrap_or(0)
        }
    }

    /// The helper's cgroup subtree: where leaves are made and which cap they get.
    pub struct Pool {
        root: PathBuf,
        cap: Mutex<u64>,
        leaves: Mutex<HashMap<String, std::sync::Arc<Leaf>>>,
    }

    impl Pool {
        /// Find the helper's delegated cgroup and make it usable, or say why there is none. Never
        /// panics and never assumes: each step that can be missing is checked.
        pub fn detect(cap: u64) -> Result<Self, String> {
            Self::detect_at(Path::new(CGROUP_MOUNT), Path::new("/proc/self/cgroup"), cap)
        }

        /// [`Pool::detect`] with the mount and the cgroup file given (tests).
        pub fn detect_at(mount: &Path, proc_cgroup: &Path, cap: u64) -> Result<Self, String> {
            // SAFETY: plain getter.
            if unsafe { libc::geteuid() } != 0 {
                return Err("the spawn helper is not root".into());
            }
            if !mount.join("cgroup.controllers").is_file() {
                return Err("cgroup v2 is not mounted (no unified hierarchy)".into());
            }
            let text = std::fs::read_to_string(proc_cgroup)
                .map_err(|e| format!("cannot read {}: {e}", proc_cgroup.display()))?;
            let own = own_cgroup_of(&text).ok_or("this process is not in a cgroup v2 path")?;
            let own_dir = mount.join(own.trim_start_matches('/'));
            // systemd 254+ with `DelegateSubgroup=helper` starts the helper in `<unit>/helper`; older
            // systemd leaves it in the unit's cgroup, where it must move itself down before the unit's
            // cgroup can have children.
            let root = if own.ends_with(&format!("/{HELPER_SUBGROUP}")) {
                mount.join(delegated_root_of(own).trim_start_matches('/'))
            } else {
                let sub = own_dir.join(HELPER_SUBGROUP);
                std::fs::create_dir_all(&sub)
                    .map_err(|e| format!("cannot create {}: {e}", sub.display()))?;
                std::fs::write(sub.join("cgroup.procs"), "0")
                    .map_err(|e| format!("cannot move into {}: {e}", sub.display()))?;
                own_dir
            };
            let controllers = std::fs::read_to_string(root.join("cgroup.controllers"))
                .map_err(|e| format!("cannot read {}: {e}", root.display()))?;
            if !controllers.split_whitespace().any(|c| c == "memory") {
                return Err(format!(
                    "the memory controller is not delegated to {} (Delegate=memory pids)",
                    root.display()
                ));
            }
            // Delegation only makes controllers available; the leaves need them enabled.
            let sub = root.join("cgroup.subtree_control");
            let have = std::fs::read_to_string(&sub).unwrap_or_default();
            let want: Vec<&str> = ["memory", "pids"]
                .into_iter()
                .filter(|c| controllers.split_whitespace().any(|x| x == *c))
                .filter(|c| !have.split_whitespace().any(|x| x == *c))
                .collect();
            if !want.is_empty() {
                let line = want
                    .iter()
                    .map(|c| format!("+{c}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                std::fs::write(&sub, line)
                    .map_err(|e| format!("cannot enable controllers in {}: {e}", root.display()))?;
            }
            Self::over(root, cap).map_err(|e| e.to_string())
        }

        /// A pool over a cgroup that is already delegated and has the controllers enabled (the scratch
        /// cgroup of a test).
        pub fn over(root: PathBuf, cap: u64) -> io::Result<Self> {
            if !root.join("cgroup.controllers").is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} is not a cgroup", root.display()),
                ));
            }
            Ok(Self {
                root,
                cap: Mutex::new(cap),
                leaves: Mutex::new(HashMap::new()),
            })
        }

        /// The cap new and existing leaves run under.
        pub fn cap(&self) -> u64 {
            *self.cap.lock()
        }

        /// Make `bytes` the cap of every leaf, existing ones included (the setting changed).
        pub fn set_cap(&self, bytes: u64) -> io::Result<()> {
            *self.cap.lock() = bytes;
            for leaf in self.leaves.lock().values() {
                write_limits(&leaf.dir, bytes)?;
            }
            Ok(())
        }

        pub fn status(&self) -> Status {
            Status::on(self.cap())
        }

        /// The leaf of `user`, created on first use (idempotent); `None` for a name that is not a
        /// crewmate's user.
        pub fn leaf(&self, user: &str) -> io::Result<std::sync::Arc<Leaf>> {
            let name = leaf_name(user).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "not a crewmate user")
            })?;
            let mut leaves = self.leaves.lock();
            if let Some(l) = leaves.get(&name) {
                return Ok(l.clone());
            }
            let dir = self.root.join(&name);
            std::fs::create_dir_all(&dir)?;
            write_limits(&dir, self.cap())?;
            let procs = OpenOptions::new()
                .write(true)
                .open(dir.join("cgroup.procs"))?;
            let leaf = std::sync::Arc::new(Leaf { dir, procs });
            leaves.insert(name, leaf.clone());
            Ok(leaf)
        }
    }

    /// `memory.max`, group kill and the process cap of one leaf. `memory.swap.max` is 0: swap would
    /// turn "killed at the cap" into a thrashing server.
    fn write_limits(dir: &Path, bytes: u64) -> io::Result<()> {
        std::fs::write(dir.join("memory.max"), bytes.to_string())?;
        std::fs::write(dir.join("memory.oom.group"), "1")?;
        // Not every kernel has swap accounting; a missing file is not a failure.
        let _ = std::fs::write(dir.join("memory.swap.max"), "0");
        std::fs::write(dir.join("pids.max"), PIDS_MAX.to_string())
    }

    /// The child's side, for `pre_exec`: write `0` (itself) to `cgroup.procs`. Only `write(2)` on an
    /// fd opened before the fork, so it is async-signal-safe.
    pub fn join(fd: RawFd) -> io::Result<()> {
        let zero = b"0";
        // SAFETY: write(2) of one byte from a static buffer to an open fd.
        let n = unsafe { libc::write(fd, zero.as_ptr().cast(), 1) };
        if n == 1 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::join;

/// The path of the cgroup the helper's unit owns, for `ReadWritePaths=` in the unit file.
pub fn unit_cgroup_path(unit: &str) -> PathBuf {
    // Joined as text: the CLI writes this into a unit file on whatever OS it runs, and a Windows
    // `Path::join` would put backslashes in a systemd unit.
    PathBuf::from(format!("{CGROUP_MOUNT}/system.slice/{unit}"))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use std::path::Path;

    use super::*;

    const GIB: u64 = 1 << 30;

    #[test]
    fn the_default_cap_is_three_quarters_of_memory_shared_by_the_agents() {
        // 16 GiB, 4 agents: 12 GiB / 4 = 3 GiB each.
        assert_eq!(compute_cap(None, 16 * GIB, 4), 3 * GIB);
        // One agent may use three quarters of the machine.
        assert_eq!(compute_cap(None, 16 * GIB, 1), 12 * GIB);
    }

    #[test]
    fn the_default_cap_never_goes_below_one_gib() {
        // 2 GiB shared by 4: 384 MiB each would starve any agent.
        assert_eq!(compute_cap(None, 2 * GIB, 4), GIB);
        // A limit of 0 (settings forbid it, but the arithmetic must not divide by zero).
        assert_eq!(compute_cap(None, 8 * GIB, 0), 6 * GIB);
    }

    #[test]
    fn an_explicit_cap_wins_even_below_the_floor() {
        // The owner asked for it: 512 MiB is theirs to choose.
        assert_eq!(compute_cap(Some(512 << 20), 64 * GIB, 4), 512 << 20);
    }

    #[test]
    fn mem_total_reads_kilobytes_as_bytes() {
        let t = "MemTotal:       16384000 kB\nMemFree:  100 kB\n";
        assert_eq!(mem_total_of(t), Some(16_384_000 * 1024));
        assert_eq!(mem_total_of("MemFree: 1 kB\n"), None);
        assert_eq!(mem_total_of("MemTotal: lots kB\n"), None);
    }

    #[test]
    fn a_leaf_name_comes_only_from_a_crewmate_user() {
        assert_eq!(
            leaf_name("atlas-cm-3f2a9c1e5b7d").as_deref(),
            Some("cm-3f2a9c1e5b7d")
        );
        // These become a path component under a root helper: nothing but the exact shape.
        for bad in [
            "",
            "root",
            "atlas-cm-",
            "atlas-cm-../etc",
            "atlas-cm-3f2a9c1e5b7",
            "atlas-cm-3F2A9C1E5B7D",
            "atlas-cm-3f2a9c1e5b7d/x",
            "atlas-cm-3f2a9c1e5b7d\0",
        ] {
            assert_eq!(leaf_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn counter_reads_one_key_of_memory_events() {
        let ev = "low 0\nhigh 0\nmax 12\noom 3\noom_kill 2\noom_group_kill 1\n";
        assert_eq!(counter(ev, "oom_kill"), Some(2));
        // `oom` is not a prefix match of `oom_kill`, and the reverse.
        assert_eq!(counter(ev, "oom"), Some(3));
        assert_eq!(counter(ev, "nope"), None);
        assert_eq!(counter("oom_kill x\n", "oom_kill"), None);
    }

    #[test]
    fn sizes_read_the_way_people_say_them() {
        assert_eq!(human(3 * GIB), "3 GiB");
        assert_eq!(human(1536 << 20), "1.5 GiB");
        assert_eq!(human(512 << 20), "512 MiB");
        assert_eq!(human(64 << 20), "64 MiB");
    }

    #[test]
    fn the_oom_reason_names_the_cap_and_the_setting_to_raise() {
        let r = oom_reason(64 << 20);
        assert_eq!(r, "ran out of memory (cap 64 MiB); raise agent_memory_max");
    }

    #[test]
    fn the_helpers_own_cgroup_is_read_from_the_v2_line_only() {
        assert_eq!(
            own_cgroup_of("0::/system.slice/atlas-crew-spawn.service/helper\n"),
            Some("/system.slice/atlas-crew-spawn.service/helper")
        );
        // A v1 or hybrid host lists controller lines too; only `0::` is the unified hierarchy.
        assert_eq!(own_cgroup_of("12:memory:/x\n0::/y\n"), Some("/y"));
        assert_eq!(own_cgroup_of("12:memory:/x\n"), None);
        assert_eq!(own_cgroup_of("0::relative\n"), None);
    }

    #[test]
    fn the_delegated_cgroup_is_the_one_above_the_helper_subgroup() {
        assert_eq!(
            delegated_root_of("/system.slice/atlas-crew-spawn.service/helper"),
            "/system.slice/atlas-crew-spawn.service"
        );
        // Older systemd: the helper is in the unit's cgroup itself.
        assert_eq!(
            delegated_root_of("/system.slice/atlas-crew-spawn.service"),
            "/system.slice/atlas-crew-spawn.service"
        );
    }

    // Incident (#385 CI): the CLI writes this path into a systemd unit; built with `Path::join` it had
    // backslashes when `atlas crew install` ran on Windows.
    #[test]
    fn the_unit_cgroup_path_is_a_unix_path_on_every_os() {
        assert_eq!(
            unit_cgroup_path("atlas-crew-spawn.service").to_str(),
            Some("/sys/fs/cgroup/system.slice/atlas-crew-spawn.service")
        );
    }

    #[test]
    fn the_status_json_is_what_clients_read() {
        let on = serde_json::to_value(Status::on(3 * GIB)).unwrap();
        assert_eq!(on, serde_json::json!({"available": true, "bytes": 3 * GIB}));
        let off = serde_json::to_value(Status::off("macOS has no cgroups")).unwrap();
        assert_eq!(
            off,
            serde_json::json!({"available": false, "reason": "macOS has no cgroups"})
        );
    }

    // ---- the real kernel: run by the root CI job (docs/contributing.md) ----
    //
    // The cap only means something with a real cgroup, so these tests run the production start-up
    // (`Pool::detect`) in a real delegated unit: each outer test re-executes this test binary under
    // `systemd-run` with the helper's cgroup settings, and the matching `inside_*` test does the work
    // there. Outside root + systemd as PID 1 the outer tests print why and pass; the `inside_*` tests do
    // nothing unless the harness started them.

    /// `systemctl --version` first number (`systemd 255 (255.4-1ubuntu8)`), or `None` without systemd.
    #[cfg(target_os = "linux")]
    fn systemd_version() -> Option<u32> {
        let out = std::process::Command::new("systemctl")
            .arg("--version")
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    }

    /// Run `inside` (the name of an `inside_*` test) in a transient unit with the helper's cgroup
    /// settings and return what it printed. `Err(reason)` when this machine cannot (not root, no systemd);
    /// a failing probe panics with its output. Below systemd 254 `DelegateSubgroup=` is left out, which
    /// is the layout `Pool::detect` must handle by moving itself down.
    #[cfg(target_os = "linux")]
    fn in_delegated_unit(inside: &str) -> Result<String, String> {
        // SAFETY: plain getter.
        if unsafe { libc::geteuid() } != 0 || !Path::new("/run/systemd/system").is_dir() {
            return Err("needs root and systemd as PID 1".into());
        }
        let version = systemd_version().ok_or("no systemctl")?;
        if version < 236 {
            return Err(format!("systemd {version} has no Delegate=memory pids"));
        }
        let unit = format!("atlas-memcap-{inside}-{}", std::process::id());
        let mut cmd = std::process::Command::new("systemd-run");
        cmd.args(["--wait", "--collect", "--pipe", "--quiet"])
            .arg(format!("--unit={unit}"))
            .args([
                "-p",
                "Delegate=memory pids",
                "-p",
                "ProtectControlGroups=yes",
            ])
            .args([
                "-p",
                &format!(
                    "ReadWritePaths=-{}",
                    unit_cgroup_path(&format!("{unit}.service")).display()
                ),
            ]);
        if version >= 254 {
            cmd.args(["-p", &format!("DelegateSubgroup={HELPER_SUBGROUP}")]);
        }
        let out = cmd
            .args(["-E", &format!("ATLAS_MEMCAP_INSIDE={inside}")])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "--ignored", "--nocapture"])
            .arg(format!("crew::spawn::cgroup::tests::{inside}"))
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success() && text.contains("1 passed"),
            "{inside} failed inside its unit (systemd {version}):\n{text}"
        );
        Ok(text)
    }

    /// Whether this process was started by [`in_delegated_unit`] for the test `name`.
    #[cfg(target_os = "linux")]
    fn inside(name: &str) -> bool {
        std::env::var("ATLAS_MEMCAP_INSIDE").is_ok_and(|v| v == name)
    }

    // The unit text is only a promise until systemd runs it: with the cgroup settings of
    // `atlas-crew-spawn.service` the helper's start-up finds its delegated cgroup, enables the
    // controllers, makes a leaf with the limits, rewrites them when the cap changes, refuses a name that
    // is not a crewmate, and still cannot write the rest of the hierarchy.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs root and systemd as PID 1 (see docs/contributing.md)"]
    fn a_unit_with_the_helpers_settings_gives_leaves_with_their_limits() {
        if let Err(why) = in_delegated_unit("inside_leaf_limits") {
            eprintln!("skipped: {why}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run inside a systemd unit by a_unit_with_the_helpers_settings_gives_leaves_with_their_limits"]
    fn inside_leaf_limits() {
        if !inside("inside_leaf_limits") {
            return;
        }
        let pool = Pool::detect(256 << 20).expect("the unit's settings must give a usable cgroup");
        let user = "atlas-cm-3f2a9c1e5b7d";
        let leaf = pool.leaf(user).unwrap();
        let read = |f: &str| std::fs::read_to_string(leaf.dir.join(f)).unwrap();
        assert_eq!(read("memory.max").trim(), (256u64 << 20).to_string());
        assert_eq!(read("memory.oom.group").trim(), "1");
        assert_eq!(read("pids.max").trim(), PIDS_MAX.to_string());
        // Created once: the same leaf comes back.
        assert_eq!(pool.leaf(user).unwrap().dir, leaf.dir);
        // The setting changed: the leaf that exists is rewritten, not only new ones.
        pool.set_cap(128 << 20).unwrap();
        assert_eq!(read("memory.max").trim(), (128u64 << 20).to_string());
        // A name that is not a crewmate's user never becomes a path.
        assert!(pool.leaf("../../etc").is_err());
        // Only the unit's own subtree is writable: the hierarchy root stays read-only.
        assert!(
            std::fs::create_dir(Path::new(CGROUP_MOUNT).join("atlas-memcap-escape")).is_err(),
            "ProtectControlGroups=yes must keep the rest of the hierarchy read-only"
        );
    }

    // Incident (security review of the crew isolation, N3): the agent joined its leaf (memory cap, `pids.max`)
    // before `setuid`, but the helper's git request did not, so a hook of the crewmate's repo ran in the
    // helper's own cgroup, outside the crewmate's cap, and could get the helper itself killed for memory. Both now
    // become the crewmate through one function; a hook reads its own cgroup.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs root and systemd as PID 1 (see docs/contributing.md)"]
    fn a_git_request_runs_in_the_crewmates_leaf_under_its_cap() {
        if let Err(why) = in_delegated_unit("inside_git_leaf") {
            eprintln!("skipped: {why}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run inside a systemd unit by a_git_request_runs_in_the_crewmates_leaf_under_its_cap"]
    fn inside_git_leaf() {
        use super::super::exec::{Ids, lookup};
        use super::super::protocol::{GitReq, Response};
        use super::super::server::{Config, Memory};

        fn as_nobody(_: &str) -> std::io::Result<Ids> {
            lookup("nobody")
        }
        if !inside("inside_git_leaf") {
            return;
        }
        let pool = Pool::detect(256 << 20).expect("the unit's settings must give a usable cgroup");
        let user = "atlas-cm-aaaaaaaaaaaa";
        let leaf_dir = pool.leaf(user).unwrap().dir.clone();
        let nobody = lookup("nobody").expect("user nobody");
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("cm");
        let work = root.join("aaaaaaaaaaaa/work");
        std::fs::create_dir_all(&work).unwrap();
        let hook = t.path().join("hook.sh");
        std::fs::write(
            &hook,
            format!(
                "#!/bin/sh\ncat /proc/self/cgroup > {}/cg\nexit 1\n",
                work.display()
            ),
        )
        .unwrap();
        let run = |prog: &str, args: &[&str]| {
            let st = std::process::Command::new(prog)
                .args(args)
                .current_dir(&work)
                .status()
                .unwrap();
            assert!(st.success(), "{prog} {args:?}");
        };
        run("git", &["init", "-q"]);
        run(
            "git",
            &["config", "core.fsmonitor", &hook.to_string_lossy()],
        );
        run("chmod", &["755", &hook.to_string_lossy()]);
        let path = |p: &Path| p.to_string_lossy().into_owned();
        run(
            "chown",
            &[
                "-R",
                &format!("{}:{}", nobody.uid, nobody.gid),
                &path(&work),
            ],
        );
        run(
            "chmod",
            &[
                "755",
                &path(t.path()),
                &path(&root),
                &path(&root.join("aaaaaaaaaaaa")),
            ],
        );
        let cfg = Config {
            crew_uid: 0,
            cm_root: path(&root),
            engines: vec![],
            resolve: as_nobody,
            memory: std::sync::Arc::new(Memory::On(pool)),
            egress: || Ok(()),
        };
        let (resp, _) = super::super::git::run(
            &cfg,
            &GitReq {
                user: user.into(),
                args: vec!["status".into()],
                cwd: path(&work),
                token: None,
                timeout_secs: 30,
            },
        );
        assert!(matches!(resp, Response::Git { .. }), "{resp:?}");
        let cg = std::fs::read_to_string(work.join("cg")).expect("the hook never ran");
        let leaf = leaf_dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            cg.trim().ends_with(&format!("/{leaf}")),
            "the hook ran in {cg:?}, not in the crewmate's leaf {leaf}"
        );
    }

    /// A shell that allocates `mb` MiB and holds it: a variable made of that many bytes stays resident
    /// (ubuntu-latest has `head`, `tr` and `sh`; no compiler or python is assumed).
    #[cfg(target_os = "linux")]
    fn hog(mb: u64) -> String {
        format!(
            "x=$(head -c {} /dev/zero | tr '\\0' a); echo allocated; sleep 30",
            mb << 20
        )
    }

    // The point of #296: an agent that allocates past its cap is killed by the kernel, alone; its sibling
    // in another crewmate's leaf lives; and the leaf's `oom_kill` count shows it was a memory kill.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs root and systemd as PID 1 (see docs/contributing.md)"]
    fn an_agent_over_its_cap_is_oom_killed_alone_and_the_kill_is_visible() {
        if let Err(why) = in_delegated_unit("inside_oom_kill") {
            eprintln!("skipped: {why}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run inside a systemd unit by an_agent_over_its_cap_is_oom_killed_alone_and_the_kill_is_visible"]
    fn inside_oom_kill() {
        use super::super::exec::{Where, lookup, spawn_capped};
        use super::super::protocol::{Mode, SpawnReq};
        use std::io::Read;
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::ExitStatusExt;
        use std::time::{Duration, Instant};

        if !inside("inside_oom_kill") {
            return;
        }
        // The agents run as `nobody`, as root would drop them for a crewmate.
        let nobody = lookup("nobody").expect("user nobody");
        let pool = Pool::detect(64 << 20).expect("the unit's settings must give a usable cgroup");
        let t = tempfile::tempdir().unwrap();
        let (user_a, user_b) = ("atlas-cm-aaaaaaaaaaaa", "atlas-cm-bbbbbbbbbbbb");
        for slug in ["aaaaaaaaaaaa", "bbbbbbbbbbbb"] {
            std::fs::create_dir_all(t.path().join(format!("cm/{slug}/work"))).unwrap();
        }
        let engines = t.path().join("engines");
        std::fs::create_dir_all(&engines).unwrap();
        let bin = engines.join("claude");
        std::fs::write(&bin, "#!/bin/sh\neval \"$FAKE\"\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        // The temp tree is root's 0700: open it to `nobody`.
        assert!(
            std::process::Command::new("chmod")
                .args(["-R", "a+rwX"])
                .arg(t.path())
                .status()
                .unwrap()
                .success()
        );
        let engines_s = engines.to_string_lossy().into_owned();
        let root = t.path().join("cm").to_string_lossy().into_owned();
        let start = |user: &str, slug: &str, fake: &str| {
            // The child's PATH is exactly these directories (`exec::pinned_path`): the engine's, then the
            // system's, which is where `head`, `tr` and `sleep` of the hog live.
            let w = Where {
                engines: &[engines_s.as_str(), "/usr/bin", "/bin"],
                cm_root: &root,
            };
            let req = SpawnReq {
                user: user.into(),
                argv: vec!["claude".into()],
                env: [("FAKE".to_string(), fake.to_string())].into(),
                cwd: format!("{root}/{slug}/work"),
                mode: Mode::Pipe,
                cols: 80,
                rows: 24,
            };
            let leaf = pool.leaf(user).unwrap();
            let before = leaf.oom_kills();
            let s = spawn_capped(&w, &req, nobody, "/tmp", Some(leaf.procs_fd())).unwrap();
            (s, leaf, before)
        };
        let (mut sibling, sibling_leaf, _) = start(user_b, "bbbbbbbbbbbb", "echo alive; sleep 30");
        let (mut hogger, leaf, before) = start(user_a, "aaaaaaaaaaaa", &hog(256));

        let t0 = Instant::now();
        let status = loop {
            if let Some(st) = hogger.child.try_wait().unwrap() {
                break st;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(30),
                "the hog was never killed"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(status.signal(), Some(libc::SIGKILL), "{status:?}");
        assert!(
            leaf.oom_kills() > before,
            "memory.events oom_kill did not grow"
        );
        let mut out = String::new();
        std::fs::File::from(hogger.fd.try_clone().unwrap())
            .read_to_string(&mut out)
            .ok();
        assert!(
            !out.contains("allocated"),
            "it must die before it finishes allocating: {out:?}"
        );

        // The sibling, in its own leaf, was not touched and never saw an OOM kill.
        assert!(
            sibling.child.try_wait().unwrap().is_none(),
            "the sibling was killed too"
        );
        assert_eq!(sibling_leaf.oom_kills(), 0);
        sibling.child.kill().ok();
        sibling.child.wait().ok();
    }
}
