//! What a task needs to know about isolation (issue #200), asked once per task: does this crewmate run
//! as its own Linux user, and so where do its files, its hook endpoint and its processes live?
//!
//! Without a helper (`spawn::layout::active()` false) every answer is the one from before: the crewmate's
//! store directory, the global hook endpoint, a process of the crew user. [`Ctx`] is the only place that
//! decides, so the runners cannot disagree about it.

use std::path::{Path, PathBuf};

use crate::crew::engine::Exec;
use crate::crew::project;
#[cfg(unix)]
use crate::crew::spawn::client::Client;
use crate::crew::spawn::layout::{self, Isolated};
use crate::server::Hub;

/// How one crewmate runs.
pub(crate) struct Ctx {
    #[cfg(unix)]
    iso: Option<(Client, Isolated)>,
}

impl Ctx {
    pub(crate) fn of(hub: &Hub, crewmate_id: &str) -> Self {
        Self::from_store(&hub.crew.store, crewmate_id)
    }

    #[cfg(unix)]
    fn from_store(store: &crate::crew::store::CrewStore, crewmate_id: &str) -> Self {
        Self {
            iso: store
                .isolation()
                .zip(store.isolated(crewmate_id))
                .map(|(setup, iso)| (Client::at(&setup.socket), iso)),
        }
    }

    /// No helper exists off Unix.
    #[cfg(not(unix))]
    fn from_store(_store: &crate::crew::store::CrewStore, _crewmate_id: &str) -> Self {
        Self {}
    }

