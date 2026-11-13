//! Who holds a TCP port: the listening process behind a service's port, read-only.
//!
//! macOS asks `lsof` (listeners only, `-sTCP:LISTEN`); Linux reads `/proc/net/tcp{,6}` and the
//! `/proc/<pid>/fd` links; Windows asks `GetExtendedTcpTable`. Nothing here ever signals a
//! process: callers only report the answer: atlas never ends a process because of its port.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The process listening on a port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortOwner {
    pub pid: u32,
    /// Short command name as the OS reports it (`node`, `bun`, `OrbStack Helper`).
    pub cmd: String,
    /// Working directory of the process, when readable.
    pub cwd: Option<PathBuf>,
}

impl PortOwner {
    /// The command that stops this process by hand: `kill 12345` (`taskkill /PID 12345 /F` on
    /// Windows). Advice for the user to run; atlas never ends a process because of its port.
    pub fn stop_hint(&self) -> String {
        if cfg!(windows) {
            format!("taskkill /PID {} /F", self.pid)
        } else {
            format!("kill {}", self.pid)
        }
    }

    /// `port 4321 held by PID 12345 (node, cwd /work/app): stop it yourself (kill 12345) or change
    /// the service port`.
    pub fn describe(&self, port: u16) -> String {
        let cwd = self
            .cwd
            .as_ref()
            .map(|c| format!(", cwd {}", c.display()))
            .unwrap_or_default();
        format!(
            "port {port} held by PID {} ({}{cwd}): stop it yourself ({}) or change the service port",
            self.pid,
            if self.cmd.is_empty() { "?" } else { &self.cmd },
            self.stop_hint()
        )
    }
}

/// The process listening on TCP `port` on any local address (the first one when several
/// sockets, e.g. IPv4 and IPv6, listen). `None` when nobody listens or the OS will not say.
pub fn port_owner(port: u16) -> Option<PortOwner> {
    imp::port_owner(port)
}

/// Parse `lsof -Fpc` output: one `p<pid>` line opens a process, `c<cmd>` names it, other field
/// lines (`f<fd>`, `n<name>`) are ignored. Processes come back in output order, deduplicated
/// (a process listening on IPv4 and IPv6 appears once).
pub fn parse_lsof_listeners(out: &str) -> Vec<(u32, String)> {
    let mut found: Vec<(u32, String)> = Vec::new();
    for line in out.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            if let Ok(pid) = pid.trim().parse::<u32>()
                && !found.iter().any(|(p, _)| *p == pid)
            {
                found.push((pid, String::new()));
            }
        } else if let Some(cmd) = line.strip_prefix('c')
            && let Some(last) = found.last_mut()
            && last.1.is_empty()
        {
            last.1 = cmd.to_string();
        }
    }
    found
}

