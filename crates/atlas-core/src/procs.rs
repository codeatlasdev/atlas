//! The process table and (where the OS allows) a process's environment, the two things session
//! hygiene needs that `sys` (exact-pid probes and kills) does not give: who exists, who is whose
//! parent, when each started (the pair `(pid, start)` names a process for good; a bare pid is
//! reused), and, on Linux, which environment variable a process carries (the owner tag that
//! survives `setsid` and double-fork, where the parent chain does not).
//!
//! macOS reads `libproc` (`proc_listallpids`, `proc_pidinfo`); Linux reads
//! `/proc/<pid>/{stat,environ}`; Windows takes a Toolhelp snapshot. **Only Linux can read another
//! process's environment.** macOS with SIP returns argv but no environment for a process that is
//! not ours to debug (verified on macOS 27, same user, `KERN_PROCARGS2` and `ps -E`), and Windows
//! has no way to. There the owner records who runs under its roots while it lives
//! (`hygiene::Registry::track`) and the tag is not consulted. Nothing here signals a process.

use std::collections::{HashMap, HashSet};

/// One row of the process table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Process start as Unix seconds (0 when the OS would not say).
    pub start: u64,
    /// Short executable name (`node`, `chrome`, `Google Chrome Helper`).
    pub name: String,
}

/// Snapshot of every process this user can see. Empty when the OS refuses.
pub fn table() -> Vec<Proc> {
    imp::table()
}

/// Start time of `pid` as Unix seconds, `None` when it is gone or unreadable.
pub fn start_time(pid: u32) -> Option<u64> {
    imp::start_time(pid)
}

/// Whether another process's environment can be read here at all (Linux only, see the module doc):
/// elsewhere the tag scans return nothing at once instead of failing once per process.
pub const ENV_READABLE: bool = cfg!(target_os = "linux");

/// The value of environment variable `key` in `pid`'s environment as it was at `exec`. `None`
/// when the process is gone, belongs to someone else, does not carry the variable, or the
/// platform will not say (**always on Windows, and on macOS for a process that is not our own
/// child under SIP**: do not build discovery on it there).
pub fn env_var(pid: u32, key: &str) -> Option<String> {
    imp::environ(pid).and_then(|e| find_var(&e, key))
}

/// The value of `key` in a raw environment block: `KEY=value` entries separated by NUL.
pub fn find_var(block: &[u8], key: &str) -> Option<String> {
    let want = key.as_bytes();
    block.split(|&b| b == 0).find_map(|kv| {
        let rest = kv.strip_prefix(want)?.strip_prefix(b"=")?;
        Some(String::from_utf8_lossy(rest).into_owned())
    })
}

/// Every live process whose environment says `key=value`. The parent chain is NOT consulted: a
/// daemonized grandchild (`setsid`, double fork) reparents to init and only the tag still names
/// its owner. Never includes this process.
pub fn tagged(key: &str, value: &str) -> Vec<Proc> {
    if !ENV_READABLE {
        return Vec::new();
    }
    let me = std::process::id();
    table()
        .into_iter()
        .filter(|p| p.pid != me && p.pid > 1)
        .filter(|p| env_var(p.pid, key).as_deref() == Some(value))
        .collect()
}

/// Every live process carrying `key`, with its value: the whole tagged population in one pass
/// (what `doctor --hygiene` and the periodic sweep group by owner). Never includes this process.
pub fn tagged_any(key: &str) -> Vec<(Proc, String)> {
    if !ENV_READABLE {
        return Vec::new();
    }
    let me = std::process::id();
    table()
        .into_iter()
        .filter(|p| p.pid != me && p.pid > 1)
        .filter_map(|p| {
            let v = env_var(p.pid, key)?;
            Some((p, v))
        })
        .collect()
}

