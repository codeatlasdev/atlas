//! The one way Atlas replaces a file: a reader (or a crash) sees the old content or the whole new
//! content, never a prefix. `.pids`, the hostd token, scrollback, session metadata, the queue's
//! records and every config file go through [`write_atomic`].

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A name beside `path` no other writer (thread or process) shares, and no reader mistakes for the
/// file: `<name>.tmp-<pid>-<n>`. The whole file name is kept, so an extension is never replaced.
fn tmp_sibling(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

/// Replace `path` with `data`: written to a unique sibling, flushed, renamed over the target, then
/// the directory is flushed (Unix) so the rename survives a crash. Missing parent directories are
/// created. `owner_only` makes the file readable by this user alone (0600 on Unix, set when the
/// temp file is created, so never wider at any moment; on Windows the folder's per-user ACL
/// applies). On any error the target is untouched and the temp file is removed.
pub fn write_atomic(path: &Path, data: &[u8], owner_only: bool) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let tmp = tmp_sibling(path);
    let written = (|| {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        if owner_only {
            crate::sys::owner_only(&mut opts);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        // Replaces an existing target on every platform (MoveFileEx on Windows).
        fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // Best effort: the rename is only durable once its directory is, and not every filesystem can
    // sync one.
    #[cfg(unix)]
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Ok(d) = fs::File::open(dir)
    {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect()
    }

    /// #301: `.pids`, the hostd token and scrollback were truncated in place, so a reader (or a
    /// crash) saw a prefix. A reader racing a writer that alternates lengths sees one whole value.
    #[test]
    fn a_reader_racing_a_writer_sees_the_old_or_the_new_content_never_a_part() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let (short, long) = ("a".repeat(10), "b".repeat(300_000));
        write_atomic(&path, short.as_bytes(), false).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let reader = {
            let (path, done, short, long) =
                (path.clone(), done.clone(), short.clone(), long.clone());
            std::thread::spawn(move || {
                let mut reads = 0;
                while !done.load(Ordering::Relaxed) {
                    let got = fs::read_to_string(&path).unwrap();
                    assert!(
                        got == short || got == long,
                        "partial read of {} bytes",
                        got.len()
                    );
                    reads += 1;
                }
                reads
            })
        };
        for i in 0..200 {
            let v = if i % 2 == 0 { &long } else { &short };
            write_atomic(&path, v.as_bytes(), false).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }

    /// #301: writers sharing one temp name (`tmp~`) interleaved bytes or lost the rename to each
    /// other. Two threads hammering one path leave one whole value and no temp file.
    #[test]
    fn concurrent_writers_to_one_path_never_corrupt_it_or_leave_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        // Different lengths: in place, one writer's tail would survive under the other's head.
        let values = [("x", 200_000), ("y", 150_000)].map(|(c, n)| c.repeat(n));
        std::thread::scope(|s| {
            for v in values.iter() {
                let path = &path;
                s.spawn(move || {
                    for _ in 0..1000 {
                        write_atomic(path, v.as_bytes(), false).unwrap();
                    }
                });
            }
        });
        let got = fs::read_to_string(&path).unwrap();
        assert!(values.contains(&got), "{} bytes, mixed", got.len());
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_files_are_0600_even_when_replaced_and_others_follow_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic(&path, b"secret", true).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(fs::read(&path).unwrap(), b"secret");
        let open = dir.path().join("open");
        write_atomic(&open, b"x", false).unwrap();
        assert_ne!(
            mode(&open) & 0o044,
            0,
            "a plain write keeps the default mode"
        );
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_removes_its_temp() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a directory: the rename fails after the data was written.
        let target = dir.path().join("d");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, b"x", false).is_err());
        assert!(target.is_dir());
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn missing_parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/f");
        write_atomic(&path, b"x", false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
    }
}
