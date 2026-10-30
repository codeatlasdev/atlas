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
}