/// `root` and every process below it by parent links, parents before children. A child counts
/// only when it started no earlier than its parent (a recorded ppid outlives its parent and pids
/// are reused). Never includes pid 0/1 or this process.
pub fn tree(root: u32, rows: &[Proc]) -> Vec<u32> {
    let me = std::process::id();
    let start: HashMap<u32, u64> = rows.iter().map(|r| (r.pid, r.start)).collect();
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for r in rows {
        kids.entry(r.ppid).or_default().push(r.pid);
    }
    let mut seen: HashSet<u32> = HashSet::from([root]);
    let mut out = Vec::new();
    if start.contains_key(&root) && root > 1 && root != me {
        out.push(root);
    }
    let mut queue = std::collections::VecDeque::from([root]);
    while let Some(p) = queue.pop_front() {
        let born = start.get(&p).copied().unwrap_or(0);
        for &c in kids.get(&p).into_iter().flatten() {
            if c > 1 && c != me && seen.insert(c) && start.get(&c).copied().unwrap_or(0) >= born {
                out.push(c);
                queue.push_back(c);
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Proc;

    pub fn table() -> Vec<Proc> {
        // SAFETY: a null buffer asks for the size only.
        let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if n <= 0 {
            return Vec::new();
        }
        // Room for processes born between the two calls.
        let mut pids = vec![0 as libc::pid_t; n as usize + 64];
        // SAFETY: the buffer is valid for the byte length passed.
        let got = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int,
            )
        };
        if got <= 0 {
            return Vec::new();
        }
        pids.truncate(got as usize);
        pids.into_iter()
            .filter(|&p| p > 0)
            .filter_map(|p| info(p as u32))
            .collect()
    }

    fn info(pid: u32) -> Option<Proc> {
        // SAFETY: zeroed plain-old-data, filled by proc_pidinfo.
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
        if ret != size {
            return None;
        }
        let name = i
            .pbi_name
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect::<Vec<u8>>();
        let name = if name.is_empty() {
            i.pbi_comm
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect()
        } else {
            name
        };
        Some(Proc {
            pid,
            ppid: i.pbi_ppid,
            start: i.pbi_start_tvsec,
            name: String::from_utf8_lossy(&name).into_owned(),
        })
    }

    pub fn start_time(pid: u32) -> Option<u64> {
        info(pid).map(|p| p.start)
    }

    /// `KERN_PROCARGS2`: `argc`, the executable path, NULs, argv, then the environment block.
    pub fn environ(pid: u32) -> Option<Vec<u8>> {
        let mut max: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>();
        let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
        // SAFETY: valid mib and out pointers of the stated sizes.
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                (&mut max as *mut libc::c_int).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
            || max <= 0
        {
            return None;
        }
        let mut buf = vec![0u8; max as usize];
        let mut len = buf.len();
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
        // SAFETY: `buf` is valid for `len` bytes.
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                buf.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        buf.truncate(len);
        parse_procargs2(&buf)
    }

    /// The environment block of a `KERN_PROCARGS2` buffer.
    pub(super) fn parse_procargs2(buf: &[u8]) -> Option<Vec<u8>> {
        let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
        let mut i = 4;
        // The executable path, then its NUL padding.
        while *buf.get(i)? != 0 {
            i += 1;
        }
        while buf.get(i) == Some(&0) {
            i += 1;
        }
        // argc arguments, each NUL terminated.
        for _ in 0..argc.max(0) {
            while *buf.get(i)? != 0 {
                i += 1;
            }
            i += 1;
        }
        buf.get(i..).map(<[u8]>::to_vec)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Proc;

    pub fn table() -> Vec<Proc> {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
            .filter_map(stat)
            .collect()
    }

    /// `/proc/<pid>/stat`: `pid (comm) state ppid ... starttime(22)`.
    fn stat(pid: u32) -> Option<Proc> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // comm may hold spaces and parentheses: the fields resume after the LAST ')'.
        let (head, rest) = s.rsplit_once(')')?;
        let name = head.split_once('(')?.1.to_string();
        let f: Vec<&str> = rest.split_whitespace().collect();
        let ppid: u32 = f.get(1)?.parse().ok()?;
        let ticks: u64 = f.get(19)?.parse().ok()?;
        Some(Proc {
            pid,
            ppid,
            start: boot_time()? + ticks / clk_tck(),
            name,
        })
    }

    fn clk_tck() -> u64 {
        // SAFETY: sysconf has no preconditions.
        (unsafe { libc::sysconf(libc::_SC_CLK_TCK) }).max(1) as u64
    }

    fn boot_time() -> Option<u64> {
        std::fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())
    }

    pub fn start_time(pid: u32) -> Option<u64> {
        stat(pid).map(|p| p.start)
    }

    pub fn environ(pid: u32) -> Option<Vec<u8>> {
        std::fs::read(format!("/proc/{pid}/environ")).ok()
    }
}

