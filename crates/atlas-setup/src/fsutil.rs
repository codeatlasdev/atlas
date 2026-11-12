//! Small file helpers: the install folder's size (the progress signal) and the log.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bytes of every file under `dir` (0 when it does not exist). Unreadable entries count as 0: the
/// installer is writing there, and a half-created file is expected.
pub fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// Append-only log. Cloning the path is enough to find it again; a log that cannot be written
/// never fails the install (the window still reports), it just stays quiet.
pub struct Log {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
}

impl Log {
    /// Start a fresh log at `path` (the previous run's is replaced).
    pub fn create(path: PathBuf) -> Self {
        let file = std::fs::File::create(&path).ok();
        Self {
            path,
            file: Mutex::new(file),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn line(&self, msg: impl AsRef<str>) {
        let mut g = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = g.as_mut() {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(f, "[{secs}] {}", msg.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_bytes_sums_nested_files_and_survives_a_missing_dir() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("a/b")).unwrap();
        std::fs::write(t.path().join("x"), [0u8; 10]).unwrap();
        std::fs::write(t.path().join("a/y"), [0u8; 20]).unwrap();
        std::fs::write(t.path().join("a/b/z"), [0u8; 30]).unwrap();
        assert_eq!(dir_bytes(t.path()), 60);
        assert_eq!(dir_bytes(&t.path().join("nope")), 0);
    }

    #[test]
    fn a_new_log_replaces_the_previous_runs() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("atlas-setup.log");
        Log::create(p.clone()).line("first run");
        let l = Log::create(p.clone());
        l.line("second run");
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains("second run") && !s.contains("first run"), "{s}");
    }

    #[test]
    fn an_unwritable_log_is_silent_not_fatal() {
        let t = tempfile::tempdir().unwrap();
        // A directory where the file should be: create() fails, line() must not panic.
        let l = Log::create(t.path().to_path_buf());
        l.line("still fine");
    }
}
