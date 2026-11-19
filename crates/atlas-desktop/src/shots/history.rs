//! Capture history: the last N captures, newest first, persisted next to the app's other state. Pure list logic +
//! JSON (de)serialization; the file location and the "does it still exist" check are parameters.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::naming::Kind;

/// One capture in the history (camelCase for the page).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Stable id (the file's path): what the page passes back to act on it.
    pub path: PathBuf,
    pub kind: Kind,
    /// Milliseconds since the epoch.
    pub at: u64,
    /// Pixel size (images and videos); 0 when unknown.
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// Seconds (recordings and GIFs).
    pub secs: Option<f64>,
}

/// Newest-first list with a cap.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub entries: Vec<Entry>,
}

impl History {
    /// Record `e` at the front. The same path again (an edit saved over the original) replaces the old entry
    /// instead of listing the file twice. Keeps at most `cap`.
    pub fn push(&mut self, e: Entry, cap: usize) {
        self.entries.retain(|o| o.path != e.path);
        self.entries.insert(0, e);
        self.entries.truncate(cap.max(1));
    }

    /// Forget one capture (the file is not touched).
    pub fn remove(&mut self, path: &Path) -> bool {
        let n = self.entries.len();
        self.entries.retain(|o| o.path != path);
        self.entries.len() != n
    }

    /// Drop what is gone from disk: the user deleted it in Finder. A history that lists files that open to
    /// "No such file" is a history nobody trusts.
    pub fn prune(&mut self, exists: impl Fn(&Path) -> bool) -> usize {
        let n = self.entries.len();
        self.entries.retain(|e| exists(&e.path));
        n - self.entries.len()
    }

    /// `cap` shrank in Settings: trim now, not at the next capture.
    pub fn trim(&mut self, cap: usize) {
        self.entries.truncate(cap.max(1));
    }