    pub(crate) fn isolated(&self) -> Option<&Isolated> {
        #[cfg(unix)]
        {
            self.iso.as_ref().map(|(_, i)| i)
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Where engine processes run.
    pub(crate) fn exec(&self) -> Exec {
        #[cfg(unix)]
        if let Some((client, iso)) = &self.iso {
            return Exec::isolated(client.clone(), iso.user.clone());
        }
        Exec::local()
    }

    /// The helper and user to start a terminal agent as, when isolated.
    #[cfg(unix)]
    pub(crate) fn as_user(&self) -> Option<(&Client, &str)> {
        self.iso.as_ref().map(|(c, i)| (c, i.user.as_str()))
    }

    /// Where the agent-readable files go: the crewmate's store directory, or `crew-runtime/`.
    pub(crate) fn runtime_dir(&self, hub: &Hub, id: &str) -> PathBuf {
        match self.isolated() {
            Some(iso) => iso.runtime(),
            None => hub.crew.store.crewmate_dir(id),
        }
    }

    /// The crewmate's checkout (or its own folder).
    pub(crate) fn work_dir(&self, hub: &Hub, id: &str) -> String {
        project::work_dir_for(&hub.crew.dir, id, self.isolated())
            .to_string_lossy()
            .into_owned()
    }

    /// `(port, token)` the crewmate's agent reaches hostd with: the global endpoint, or, isolated, this
    /// crewmate's own token (which opens only its routes).
    pub(crate) fn hook_endpoint(&self, hub: &Hub, id: &str) -> (u16, String) {
        let (port, global) = super::prompt::load_hook_endpoint(&hub.dir);
        match self.isolated() {
            Some(_) => match hub.crew.store.ensure_hook_token(id) {
                Ok(t) => (port, t),
                // No token, no endpoint: better a refused hook than the global token in an
                // isolated agent's environment.
                Err(e) => {
                    eprintln!("atlas crew: crewmate {id}: cannot create its hook token: {e}");
                    (port, String::new())
                }
            },
            None => (port, global),
        }
    }

    /// Write what an isolated agent reads from `crew-runtime/` (the hook endpoint file `atlas hook`
    /// follows through `ATLAS_HOOK_ENDPOINT`). A no-op when not isolated. Group-readable and not
    /// writable: the crewmate is in the directory's group only.
    pub(crate) fn prepare(&self, hub: &Hub, id: &str) -> std::io::Result<()> {
        let Some(iso) = self.isolated() else {
            return Ok(());
        };
        // A crewmate imported from an archive, or made before the helper existed, has no user yet:
        // the first task creates it (idempotent, so a crewmate that has one costs one quick call).
        #[cfg(unix)]
        if let Some(setup) = hub.crew.store.isolation() {
            crate::crew::spawn::provision::ProvClient::at(&setup.provision_socket)
                .provision(&iso.user)
                .map_err(|e| {
                    std::io::Error::other(format!("creating the Linux user {}: {e}", iso.user))
                })?;
        }
        adopt_memory(
            &hub.crew.store.crewmate_dir(id).join("memory.md"),
            &iso.memory(),
        )?;
        let (port, token) = self.hook_endpoint(hub, id);
        write_readable(
            &iso.hook_endpoint(),
            format!("ATLAS_HOOK_PORT={port}\nATLAS_HOOK_TOKEN={token}\n").as_bytes(),
        )
    }

    /// Environment a terminal agent needs on top of the request's own: where its hook endpoint file is
    /// and its token. Empty when not isolated.
    pub(crate) fn agent_env(&self, hub: &Hub, id: &str) -> Vec<(String, String)> {
        let Some(iso) = self.isolated() else {
            return vec![];
        };
        let (port, token) = self.hook_endpoint(hub, id);
        vec![
            (
                "ATLAS_HOOK_ENDPOINT".to_string(),
                iso.hook_endpoint().to_string_lossy().into_owned(),
            ),
            ("ATLAS_HOOK_PORT".to_string(), port.to_string()),
            ("ATLAS_HOOK_TOKEN".to_string(), token),
        ]
    }
}

/// Move a crewmate's memory from the crew home (where it lived before isolation) into its own
/// directory, once: only when the new file is empty and the old one has content. The old file is kept
/// as `memory.md.pre-isolation`, never deleted.
fn adopt_memory(old: &Path, new: &Path) -> std::io::Result<()> {
    // Neither read nor written through a link: `new` is in the crewmate's directory.
    let have = layout::read_nofollow(new).unwrap_or_default();
    // tree-access: reads `crew-home/memory.md` (hostd's); the crewmate's file is read and written with the `*_nofollow` helpers
    let before = match std::fs::read_to_string(old) {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return Ok(()),
    };
    if !have.trim().is_empty() {
        return Ok(());
    }
    // In place: the file is the crewmate's (group-writable), not replaced by hostd's.
    layout::write_nofollow(new, before.as_bytes())?;
    // tree-access: renames hostd's own old memory file inside the crew home
    std::fs::rename(old, old.with_extension("md.pre-isolation"))
}

/// Replace `path` with `data`, mode 0640 (the crewmate reads it through the group, nobody else does).
/// Atomic, so an agent never reads half a file.
#[cfg(unix)]
fn write_readable(path: &Path, data: &[u8]) -> std::io::Result<()> {
    atlas_core::fs::write_atomic(path, data, atlas_core::fs::Mode::Exact(0o640))
}

/// Nothing is isolated off Unix, so nothing is written.
#[cfg(not(unix))]
fn write_readable(_path: &Path, _data: &[u8]) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // The agent reads these through the directory's group and must not be able to rewrite them: 0640,
    // replaced atomically, and rewritten in full on every task (the hook port changes with hostd).
    #[test]
    fn runtime_files_are_group_readable_never_world_readable_and_replaced_whole() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("hook-endpoint.env");
        write_readable(&p, b"one").unwrap();
        write_readable(&p, b"two-longer").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two-longer");
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    // Memory written before isolation is not lost, not overwritten, and not moved twice.
    #[test]
    fn memory_moves_into_the_crewmates_directory_once_and_never_overwrites() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old.md"), t.path().join("new.md"));
        std::fs::write(&new, "").unwrap();
        // Nothing to move.
        adopt_memory(&old, &new).unwrap();
        std::fs::write(&old, "milk").unwrap();
        adopt_memory(&old, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "milk");
        assert_eq!(
            std::fs::read_to_string(t.path().join("old.md.pre-isolation")).unwrap(),
            "milk"
        );
        assert!(!old.exists());
        // The agent wrote to the new file since: a leftover old file must not clobber it.
        std::fs::write(&old, "stale").unwrap();
        std::fs::write(&new, "fresh").unwrap();
        adopt_memory(&old, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "fresh");
        assert!(old.exists(), "left alone when the new file has content");
    }

