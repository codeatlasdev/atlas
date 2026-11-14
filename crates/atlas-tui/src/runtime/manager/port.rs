//! Port-conflict detection at start: name the foreign process holding a service's port, in the
//! service's own log (disk + live buffer), the sidebar note, the status file and MCP.

use super::ServiceManager;
use crate::runtime::port_conflict::{Holder, PortConflict, holder_of};

impl ServiceManager {
    /// Look up who holds `port` right now and record it for `name`: a **foreign** holder becomes
    /// `port_conflict`; a free port or one of our own leftovers clears it. Never signals
    /// anything. The lookup shells out on macOS, so it runs off the async runtime. Writes no log
    /// line: [`announce_port_conflict`](Self::announce_port_conflict) does, once per spawn.
    pub(crate) async fn detect_port_conflict(&mut self, name: &str, port: u16) {
        let root = self.root_dir.clone();
        let holder = tokio::task::spawn_blocking(move || holder_of(&root, port))
            .await
            .ok()
            .flatten();
        let conflict = match holder {
            Some(Holder::Foreign(owner)) => Some(PortConflict::from_owner(&owner)),
            Some(Holder::Own(_)) | None => None,
        };
        self.set_port_conflict(name, conflict);
    }

    /// Store `conflict` for `name`; tell the subscribers when it changed.
    pub(crate) fn set_port_conflict(&mut self, name: &str, conflict: Option<PortConflict>) {
        let Some(svc) = self.services.get_mut(name) else {
            return;
        };
        if svc.sched.port_conflict == conflict {
            return;
        }
        svc.sched.port_conflict = conflict;
        let _ = self.event_tx.send(super::ManagerEvent::StatusChanged);
    }

    /// Write the stored conflict of `name` into its log (disk + live buffer), once per spawn. Called
    /// after the spawn's log writers are open: they rotate `<name>.log` to `.prev.log`, which
    /// would take a line written earlier with it.
    pub(crate) fn announce_port_conflict(&self, name: &str) {
        let Some(svc) = self.services.get(name) else {
            return;
        };
        let (Some(c), Some(port)) = (&svc.sched.port_conflict, svc.status.port) else {
            return;
        };
        let line = c.describe(port);
        self.append_log_mark(name, &line);
        let _ = self.event_tx.send(super::ManagerEvent::LogLine {
            name: name.to_string(),
            line,
            stream: crate::runtime::logs::LogStream::Stderr,
        });
    }

    /// Re-check the port of every service whose start found a foreign holder and is not healthy:
    /// the conflict ends when the holder is gone, so the note and `port_conflict` never outlive
    /// it. Bounded by the (few) conflicted services; each lookup is one read-only OS query.
    pub(crate) async fn refresh_port_conflicts(&mut self) {
        let stale: Vec<(String, u16)> = self
            .order
            .iter()
            .filter_map(|n| {
                let s = self.services.get(n)?;
                s.sched.port_conflict.as_ref()?;
                Some((n.clone(), s.status.port?))
            })
            .collect();
        for (name, port) in stale {
            self.detect_port_conflict(&name, port).await;
        }
    }
}