#[cfg(windows)]
mod imp {
    use super::Proc;

    pub fn table() -> Vec<Proc> {
        crate::sys::process_rows()
            .into_iter()
            .map(|r| Proc {
                start: crate::sys::start_time_secs(r.pid).unwrap_or(0),
                pid: r.pid,
                ppid: r.ppid,
                name: r.exe,
            })
            .collect()
    }

    pub fn start_time(pid: u32) -> Option<u64> {
        crate::sys::start_time_secs(pid)
    }

    /// Windows cannot read another process's environment without a debugger-grade handle:
    /// its trees are found by parent chain and the job object, not by tag.
    pub fn environ(_pid: u32) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_var_matches_the_whole_key_and_keeps_equals_signs_in_the_value() {
        let block = b"A=1\0ATLAS_SESSION_X=no\0ATLAS_SESSION=s=1\0EMPTY=\0";
        assert_eq!(find_var(block, "ATLAS_SESSION").as_deref(), Some("s=1"));
        assert_eq!(find_var(block, "A").as_deref(), Some("1"));
        assert_eq!(find_var(block, "EMPTY").as_deref(), Some(""));
        assert_eq!(find_var(block, "ATLAS"), None);
        assert_eq!(find_var(b"", "A"), None);
    }

    #[test]
    fn our_own_row_has_our_parent_a_start_time_and_a_name() {
        let me = std::process::id();
        let rows = table();
        let row = rows.iter().find(|r| r.pid == me).expect("own row");
        assert_eq!(row.ppid, crate::sys::parent_pid());
        assert!(row.start > 1_600_000_000, "start {}", row.start);
        assert!(!row.name.is_empty());
        assert_eq!(start_time(me), Some(row.start));
    }

    #[cfg(unix)]
    // Linux only: macOS (SIP) returns argv but no environment for another process, even the same
    // user's, and Windows has no way to read it. Discovery there rests on recorded membership
    // (`hygiene::Registry::track`), which `hygiene/tests.rs` covers.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_environment_is_read_back_from_another_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .env("ATLAS_PROCS_TEST", "v a=l")
            .spawn()
            .unwrap();
        let pid = child.id();
        // exec has happened once the table shows the child as `sleep`.
        let mut got = None;
        for _ in 0..100 {
            got = env_var(pid, "ATLAS_PROCS_TEST");
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(got.as_deref(), Some("v a=l"));
        assert_eq!(env_var(pid, "ATLAS_PROCS_TEST"), None, "gone process");
    }

    #[test]
    fn tree_follows_parents_and_ignores_a_child_older_than_its_parent() {
        let p = |pid, ppid, start| Proc {
            pid,
            ppid,
            start,
            name: String::new(),
        };
        let rows = vec![
            p(100, 1, 50),
            p(200, 100, 60),
            p(300, 200, 70),
            // Same ppid as a live parent but born before it: a reused parent pid, not a child.
            p(400, 100, 10),
            p(500, 999, 80),
        ];
        assert_eq!(tree(100, &rows), vec![100, 200, 300]);
        assert_eq!(tree(200, &rows), vec![200, 300]);
        assert!(tree(7777, &rows).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_skips_argc_exe_and_argv_to_reach_the_environment() {
        let mut b = 2i32.to_ne_bytes().to_vec();
        b.extend(b"/bin/x\0\0\0");
        b.extend(b"x\0-a\0");
        b.extend(b"K=V\0ATLAS_SESSION=s1\0");
        let env = imp::parse_procargs2(&b).unwrap();
        assert_eq!(find_var(&env, "ATLAS_SESSION").as_deref(), Some("s1"));
        // argv words are not mistaken for variables.
        assert_eq!(find_var(&env, "x"), None);
        assert!(imp::parse_procargs2(&b[..6]).is_none());
    }
}
