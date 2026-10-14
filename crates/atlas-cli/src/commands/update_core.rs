//! Pieces of the self-update shared by `atlas update` and `atlas desktop --sync`:
//! the single-update lock, the machine-readable progress protocol the app reads, and the
//! rollback of the CLI when the app could not follow.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Set by Atlas Desktop (and anything else that wants structured progress): the CLI then
/// prints `@@atlas …` lines next to its human output.
pub const PROGRESS_ENV: &str = "ATLAS_UPDATE_PROGRESS";

/// Set by an `atlas update` that holds [`UpdateLock`] on the child processes it spawns
/// (`atlas desktop --sync`), so they do not wait on their own parent.
pub const HOLDS_LOCK_ENV: &str = "ATLAS_UPDATE_HOLDS_LOCK";

/// Phases of an update, in order. The ids are the contract with `apps/desktop/src/lib/updates.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Download,
    Verify,
    InstallCli,
    UpdateApp,
    /// Bring the terminal host (`atlas hostd`) to the new version: live upgrade, restart or ask.
    TerminalHost,
}

impl Phase {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Verify => "verify",
            Self::InstallCli => "install-cli",
            Self::UpdateApp => "update-app",
            Self::TerminalHost => "terminal-host",
        }
    }
}

fn enabled() -> bool {
    std::env::var_os(PROGRESS_ENV).is_some_and(|v| v == "1")
}

/// `@@atlas phase <id>`: the phase `id` has started (the previous one is done).
pub fn phase(p: Phase) {
    if enabled() {
        println!("@@atlas phase {}", p.id());
    }
}

/// `@@atlas host <state> [<n>]`: how the terminal host ended up: `current`, `upgraded`,
/// `restarted`, `needs-restart <live terminals>` (kept: it cannot be replaced without ending
/// terminals, the user decides) or `failed`.
pub fn host_event(state: &str, live: usize) {
    if enabled() {
        println!("@@atlas host {state} {live}");
    }
}

/// `@@atlas bytes <done> <total>` (total 0 = unknown) while downloading.
pub fn bytes(done: u64, total: u64) {
    if enabled() {
        println!("@@atlas bytes {done} {total}");
    }
}

/// `@@atlas rollback <state>`: what happened to the CLI after the app failed to update.
pub fn rollback_event(state: &str) {
    if enabled() {
        println!("@@atlas rollback {state}");
    }
}

fn lock_path() -> PathBuf {
    atlas_core::paths::atlas_home().join(".update.lock")
}

/// Exclusive update lock (`~/.atlas/.update.lock`, `flock`): a second `atlas update` (another
/// terminal, the app, another window) fails fast instead of racing the first one.
/// The kernel drops the lock when the process dies, so there is no stale-lock cleanup.
#[derive(Debug)]
pub struct UpdateLock {
    _file: File,
}

impl UpdateLock {
    pub fn acquire() -> Result<Self> {
        Self::acquire_at(&lock_path())
    }

    pub fn acquire_at(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        if !atlas_core::sys::try_lock(&file)? {
            bail!("another Atlas update is already running; wait for it to finish");
        }
        let _ = file.set_len(0);
        let _ = write!(file, "{}", std::process::id());
        Ok(Self { _file: file })
    }
}

/// The CLI binary as it was before an update, kept next to it so the update can be undone.
#[derive(Debug)]
pub struct CliBackup {
    saved: PathBuf,
    target: PathBuf,
}

impl CliBackup {
    /// Copy `target` (the installed CLI) to a sibling backup file.
    pub fn save(target: &Path) -> Result<Self> {
        let saved = target.with_file_name(".atlas.prev");
        std::fs::copy(target, &saved)
            .with_context(|| format!("cannot back up {}", target.display()))?;
        Ok(Self {
            saved,
            target: target.to_path_buf(),
        })
    }

    /// Put the previous CLI back (atomic rename over the new one).
    pub fn restore(&self) -> Result<()> {
        std::fs::rename(&self.saved, &self.target)
            .with_context(|| format!("cannot restore {}", self.target.display()))
    }

    /// The update stuck: the backup is no longer needed.
    pub fn discard(self) {
        let _ = std::fs::remove_file(&self.saved);
    }