/// Parse the `n<path>` line of `lsof -Fn` output (the cwd of one process).
pub fn parse_lsof_cwd(out: &str) -> Option<PathBuf> {
    out.lines()
        .find_map(|l| l.strip_prefix('n'))
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

/// Inodes of the sockets listening on `port` in a `/proc/net/tcp` or `/proc/net/tcp6` table.
/// A row is `sl local_address rem_address st … inode`; state `0A` is LISTEN and the local
/// address ends in `:<port as 4 hex digits>`.
pub fn parse_proc_net_tcp(table: &str, port: u16) -> Vec<u64> {
    table
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let local = f.get(1)?;
            let state = f.get(3)?;
            let p = u16::from_str_radix(local.rsplit_once(':')?.1, 16).ok()?;
            if *state != "0A" || p != port {
                return None;
            }
            f.get(9)?.parse::<u64>().ok().filter(|&inode| inode != 0)
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::process::Command;

    pub fn port_owner(port: u16) -> Option<PortOwner> {
        let out = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"])
            .output()
            .ok()?;
        let (pid, cmd) = parse_lsof_listeners(&String::from_utf8_lossy(&out.stdout))
            .into_iter()
            .next()?;
        let cwd = Command::new("lsof")
            .args(["-nP", "-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .ok()
            .and_then(|o| parse_lsof_cwd(&String::from_utf8_lossy(&o.stdout)));
        Some(PortOwner { pid, cmd, cwd })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;

    pub fn port_owner(port: u16) -> Option<PortOwner> {
        let inodes: Vec<u64> = ["/proc/net/tcp", "/proc/net/tcp6"]
            .iter()
            .filter_map(|t| std::fs::read_to_string(t).ok())
            .flat_map(|t| parse_proc_net_tcp(&t, port))
            .collect();
        if inodes.is_empty() {
            return None;
        }
        let wanted: Vec<String> = inodes.iter().map(|i| format!("socket:[{i}]")).collect();
        for entry in std::fs::read_dir("/proc").ok()?.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
                continue; // another user's process: unreadable, not ours to name
            };
            let holds = fds.flatten().any(|fd| {
                std::fs::read_link(fd.path())
                    .ok()
                    .and_then(|l| l.to_str().map(|s| wanted.iter().any(|w| w == s)))
                    .unwrap_or(false)
            });
            if holds {
                let cmd = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
                return Some(PortOwner { pid, cmd, cwd });
            }
        }
        None
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };

    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;

    /// Owning pids of the listeners on `port` in one family's table.
    fn listener_pids(family: u32, port: u16) -> Vec<u32> {
        let mut size = 0u32;
        // SAFETY: a null table with size 0 only asks for the needed size.
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if size == 0 {
            return Vec::new();
        }
        // u64 storage keeps the table header aligned.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: buf holds `size` bytes.
        let rc = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if rc != 0 {
            return Vec::new();
        }
        // The port is network byte order in the low 16 bits of dwLocalPort.
        let matches = |raw: u32| u16::from_be((raw & 0xFFFF) as u16) == port;
        let mut pids = Vec::new();
        if family == AF_INET {
            // SAFETY: on success the buffer starts with a MIB_TCPTABLE_OWNER_PID whose
            // `dwNumEntries` rows follow the header inside `buf`.
            let t = unsafe { &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
            let rows = unsafe {
                std::slice::from_raw_parts(
                    t.table.as_ptr(),
                    t.dwNumEntries as usize,
                )
            };
            pids.extend(
                rows.iter()
                    .filter(|r| matches(r.dwLocalPort))
                    .map(|r| r.dwOwningPid),
            );
        } else {
            // SAFETY: as above, for the IPv6 table.
            let t = unsafe { &*(buf.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID) };
            let rows = unsafe {
                std::slice::from_raw_parts(
                    t.table.as_ptr(),
                    t.dwNumEntries as usize,
                )
            };
            pids.extend(
                rows.iter()
                    .filter(|r| matches(r.dwLocalPort))
                    .map(|r| r.dwOwningPid),
            );
        }
        pids
    }

    pub fn port_owner(port: u16) -> Option<PortOwner> {
        let pid = [AF_INET, AF_INET6]
            .into_iter()
            .flat_map(|f| listener_pids(f, port))
            .find(|&p| p != 0)?;
        let cmd = crate::sys::exe_path(pid)
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        // A foreign process's cwd needs reading its PEB; the exe's folder is what we can name.
        Some(PortOwner {
            pid,
            cmd,
            cwd: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // lsof's real field output for a process holding IPv4 and IPv6 listeners.
    const LSOF: &str = "p85746\ncPython\nf3\nf4\np99\ncnode\nf12\n";

    #[test]
    fn lsof_listeners_are_deduplicated_and_keep_pid_and_command_paired() {
        assert_eq!(
            parse_lsof_listeners(LSOF),
            vec![(85746, "Python".to_string()), (99, "node".to_string())]
        );
        // The same process listening twice (v4 + v6) is one owner.
        assert_eq!(
            parse_lsof_listeners("p7\ncbun\nf3\np7\ncbun\nf4\n"),
            vec![(7, "bun".to_string())]
        );
        assert!(parse_lsof_listeners("").is_empty());
        assert!(parse_lsof_listeners("pnotanumber\ncx\n").is_empty());
    }

    #[test]
    fn lsof_cwd_is_the_n_line() {
        assert_eq!(
            parse_lsof_cwd("p85746\nfcwd\nn/private/tmp/my app\n"),
            Some(PathBuf::from("/private/tmp/my app"))
        );
        assert_eq!(parse_lsof_cwd("p1\nfcwd\n"), None);
    }

    #[test]
    fn proc_net_tcp_matches_only_listening_rows_on_the_port() {
        // 0x10E1 = 4321. Row 1 listens, row 2 is an ESTABLISHED client of the same port,
        // row 3 listens on another port, row 4 is a v4-mapped listener.
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:10E1 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5551 1 0000 100 0 0 10 0\n\
   1: 0100007F:10E1 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 5552 1 0000 100 0 0 10 0\n\
   2: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5553 1 0000 100 0 0 10 0\n\
   3: 00000000000000000000000001000000:10E1 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5554 1 0000 100 0 0 10 0\n";
        assert_eq!(parse_proc_net_tcp(table, 4321), vec![5551, 5554]);
        assert!(parse_proc_net_tcp(table, 9999).is_empty());
        assert!(parse_proc_net_tcp("", 4321).is_empty());
    }

    #[test]
    fn describe_names_pid_command_and_cwd() {
        let o = PortOwner {
            pid: 12345,
            cmd: "node".into(),
            cwd: Some(PathBuf::from("/work/app")),
        };
        assert_eq!(
            o.describe(4321),
            format!(
                "port 4321 held by PID 12345 (node, cwd /work/app): stop it yourself ({}) or change the service port",
                if cfg!(windows) {
                    "taskkill /PID 12345 /F"
                } else {
                    "kill 12345"
                }
            )
        );
        let o = PortOwner {
            pid: 1,
            cmd: String::new(),
            cwd: None,
        };
        assert!(
            o.describe(80)
                .starts_with("port 80 held by PID 1 (?): stop it yourself (")
        );
    }

    // The real thing, no mocks: this very process listens, the OS must name us.
    #[test]
    fn a_real_listener_in_this_process_is_found_with_our_pid() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let o = port_owner(port).expect("listener not found");
        assert_eq!(o.pid, std::process::id());
        assert!(!o.cmd.is_empty());
        drop(l);
        assert_eq!(port_owner(port), None, "a closed port has no owner");
    }

    #[test]
    fn an_ipv6_only_listener_is_found_too() {
        let Ok(l) = std::net::TcpListener::bind("[::1]:0") else {
            return; // no IPv6 loopback on this host
        };
        let port = l.local_addr().unwrap().port();
        assert_eq!(port_owner(port).map(|o| o.pid), Some(std::process::id()));
    }
}
