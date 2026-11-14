//! What a task needs to know about isolation (issue #200), asked once per task: does this crewmate run
//! as its own Linux user, and so where do its files, its hook endpoint and its processes live?
//!
//! Without a helper (`spawn::layout::active()` false) every answer is the one from before: the crewmate's
//! store directory, the global hook endpoint, a process of the crew user. [`Ctx`] is the only place that
//! decides, so the runners cannot disagree about it.

use std::path::{Path, PathBuf};

use crate::crew::engine::proc::Runner;
use crate::crew::project;
#[cfg(unix)]
use crate::crew::spawn::client::Client;
use crate::crew::spawn::layout::{self, GITCONFIG, Isolated};
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
    pub(crate) fn runner(&self) -> Runner {
        #[cfg(unix)]
        if let Some((client, iso)) = &self.iso {
            return Runner::isolated(client.clone(), iso.user.clone());
        }
        Runner::local()
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
    /// follows through `ATLAS_HOOK_ENDPOINT`, and git's config for the shared checkout). A no-op when not
    /// isolated. Group-readable and not writable: the crewmate is in the directory's group only.
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
        )?;
        write_readable(&iso.gitconfig(), GITCONFIG.as_bytes())
    }

    /// Environment a terminal agent needs on top of the request's own: where its hook endpoint file is,
    /// its token, and git's config. Empty when not isolated.
    pub(crate) fn agent_env(&self, hub: &Hub, id: &str) -> Vec<(String, String)> {
        let Some(iso) = self.isolated() else {
            return vec![];
        };
        let (port, token) = self.hook_endpoint(hub, id);
        let mut env = vec![
            (
                "ATLAS_HOOK_ENDPOINT".to_string(),
                iso.hook_endpoint().to_string_lossy().into_owned(),
            ),
            ("ATLAS_HOOK_PORT".to_string(), port.to_string()),
            ("ATLAS_HOOK_TOKEN".to_string(), token),
        ];
        env.extend(layout::git_env(Some(iso)));
        env
    }
}

/// Move a crewmate's memory from the crew home (where it lived before isolation) into its own
/// directory, once: only when the new file is empty and the old one has content. The old file is kept
/// as `memory.md.pre-isolation`, never deleted.
fn adopt_memory(old: &Path, new: &Path) -> std::io::Result<()> {
    let have = std::fs::read_to_string(new).unwrap_or_default();
    let before = match std::fs::read_to_string(old) {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return Ok(()),
    };
    if !have.trim().is_empty() {
        return Ok(());
    }
    // In place: the file is the crewmate's (group-writable), not replaced by hostd's.
    std::fs::write(new, before)?;
    std::fs::rename(old, old.with_extension("md.pre-isolation"))
}

/// Replace `path` with `data`, mode 0640 (the crewmate reads it through the group, nobody else does).
/// Atomic, so an agent never reads half a file.
#[cfg(unix)]
fn write_readable(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let tmp = path.with_extension("tmp~");
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o640)
            .open(&tmp)?;
        f.write_all(data)?;
    }
    // The umask may have narrowed the mode at creation; the group bit is the point.
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o640))?;
    std::fs::rename(tmp, path)
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
        assert!(!p.with_extension("tmp~").exists());
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
        assert!(!c.runner().is_isolated());
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
        assert!(c.runner().is_isolated());
        // Test crewmates with short ids (and any id that is not a UUID) stay unisolated.
        let plain = Ctx::from_store(&store, "crewmate-a");
        assert!(plain.isolated().is_none() && !plain.runner().is_isolated());
    }
}
