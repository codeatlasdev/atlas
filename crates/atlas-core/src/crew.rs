//! Atlas Crew names shared by hostd, the CLI and the desktop, and the one on-disk migration they all
//! need: Atlas Crew was `Atlas Bot` before it shipped, and servers already run crewmates under the
//! old names (`<hostd dir>/bots/`, `desktop/bot-hosts.json`).

use std::path::Path;

/// System user of a crew server installed from now on. A server installed before the rename has
/// [`LEGACY_USER`] instead; that user is reused, never renamed (its home holds the data).
pub const USER: &str = "atlas-crew";

/// The system user (and systemd unit, install dir, home) of crew servers installed as "Atlas Bot".
/// Kept only to recognise and migrate those servers.
pub const LEGACY_USER: &str = "atlas-bot";

/// What [`adopt_legacy_path`] did, so the caller can log one line.
#[derive(Debug, PartialEq, Eq)]
pub enum Adopted {
    /// The old path was renamed to the new one (atomic `rename`).
    Moved,
    /// Nothing to migrate: the old path does not exist.
    Nothing,
    /// Both exist: the new one wins and the old one is left untouched.
    BothExist,
}

/// Move `old` to `new` when only `old` exists. Both existing is not an error: `new` is the truth
/// from then on and `old` stays for a human to look at (nothing is ever merged or deleted).
pub fn adopt_legacy_path(old: &Path, new: &Path) -> std::io::Result<Adopted> {
    if !old.exists() {
        return Ok(Adopted::Nothing);
    }
    if new.exists() {
        return Ok(Adopted::BothExist);
    }
    std::fs::rename(old, new)?;
    Ok(Adopted::Moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guards the rename Bot -> Crew: Luan's servers keep their crewmates in `<dir>/bots/<id>/`.
    #[test]
    fn a_directory_only_under_the_old_name_is_moved_with_its_content() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("bots"), t.path().join("crew"));
        std::fs::create_dir_all(old.join("abc")).unwrap();
        std::fs::write(old.join("abc/bot.json"), "{}").unwrap();

        assert_eq!(adopt_legacy_path(&old, &new).unwrap(), Adopted::Moved);

        assert!(!old.exists());
        assert_eq!(
            std::fs::read_to_string(new.join("abc/bot.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn a_directory_only_under_the_new_name_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("bots"), t.path().join("crew"));
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("keep"), "1").unwrap();

        assert_eq!(adopt_legacy_path(&old, &new).unwrap(), Adopted::Nothing);

        assert_eq!(std::fs::read_to_string(new.join("keep")).unwrap(), "1");
        assert!(!old.exists());
    }

    // Both present (a second hostd already wrote the new dir): the new one wins, the old one is
    // never merged, overwritten or deleted.
    #[test]
    fn with_both_present_the_new_one_wins_and_the_old_one_is_untouched() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("bots"), t.path().join("crew"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("old.txt"), "old").unwrap();
        std::fs::write(new.join("new.txt"), "new").unwrap();

        assert_eq!(adopt_legacy_path(&old, &new).unwrap(), Adopted::BothExist);

        assert_eq!(std::fs::read_to_string(old.join("old.txt")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(new.join("new.txt")).unwrap(), "new");
        assert!(!new.join("old.txt").exists(), "never merged");
    }

    #[test]
    fn neither_present_is_nothing_to_do() {
        let t = tempfile::tempdir().unwrap();
        let r = adopt_legacy_path(&t.path().join("bots"), &t.path().join("crew"));
        assert_eq!(r.unwrap(), Adopted::Nothing);
        assert!(!t.path().join("crew").exists());
    }

    // The desktop registry is a single file: same rule.
    #[test]
    fn a_registry_file_follows_the_same_rule() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (
            t.path().join("bot-hosts.json"),
            t.path().join("crew-servers.json"),
        );
        std::fs::write(&old, r#"[{"host":"atlas-bot@box","name":"Ship"}]"#).unwrap();
        assert_eq!(adopt_legacy_path(&old, &new).unwrap(), Adopted::Moved);
        assert!(new.exists() && !old.exists());

        std::fs::write(&old, "[]").unwrap();
        assert_eq!(adopt_legacy_path(&old, &new).unwrap(), Adopted::BothExist);
        assert_eq!(
            std::fs::read_to_string(&new).unwrap(),
            r#"[{"host":"atlas-bot@box","name":"Ship"}]"#
        );
    }
}