    /// Settle the update once the app was brought to the new version (`app` is that result).
    /// On success the backup goes away. On failure the previous CLI is put back, so CLI and
    /// app never sit on different versions unnoticed, and the error says what is true now
    /// (`rollback` events tell the app which).
    pub fn finish(self, app: Result<()>, from: &str, to: &str) -> Result<()> {
        let Err(e) = app else {
            self.discard();
            return Ok(());
        };
        match self.restore() {
            Ok(()) => {
                rollback_event("restored");
                Err(e.context(format!(
                    "Atlas Desktop could not be updated, so Atlas CLI stays on v{from}"
                )))
            }
            Err(r) => {
                rollback_event("failed");
                Err(e.context(format!(
                    "Atlas Desktop could not be updated and the CLI could not be rolled back ({r}); \
                     CLI is v{to}, run `atlas desktop --install` to bring the app to it"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_exclusive_and_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("u.lock");
        let first = UpdateLock::acquire_at(&p).unwrap();
        let e = UpdateLock::acquire_at(&p).unwrap_err().to_string();
        assert!(e.contains("already running"), "{e}");
        drop(first);
        UpdateLock::acquire_at(&p).expect("free again after the holder is gone");
    }

    #[test]
    fn finish_keeps_the_new_cli_when_the_app_followed() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("atlas");
        std::fs::write(&bin, b"old").unwrap();
        let backup = CliBackup::save(&bin).unwrap();
        std::fs::write(&bin, b"new").unwrap();
        backup.finish(Ok(()), "1.0.0", "1.1.0").unwrap();
        assert_eq!(std::fs::read(&bin).unwrap(), b"new");
        assert!(!dir.path().join(".atlas.prev").exists(), "backup discarded");
    }

    #[test]
    fn finish_rolls_the_cli_back_when_the_app_failed() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("atlas");
        std::fs::write(&bin, b"old").unwrap();
        let backup = CliBackup::save(&bin).unwrap();
        std::fs::write(&bin, b"new").unwrap();
        let e = backup
            .finish(Err(anyhow::anyhow!("checksum mismatch")), "1.0.0", "1.1.0")
            .unwrap_err();
        assert_eq!(std::fs::read(&bin).unwrap(), b"old", "CLI is back");
        let msg = format!("{e:#}");
        assert!(msg.contains("stays on v1.0.0"), "{msg}");
        assert!(msg.contains("checksum mismatch"), "keeps the cause: {msg}");
    }

    #[test]
    fn finish_says_so_when_the_rollback_itself_fails() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("atlas");
        std::fs::write(&bin, b"old").unwrap();
        let backup = CliBackup::save(&bin).unwrap();
        std::fs::write(&bin, b"new").unwrap();
        std::fs::remove_file(dir.path().join(".atlas.prev")).unwrap(); // nothing to restore
        let e = backup
            .finish(Err(anyhow::anyhow!("no space")), "1.0.0", "1.1.0")
            .unwrap_err();
        assert_eq!(std::fs::read(&bin).unwrap(), b"new");
        let msg = format!("{e:#}");
        assert!(msg.contains("could not be rolled back"), "{msg}");
        assert!(msg.contains("CLI is v1.1.0"), "{msg}");
        assert!(msg.contains("atlas desktop --install"), "{msg}");
    }

    #[test]
    fn backup_restores_the_previous_binary() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("atlas");
        std::fs::write(&bin, b"old").unwrap();
        let backup = CliBackup::save(&bin).unwrap();
        std::fs::write(&bin, b"new").unwrap();
        backup.restore().unwrap();
        assert_eq!(std::fs::read(&bin).unwrap(), b"old");
        assert!(!dir.path().join(".atlas.prev").exists(), "backup consumed");
    }

    #[test]
    fn discarded_backup_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("atlas");
        std::fs::write(&bin, b"old").unwrap();
        CliBackup::save(&bin).unwrap().discard();
        assert!(!dir.path().join(".atlas.prev").exists());
        assert_eq!(std::fs::read(&bin).unwrap(), b"old");
    }

    #[test]
    fn phase_ids_are_the_wire_contract() {
        let ids: Vec<_> = [
            Phase::Download,
            Phase::Verify,
            Phase::InstallCli,
            Phase::UpdateApp,
            Phase::TerminalHost,
        ]
        .map(Phase::id)
        .to_vec();
        assert_eq!(
            ids,
            [
                "download",
                "verify",
                "install-cli",
                "update-app",
                "terminal-host"
            ]
        );
    }
}
