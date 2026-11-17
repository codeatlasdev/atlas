//! The light editor's file access (#499): read, stat and write one file straight from the app process.
//!
//! A loose file opens before anything else of Atlas is up. The project editor reads through hostd (`fs_read`), which
//! on a cold start may still be spawning; these three commands use `std::fs` on a blocking thread and answer in the
//! shape the editor already reads (`{kind, size, mtime}`, `{data, truncated}`), so opening a file costs one IPC round
//! trip per call and no other process. They are for the editor's own file only; the page never walks with them.

use std::io::Read as _;
use std::path::Path;
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct Stat {
    kind: &'static str,
    size: u64,
    /// Milliseconds since the epoch.
    mtime: u64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Read {
    /// The bytes, base64.
    data: String,
    /// The file is longer than `max`: `data` is its start.
    truncated: bool,
}

fn stat_of(path: &Path) -> Result<Stat, String> {
    let m = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    Ok(Stat {
        kind: if m.is_dir() { "dir" } else { "file" },
        size: m.len(),
        mtime,
    })
}

fn read_of(path: &Path, max: Option<u64>) -> Result<Read, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cap = max.unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    // One byte more than `max` tells "exactly max long" from "longer".
    file.take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let truncated = bytes.len() as u64 > cap;
    if truncated {
        bytes.truncate(cap as usize);
    }
    Ok(Read {
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        truncated,
    })
}

/// Write to a sibling temp file and rename it over `path`, keeping the file's permissions, so a crash or a full disk
/// never leaves half a file where the user's text was.
fn write_of(path: &Path, text: &str) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} is not a file path", path.display()))?;
    let tmp = path.with_file_name(format!(".{}.atlas-tmp", name.to_string_lossy()));
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::write(&tmp, text).map_err(fail)?;
    if let Ok(m) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        fail(e)
    })
}

#[tauri::command]
pub async fn loose_stat(path: String) -> Result<Stat, String> {
    tokio::task::spawn_blocking(move || stat_of(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn loose_read(path: String, max: Option<u64>) -> Result<Read, String> {
    tokio::task::spawn_blocking(move || read_of(Path::new(&path), max))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn loose_write(path: String, text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || write_of(Path::new(&path), &text))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(r: &Read) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(&r.data)
            .unwrap()
    }

    #[test]
    fn a_file_reads_back_as_written_with_its_size_and_a_moving_mtime() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("notes.md");
        write_of(&f, "héllo\nworld\n").unwrap();
        let s = stat_of(&f).unwrap();
        assert_eq!((s.kind, s.size), ("file", "héllo\nworld\n".len() as u64));
        assert!(s.mtime > 0);
        let r = read_of(&f, None).unwrap();
        assert_eq!(decode(&r), "héllo\nworld\n".as_bytes());
        assert!(!r.truncated);
        assert_eq!(stat_of(t.path()).unwrap().kind, "dir");
    }

    // The editor's size cap relies on `truncated`: a file of exactly `max` bytes is whole, one byte more is not.
    #[test]
    fn the_cap_truncates_only_a_longer_file() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("a.txt");
        std::fs::write(&f, "0123456789").unwrap();
        let exact = read_of(&f, Some(10)).unwrap();
        assert!((decode(&exact).len(), exact.truncated) == (10, false));
        let cut = read_of(&f, Some(4)).unwrap();
        assert_eq!(decode(&cut), b"0123");
        assert!(cut.truncated);
    }

    #[test]
    fn a_write_replaces_the_file_atomically_and_leaves_no_temp_file() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("a.txt");
        std::fs::write(&f, "old").unwrap();
        write_of(&f, "new text").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new text");
        let names: Vec<_> = std::fs::read_dir(t.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_the_files_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("run.sh");
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        write_of(&f, "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn missing_files_and_unwritable_places_are_errors_naming_the_path() {
        let t = tempfile::tempdir().unwrap();
        let gone = t.path().join("gone.txt");
        assert!(read_of(&gone, None).unwrap_err().contains("gone.txt"));
        assert!(stat_of(&gone).unwrap_err().contains("gone.txt"));
        let nowhere = t.path().join("no/such/dir/a.txt");
        assert!(write_of(&nowhere, "x").is_err());
        assert!(!nowhere.exists());
    }
}
