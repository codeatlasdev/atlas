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

/// Who may read the replaced file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// What the umask gives a new file.
    Default,
    /// This user alone: 0600 on Unix. On Windows the folder's per-user ACL applies.
    OwnerOnly,
    /// Exactly these permission bits on Unix (a group that must read the file), whatever the umask;
    /// ignored on Windows.
    Exact(u32),
}

impl Mode {
    fn bits(self) -> Option<u32> {
        match self {
            Mode::Default => None,
            Mode::OwnerOnly => Some(0o600),
            Mode::Exact(bits) => Some(bits),
        }
    }
}

/// Replace `path` with `data`: written to a unique sibling, flushed, renamed over the target, then
/// the directory is flushed (Unix) so the rename survives a crash. Missing parent directories are
/// created. `mode` is set on the temp file before the rename (given at creation, so a secret is
/// never wider at any moment, then set again because the umask may have narrowed it). On any error
/// the target is untouched and the temp file is removed.
pub fn write_atomic(path: &Path, data: &[u8], mode: Mode) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let tmp = tmp_sibling(path);
    let written = (|| {
        let bits = mode.bits();
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        if let Some(bits) = bits {
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, bits);
        }
        let mut f = opts.open(&tmp)?;
        #[cfg(unix)]
        if let Some(bits) = bits {
            f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(bits))?;
        }
        #[cfg(not(unix))]
        let _ = bits; // Windows: the folder's ACL decides.
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        // Replaces an existing target on every platform, and on Windows without a moment in which a reader opening
        // the target finds it missing (#545).
        crate::sys::replace_file(&tmp, path)
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
        write_atomic(&path, short.as_bytes(), Mode::Default).unwrap();
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
            write_atomic(&path, v.as_bytes(), Mode::Default).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }

    /// #301: writers sharing one temp name (`tmp~`) interleaved bytes or lost the rename to each
    /// other. Two threads hammering one path leave one whole value and no temp file. 100 writes
    /// each: every one is a flushed 150-200 KB file, and 1000 each took 40 s to over 60 s on the
    /// Windows runner (nextest's slow mark) for no extra chance to interleave (the shared name
    /// fails within the first few).
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
                    for _ in 0..100 {
                        write_atomic(path, v.as_bytes(), Mode::Default).unwrap();
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
    fn modes_are_set_even_when_replacing_a_wider_file_and_default_follows_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic(&path, b"secret", Mode::OwnerOnly).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(fs::read(&path).unwrap(), b"secret");
        // `Default` is what the umask gives any new file here, whatever the umask is.
        let (open, plain) = (dir.path().join("open"), dir.path().join("plain"));
        write_atomic(&open, b"x", Mode::Default).unwrap();
        fs::write(&plain, b"x").unwrap();
        assert_eq!(mode(&open), mode(&plain));
        // `Exact` survives a narrowing umask: the group bit is the point.
        let group = dir.path().join("group");
        write_atomic(&group, b"x", Mode::Exact(0o640)).unwrap();
        assert_eq!(mode(&group), 0o640);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_removes_its_temp() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a directory: the rename fails after the data was written.
        let target = dir.path().join("d");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, b"x", Mode::Default).is_err());
        assert!(target.is_dir());
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn missing_parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/f");
        write_atomic(&path, b"x", Mode::Default).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
    }

    /// #545 (`status_write_is_atomic_under_concurrent_reads`, Windows): a reader opening the target while it is
    /// replaced found it missing (`NotFound`) now and then, because `std::fs::rename` is `MoveFileExW`, which does not
    /// promise that the name never disappears. The reader here only opens the name, as often as it can: the old file
    /// or the new one, never `NotFound`. (Unix never had the window; the Windows runner is where this can fail.)
    #[test]
    fn replacing_a_file_never_leaves_its_name_missing_for_a_reader_that_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.json");
        fs::write(&path, "0").unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let reader = {
            let (path, done) = (path.clone(), done.clone());
            std::thread::spawn(move || {
                let mut opens = 0u32;
                while !done.load(Ordering::Relaxed) {
                    let got = fs::read_to_string(&path).unwrap_or_else(|e| {
                        panic!("the name was missing for a reader after {opens} opens: {e}")
                    });
                    assert!(got.parse::<u32>().is_ok(), "{got:?}");
                    opens += 1;
                }
                opens
            })
        };
        for n in 1..2000 {
            let tmp = dir.path().join(format!("status.json.tmp-{n}"));
            fs::write(&tmp, n.to_string()).unwrap();
            crate::sys::replace_file(&tmp, &path).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), "1999");
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
    }

    /// The old file lives on for whoever already holds it open (on every OS), and the name goes to the new one.
    #[test]
    fn a_reader_that_already_holds_the_old_file_keeps_reading_it_after_the_replace() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        fs::write(&path, "old content").unwrap();
        let mut held = fs::File::open(&path).unwrap();
        write_atomic(&path, b"new", Mode::Default).unwrap();
        let mut seen = String::new();
        held.read_to_string(&mut seen).unwrap();
        assert_eq!(seen, "old content");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }
}