    #[test]
    fn without_a_helper_a_crewmate_runs_where_it_always_did() {
        let t = tempfile::tempdir().unwrap();
        let store = crate::crew::store::CrewStore::new(t.path());
        let c = Ctx::from_store(&store, "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b");
        assert!(c.isolated().is_none() && c.as_user().is_none());
        assert!(!c.exec().is_isolated());
    }

    #[test]
    fn with_a_helper_the_crewmate_runs_as_its_user_and_a_non_crewmate_id_does_not() {
        let t = tempfile::tempdir().unwrap();
        let setup = layout::Setup {
            socket: t.path().join("s.sock"),
            provision_socket: t.path().join("p.sock"),
            root: t.path().join("cm").to_string_lossy().into_owned(),
            hook_port: 0,
        };
        let store = crate::crew::store::CrewStore::new(t.path()).with_isolation(Some(setup));
        let c = Ctx::from_store(&store, "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b");
        assert_eq!(c.isolated().unwrap().user, "atlas-cm-3f2a9c1e5b7d");
        assert!(c.exec().is_isolated());
        // Test crewmates with short ids (and any id that is not a UUID) stay unisolated.
        let plain = Ctx::from_store(&store, "crewmate-a");
        assert!(plain.isolated().is_none() && !plain.exec().is_isolated());
    }

    // Regression (security review of #448, B2): the tree of an isolated crewmate is the crewmate's, so git
    // there runs as the crewmate through the helper; running it as hostd would run the crewmate's
    // `core.fsmonitor`. Any other directory (a Path project on the server) and a server without a helper
    // keep running git in hostd.
    #[test]
    fn git_runs_as_the_crewmate_only_in_an_isolated_crewmates_tree() {
        let t = tempfile::tempdir().unwrap();
        let setup = layout::Setup {
            socket: t.path().join("s.sock"),
            provision_socket: t.path().join("p.sock"),
            root: t.path().join("cm").to_string_lossy().into_owned(),
            hook_port: 0,
        };
        let id = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let isolated = crate::crew::store::CrewStore::new(t.path()).with_isolation(Some(setup));
        let work = isolated.isolated(id).unwrap().work();
        assert!(isolated.git_in(&work).is_crewmate());
        assert!(isolated.git_in(&work.join("src")).is_crewmate());
        assert!(!isolated.git_in(&t.path().join("projects/x")).is_crewmate());
        let plain = crate::crew::store::CrewStore::new(t.path());
        assert!(!plain.git_in(&work).is_crewmate());
    }

    // Regression (security review of #448, B1): adopting the old memory must not write through a link the
    // crewmate put where `memory.md` is.
    #[test]
    fn adopting_memory_never_writes_through_a_link_in_the_crewmates_directory() {
        let t = tempfile::tempdir().unwrap();
        let (old, new, secret) = (
            t.path().join("old.md"),
            t.path().join("new.md"),
            t.path().join("hostd.token"),
        );
        std::fs::write(&secret, "token-abc").unwrap();
        std::os::unix::fs::symlink(&secret, &new).unwrap();
        std::fs::write(&old, "milk").unwrap();
        assert!(adopt_memory(&old, &new).is_err());
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "token-abc");
        assert!(old.exists(), "the old memory stays where it was");
    }
}
