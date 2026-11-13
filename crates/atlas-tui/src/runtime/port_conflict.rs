//! A service's port held by something else: who.
//!
//! A start that finds its port taken names the holder (`port 4321 held by PID 12345 (node, cwd …):
//! stop it yourself (kill 12345) or change the service port`) instead of leaving the developer with
//! the framework's own "another dev server is already running". The holder is **own** when it is a
//! process of this project's previous run (the pids in `.logs/.pids` and their descendants) and
//! **foreign** otherwise (an agent's `bun dev`, another project, a container runtime's port
//! forwarder). Reporting is read-only: atlas never ends a process because of its port (it only
//! kills the exact pids it recorded itself, see `kill_orphans`).

use std::path::Path;

use atlas_core::sys::PortOwner;
use serde::{Deserialize, Serialize};

use super::process::{ProcEntry, descendants};

/// The foreign process on a service's port, as the status file, MCP and the wire carry it
/// (`port_conflict: {pid, cmd, cwd}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortConflict {
    pub pid: u32,
    pub cmd: String,
    pub cwd: Option<String>,
}

impl PortConflict {
    pub fn from_owner(o: &PortOwner) -> Self {
        Self {
            pid: o.pid,
            cmd: o.cmd.clone(),
            cwd: o.cwd.as_ref().map(|c| c.to_string_lossy().into_owned()),
        }
    }

    pub fn owner(&self) -> PortOwner {
        PortOwner {
            pid: self.pid,
            cmd: self.cmd.clone(),
            cwd: self.cwd.as_ref().map(Into::into),
        }
    }

    /// `port 4321 held by PID 12345 (node, cwd /work/app): stop it yourself (kill 12345) or change the service port`.
    pub fn describe(&self, port: u16) -> String {
        self.owner().describe(port)
    }
}

/// Who holds a port, relative to this atlas run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    /// A process of this project's previous run (or this run): `kill_orphans` owns it.
    Own(PortOwner),
    /// Anything else.
    Foreign(PortOwner),
}

/// Classify `owner`: own when its pid is one of `own_roots` (the `.pids` entries), a descendant of
/// one, or this very process.
pub fn classify(owner: PortOwner, own_roots: &[i32], table: &[ProcEntry]) -> Holder {
    let pid = owner.pid as i32;
    let own = pid == std::process::id() as i32
        || own_roots
            .iter()
            .any(|&r| r == pid || descendants(r, table).iter().any(|e| e.pid == pid));
    if own {
        Holder::Own(owner)
    } else {
        Holder::Foreign(owner)
    }
}

/// The pids recorded in `.logs/.pids` (empty when the file is missing).
pub fn recorded_pids(root: &Path) -> Vec<i32> {
    std::fs::read_to_string(super::pid_file(root))
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

/// Look up the holder of `port` and classify it against the project's `.pids`.
/// `None` when the port is free.
pub fn holder_of(root: &Path, port: u16) -> Option<Holder> {
    let owner = atlas_core::sys::port_owner(port)?;
    let roots = recorded_pids(root);
    let table = if roots.is_empty() {
        Vec::new()
    } else {
        super::process::process_table()
    };
    Some(classify(owner, &roots, &table))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn owner(pid: u32, cmd: &str) -> PortOwner {
        PortOwner {
            pid,
            cmd: cmd.into(),
            cwd: Some(PathBuf::from("/work/app")),
        }
    }

    fn row(pid: i32, ppid: i32) -> ProcEntry {
        ProcEntry {
            pid,
            ppid,
            pgid: pid,
        }
    }

    #[test]
    fn a_holder_in_our_pid_tree_is_own_and_anything_else_is_foreign() {
        // 100 (sh from .pids) -> 101 (turbo) -> 102 (vite); 900 is somebody else's.
        let table = [row(100, 1), row(101, 100), row(102, 101), row(900, 1)];
        let roots = [100];
        assert_eq!(
            classify(owner(100, "sh"), &roots, &table),
            Holder::Own(owner(100, "sh"))
        );
        assert_eq!(
            classify(owner(102, "node"), &roots, &table),
            Holder::Own(owner(102, "node"))
        );
        assert_eq!(
            classify(owner(900, "node"), &roots, &table),
            Holder::Foreign(owner(900, "node"))
        );
        // No .pids at all: nothing is ours.
        assert_eq!(
            classify(owner(100, "sh"), &[], &table),
            Holder::Foreign(owner(100, "sh"))
        );
    }

    #[test]
    fn this_process_is_always_own() {
        let me = owner(std::process::id(), "atlas");
        assert_eq!(classify(me.clone(), &[], &[]), Holder::Own(me));
    }

    #[test]
    fn port_conflict_json_has_exactly_pid_cmd_cwd() {
        let c = PortConflict::from_owner(&owner(12345, "node"));
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"pid": 12345, "cmd": "node", "cwd": "/work/app"})
        );
        assert_eq!(c.describe(4321), owner(12345, "node").describe(4321));
        // Round trip: what the status file wrote is what the reader parses back.
        let back: PortConflict = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn recorded_pids_skips_garbage_and_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(recorded_pids(dir.path()).is_empty());
        std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
        std::fs::write(super::super::pid_file(dir.path()), "12\nnope\n\n 34 \n").unwrap();
        assert_eq!(recorded_pids(dir.path()), vec![12, 34]);
    }
}