    /// Read a history file; a missing, empty or corrupt file is an empty history (never an error that stops
    /// captures from working).
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// The capture file `path` names, resolved and safe to hand to the OS opener, or why not. The page is a
    /// webview and its `path` is untrusted: it must be an entry of this history (so a `../` escape, a path
    /// outside the capture folders or one starting with `-` is no entry), absolute (so it can never be read as a
    /// command-line option), of the extension its capture kind produces (never an `.app` or `.command`), and a
    /// regular file that is itself in the folder it is listed in (a symlink that leaves it is refused). Returns the
    /// resolved path.
    pub fn launchable(&self, path: &Path) -> Result<PathBuf, String> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.path == path)
            .ok_or("That file is not one of Atlas's captures.")?;
        let name = path.file_name().filter(|_| path.is_absolute());
        let ext_ok = path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case(entry.kind.extension()));
        let (Some(name), true) = (name, ext_ok) else {
            return Err("That is not a capture file.".into());
        };
        let gone = |e: std::io::Error| format!("{}: {e}", path.display());
        let resolved = std::fs::canonicalize(path).map_err(gone)?;
        let folder =
            std::fs::canonicalize(path.parent().unwrap_or(Path::new("/"))).map_err(gone)?;
        let stays = resolved.parent() == Some(folder.as_path())
            && resolved
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(name));
        if stays && resolved.is_file() {
            Ok(resolved)
        } else {
            Err("That capture is not a plain file in its folder.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, at: u64) -> Entry {
        Entry {
            path: PathBuf::from(format!("/shots/{name}")),
            kind: Kind::Screenshot,
            at,
            width: 100,
            height: 50,
            bytes: 10,
            secs: None,
        }
    }

    #[test]
    fn the_newest_capture_is_first_and_the_list_is_capped() {
        let mut h = History::default();
        for i in 0..5 {
            h.push(e(&format!("{i}.png"), i), 3);
        }
        let names: Vec<_> = h.entries.iter().map(|e| e.at).collect();
        assert_eq!(names, [4, 3, 2], "newest first, the oldest dropped");
    }

    /// Saving an edit over the original must not list the file twice (it would show two thumbnails of one file).
    #[test]
    fn the_same_file_again_replaces_its_entry_and_moves_to_the_front() {
        let mut h = History::default();
        h.push(e("a.png", 1), 10);
        h.push(e("b.png", 2), 10);
        h.push(e("a.png", 3), 10);
        let paths: Vec<_> = h.entries.iter().map(|e| (e.path.clone(), e.at)).collect();
        assert_eq!(
            paths,
            [
                (PathBuf::from("/shots/a.png"), 3),
                (PathBuf::from("/shots/b.png"), 2)
            ]
        );
    }

    #[test]
    fn a_cap_of_zero_still_keeps_the_latest() {
        let mut h = History::default();
        h.push(e("a.png", 1), 0);
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn files_deleted_in_finder_are_pruned_and_counted() {
        let mut h = History::default();
        h.push(e("a.png", 1), 10);
        h.push(e("b.png", 2), 10);
        h.push(e("c.png", 3), 10);
        let gone = h.prune(|p| p != Path::new("/shots/b.png"));
        assert_eq!(gone, 1);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.prune(|_| true), 0);
    }

    #[test]
    fn removing_forgets_the_entry_not_the_file_and_says_if_it_was_there() {
        let mut h = History::default();
        h.push(e("a.png", 1), 10);
        assert!(h.remove(Path::new("/shots/a.png")));
        assert!(!h.remove(Path::new("/shots/a.png")));
        assert!(h.entries.is_empty());
    }

    #[test]
    fn shrinking_the_cap_trims_immediately() {
        let mut h = History::default();
        for i in 0..10 {
            h.push(e(&format!("{i}.png"), i), 50);
        }
        h.trim(4);
        assert_eq!(h.entries.len(), 4);
        assert_eq!(h.entries[0].at, 9);
    }

    /// A corrupt history file must never stop a capture from being saved.
    #[test]
    fn a_missing_or_corrupt_file_is_an_empty_history() {
        for bad in ["", "not json", "{", "[]", "{\"entries\": 3}"] {
            assert_eq!(History::parse(bad), History::default(), "{bad:?}");
        }
    }

    /// The page reads these exact keys (`shots/history.ts`): serialize the real value.
    #[test]
    fn entries_serialize_with_the_keys_the_page_reads_and_round_trip() {
        let mut h = History::default();
        h.push(e("a.png", 1), 10);
        let v = serde_json::to_value(&h).unwrap();
        let first = &v["entries"][0];
        assert_eq!(first["path"], "/shots/a.png");
        assert_eq!(first["kind"], "screenshot");
        assert_eq!(first["width"], 100);
        assert!(first["secs"].is_null());
        assert_eq!(History::parse(&v.to_string()), h);
    }

    /// A history over real files in a temp folder: `(history, folder)`, one entry per `(name, kind)`.
    fn on_disk(files: &[(&str, Kind)]) -> (History, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::default();
        for (i, (name, kind)) in files.iter().enumerate() {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if !path.exists() {
                std::fs::write(&path, b"x").unwrap();
            }
            h.push(
                Entry {
                    path,
                    kind: *kind,
                    ..e("x", i as u64)
                },
                50,
            );
        }
        (h, dir)
    }

    // Regression: `shots_history_open` ran `open <any path>` for any local page: any app, any `.command`, any option.
    #[test]
    fn a_real_capture_of_the_history_is_launchable_for_each_kind() {
        let (h, dir) = on_disk(&[
            ("a.png", Kind::Screenshot),
            ("b.mp4", Kind::Recording),
            ("c.GIF", Kind::Gif),
        ]);
        for n in ["a.png", "b.mp4", "c.GIF"] {
            let got = h.launchable(&dir.path().join(n)).unwrap();
            assert!(got.is_absolute() && got.is_file(), "{n}");
            assert_eq!(got.file_name().unwrap(), n);
        }
    }

    #[test]
    fn a_real_file_outside_the_history_is_refused() {
        let (h, _dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.png");
        std::fs::write(&secret, b"x").unwrap();
        assert!(h.launchable(&secret).is_err());
        assert!(h.launchable(Path::new("/etc/passwd")).is_err());
        assert!(
            h.launchable(Path::new("/Applications/Calculator.app"))
                .is_err()
        );
    }

    #[test]
    fn a_dot_dot_escape_out_of_the_history_folder_is_refused() {
        let (h, dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), b"x").unwrap();
        let escape = dir
            .path()
            .join("../")
            .join(outside.path().file_name().unwrap())
            .join("secret.png");
        assert!(escape.is_file(), "the escape path really reaches the file");
        assert!(h.launchable(&escape).is_err());
        // The listed file reached through `sub/..` is a different spelling, not the entry.
        assert!(h.launchable(&dir.path().join("sub/../a.png")).is_err());
    }

    #[test]
    fn a_file_of_the_folder_that_is_not_listed_is_refused() {
        let (h, dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        std::fs::write(dir.path().join("untracked.png"), b"x").unwrap();
        assert!(h.launchable(&dir.path().join("untracked.png")).is_err());
    }

    #[test]
    fn a_path_that_could_be_an_option_or_is_relative_is_refused() {
        let (h, _dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        for bad in ["-a", "--args", "-R", "-a Calculator", "a.png", ""] {
            assert!(h.launchable(Path::new(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_app_bundle_is_refused_by_type_even_when_listed_in_the_history() {
        let (h, dir) = on_disk(&[("Evil.app", Kind::Screenshot)]);
        assert!(h.launchable(&dir.path().join("Evil.app")).is_err());
    }

    #[test]
    fn a_command_script_is_refused_by_type_even_when_listed_in_the_history() {
        let (h, dir) = on_disk(&[
            ("run.command", Kind::Screenshot),
            ("noext", Kind::Gif),
            ("shot.png.command", Kind::Screenshot),
            ("movie.png", Kind::Recording),
        ]);
        for n in ["run.command", "noext", "shot.png.command", "movie.png"] {
            assert!(h.launchable(&dir.path().join(n)).is_err(), "{n}");
        }
    }

    #[test]
    fn a_folder_named_like_a_capture_is_refused() {
        let (h, dir) = on_disk(&[("dir.png", Kind::Screenshot)]);
        std::fs::remove_file(dir.path().join("dir.png")).unwrap();
        std::fs::create_dir(dir.path().join("dir.png")).unwrap();
        assert!(h.launchable(&dir.path().join("dir.png")).is_err());
    }

    #[test]
    fn a_vanished_capture_is_an_error_not_a_launch() {
        let (h, dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        std::fs::remove_file(dir.path().join("a.png")).unwrap();
        assert!(h.launchable(&dir.path().join("a.png")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_capture_folder_or_changes_the_type_is_refused() {
        use std::os::unix::fs::symlink;
        let (h, dir) = on_disk(&[
            ("link.png", Kind::Screenshot),
            ("scripty.png", Kind::Screenshot),
            ("dirlink.png", Kind::Screenshot),
            ("ok.png", Kind::Screenshot),
        ]);
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("secret.png");
        std::fs::write(&target, b"x").unwrap();
        std::fs::write(dir.path().join("run.command"), b"#!/bin/sh").unwrap();
        for (link, to) in [
            ("link.png", target),
            ("scripty.png", dir.path().join("run.command")),
            ("dirlink.png", outside.path().to_path_buf()),
        ] {
            let at = dir.path().join(link);
            std::fs::remove_file(&at).unwrap();
            symlink(to, &at).unwrap();
            assert!(h.launchable(&at).is_err(), "{link}");
        }
        assert!(h.launchable(&dir.path().join("ok.png")).is_ok());
    }

    // The history folder itself reached through a symlink (macOS: /var -> /private/var) is not an escape.
    #[cfg(unix)]
    #[test]
    fn a_folder_reached_through_a_symlink_is_still_launchable() {
        use std::os::unix::fs::symlink;
        let (_, dir) = on_disk(&[("a.png", Kind::Screenshot)]);
        let via = tempfile::tempdir().unwrap();
        let alias = via.path().join("alias");
        symlink(dir.path(), &alias).unwrap();
        let mut h = History::default();
        h.push(
            Entry {
                path: alias.join("a.png"),
                ..e("a.png", 1)
            },
            10,
        );
        assert!(h.launchable(&alias.join("a.png")).is_ok());
    }
}
