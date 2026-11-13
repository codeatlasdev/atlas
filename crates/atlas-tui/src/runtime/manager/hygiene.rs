//! Owner tags for what `atlas dev` starts. Every service and action process carries
//! `ATLAS_SESSION=<run>:<service>` (inherited by its whole tree, even a grandchild that
//! daemonizes: vite's workers, a browser a test run opened), and its root is registered with
//! this run as owner. A stop reaps the tag after the process-tree kill; a crash of `atlas dev`
//! leaves the registry behind, and the next sweep ends what carries the dead owner's tag.

use super::ServiceManager;

/// How often the descendants of this run's roots are recorded: a process must survive this long
/// under its root before it can daemonize away unseen (macOS and Windows, see
/// `atlas_core::hygiene`).
pub(super) const TRACK_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
use atlas_core::hygiene::{self, Kind, Owner, Root};

impl ServiceManager {
    /// The tag of `name`'s processes in this run.
    pub(crate) fn session_tag(&self, name: &str) -> String {
        format!("{}:{name}", self.hygiene_run)
    }

    /// Make `cmd` start inside `name`'s session.
    pub(crate) fn tag_spawn(&self, cmd: &mut tokio::process::Command, name: &str) {
        cmd.env(hygiene::SESSION_ENV, self.session_tag(name));
    }

    /// Record the root process just spawned for `name`: a crash of this process leaves the record,
    /// and the next sweep reaps the tree. Best effort: a full disk must not stop a service.
    pub(crate) fn register_root(&self, name: &str, pid: u32, kind: Kind) {
        let root = Root {
            session: self.session_tag(name),
            owner: Owner::current(),
            pid,
            start: atlas_core::procs::start_time(pid).unwrap_or(0),
            kind,
            label: name.to_string(),
            project: Some(self.root_dir.to_string_lossy().into_owned()),
            started_at: crate::runtime::clock::now_secs(),
        };
        let _ = self.hygiene.add_root(root);
        // The root's first children (a shell's `sleep`, a dev server's workers) are recorded at
        // once; a later snapshot catches the rest before they can daemonize away.
        let _ = self.hygiene.track();
    }

    /// Point this run at another registry (tests; `atlas dev` uses the user's).
    pub fn set_hygiene_registry(&mut self, registry: hygiene::Registry) {
        self.hygiene = registry;
    }

    /// End everything still belonging to `names` (a service stop, or the whole run on quit): the
    /// grandchildren the process-tree kill could not see because they left the parent chain.
    /// Runs the process-table scan and the short grace off the async runtime.
    pub(crate) async fn reap_tags(&self, names: &[String]) {
        let tags: Vec<String> = names.iter().map(|n| self.session_tag(n)).collect();
        let reg = self.hygiene.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for tag in &tags {
                reg.reap_session(tag, std::time::Duration::from_secs(2));
            }
        })
        .await;
    }

    /// Record the descendants of every live root of this run (daemonized ones included, once
    /// seen), see `atlas_core::hygiene`. Off the async runtime: one process-table read.
    pub(crate) fn track_members(&self) {
        let reg = self.hygiene.clone();
        tokio::task::spawn_blocking(move || {
            let _ = reg.track();
        });
    }
}
