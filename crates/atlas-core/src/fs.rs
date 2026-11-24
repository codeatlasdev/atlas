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
        "{TMP_MARK}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

const TMP_MARK: &str = ".tmp-";

/// Is a [`write_atomic`] of `path` between creating its temp file and finishing the rename? The temp file is the
/// sign: it exists for the whole replace, and a writer that crashed leaves one behind (a reader then waits its
/// retries for nothing, a few tens of ms).
fn replace_in_flight(path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let prefix = format!("{}{TMP_MARK}", name.to_string_lossy());
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(&prefix))
    })
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
    write_atomic_budget(path, data, mode, crate::sys::RetryBudget::Full)
}

/// [`write_atomic`] for a snapshot that is rewritten every tick anyway (`.tui-status.json`, `.tui-metrics.json`): on
/// Windows a refused replace is retried for about 63 ms instead of 1.2 s, then the write is dropped (`Err`, old file
/// untouched) and the next tick lands, so the caller's tick is never stalled for long. Not for a write whose loss
/// matters: those use [`write_atomic`].
pub fn write_atomic_snapshot(path: &Path, data: &[u8], mode: Mode) -> io::Result<()> {
    write_atomic_budget(path, data, mode, crate::sys::RetryBudget::Brief)
}

fn write_atomic_budget(
    path: &Path,
    data: &[u8],
    mode: Mode,
    budget: crate::sys::RetryBudget,
) -> io::Result<()> {
    write_atomic_via(path, data, mode, |from, to| {
        crate::sys::replace_file_with(from, to, budget)
    })
}

/// [`write_atomic`] with the replace step given: the seam that lets a test run the step Windows falls back to when the
/// POSIX rename is refused (`std::fs::rename`, which can leave the name missing for a moment) on every OS.
fn write_atomic_via(
    path: &Path,
    data: &[u8],
    mode: Mode,
    replace: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
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
        // Replaces an existing target on every platform. On Windows the POSIX rename (retried when refused, never
        // replaced by the legacy rename for a refusal that lasts) leaves no moment in which the name is missing (#545,
        // #743); readers still cover a moment with [`read_replaced`].
        replace(&tmp, path)
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

/// Read a file that another process replaces with [`write_atomic`] (`.tui-status.json` every second, the metrics and
/// history files): the old content or the new one, never an error the replace itself causes. Two windows exist on
/// Windows, both retried briefly (1, 2, 4... ms), after which the next open finds the new file:
/// - "Access is denied": the open had already found the old file when the rename marked it for deletion
///   (`STATUS_DELETE_PENDING`), which the writer cannot close (#545). Up to 5 retries.
/// - `NotFound`: a replace that leaves the name missing for a moment: the legacy `std::fs::rename` the writer uses
///   where the POSIX rename is not supported, a replace by something else (#743), and the POSIX rename itself, which
///   is not atomic against an open of the name (about 7 of 3.6 million raw opens on Windows runners found it missing
///   while the replace ran). Retried while the writer's temp file exists, up to 8 retries (about 250 ms). A file that
///   does not exist and has no replace in flight is the common answer (no run), and is returned at once.
///
/// This is a bounded safety net, not a guarantee: the writer keeps the name present almost always, and the retries
/// cover the rest for about 250 ms. A replace that outlasts them (a writer descheduled in the middle of the replace, an
/// antivirus scan holding the temp file longer) still shows as the error.
pub fn read_replaced(path: &Path) -> io::Result<Vec<u8>> {
    let mut attempt = 0;
    loop {
        match fs::read(path) {
            Err(e) if attempt < 5 && replaced_meanwhile(&e) => {}
            Err(e)
                if attempt < 8
                    && e.kind() == io::ErrorKind::NotFound
                    && replace_in_flight(path) =>
            {
                #[cfg(test)]
                NOT_FOUND_RETRIES.with(|c| c.set(c.get() + 1));
            }
            // The replace may have ended between the failed open and the look for its temp file: one more open.
            Err(e) if e.kind() == io::ErrorKind::NotFound => return fs::read(path),
            r => return r,
        }
        std::thread::sleep(std::time::Duration::from_millis(1 << attempt));
        attempt += 1;
    }
}

#[cfg(test)]
thread_local! {
    /// How many `NotFound` opens this thread's [`read_replaced`] waited out (tests only): proof that a test met the gap.
    static NOT_FOUND_RETRIES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// [`read_replaced`] as UTF-8 text.
pub fn read_to_string_replaced(path: &Path) -> io::Result<String> {
    String::from_utf8(read_replaced(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The refusal an open gets while the file it found is being replaced (Windows `ERROR_ACCESS_DENIED`).
fn replaced_meanwhile(e: &io::Error) -> bool {
    cfg!(windows) && e.raw_os_error() == Some(5)
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
    /// promise that the name never disappears. The contract is for a reader that reads with [`read_replaced`]: the old
    /// file or the new one, never an error, as often as it can open it. A plain `fs::read` can still be refused
    /// ("Access is denied", the old file delete-pending under an open in flight): seen on the Windows runner after
    /// 35494 opens with the POSIX rename and its retries. (Unix never had the window.)
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
                    let got = read_to_string_replaced(&path).unwrap_or_else(|e| {
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

    /// #743 (`status_write_is_atomic_under_concurrent_reads`, Windows, `NotFound` at the reader): the reader's safety
    /// net. The writer keeps the name present almost always (`sys::replace_file`: POSIX rename, `std::fs::rename` only
    /// where that cannot be done at all), but the POSIX rename is not atomic against an open either, and a replace of
    /// another writer or a filesystem that does leave it missing for a moment is not ours to promise about. The
    /// replace step is given as one that removes the target, waits 5 ms and renames, so the window is open on every OS
    /// and for every write: a reader still gets the old file or the new one.
    #[test]
    fn a_reader_survives_a_replace_that_leaves_the_name_missing_for_a_while() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.json");
        write_atomic(&path, b"0", Mode::Default).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let reader = {
            let (path, done) = (path.clone(), done.clone());
            std::thread::spawn(move || {
                let mut opens = 0u32;
                while !done.load(Ordering::Relaxed) {
                    let got = read_to_string_replaced(&path).unwrap_or_else(|e| {
                        panic!("the name was missing for a reader after {opens} opens: {e}")
                    });
                    assert!(got.parse::<u32>().is_ok(), "{got:?}");
                    opens += 1;
                }
                (opens, NOT_FOUND_RETRIES.with(std::cell::Cell::get))
            })
        };
        let delete_then_rename = |from: &Path, to: &Path| {
            fs::remove_file(to)?;
            // Held open by spinning, not sleeping: a sleep on a loaded CI runner (macOS) can outlast the reader's
            // whole budget, which says nothing about the reader.
            let until = std::time::Instant::now() + std::time::Duration::from_millis(3);
            while std::time::Instant::now() < until {
                std::hint::spin_loop();
            }
            fs::rename(from, to)
        };
        for n in 1..40 {
            write_atomic_via(
                &path,
                n.to_string().as_bytes(),
                Mode::Default,
                delete_then_rename,
            )
            .unwrap();
        }
        done.store(true, Ordering::Relaxed);
        let (opens, rode_out) = reader.join().unwrap();
        assert!(opens > 0);
        // The test cannot pass by never meeting the gap: the reader met the missing name and waited it out. The window
        // is 3 ms in each of 39 replaces, the reader opens continuously, and its first retries wait 1 ms and 2 ms: a
        // hit takes two or three retries to recover, well inside its 250 ms budget.
        assert!(
            rode_out > 0,
            "the reader never met the missing name in {opens} opens"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "39");
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
    }

    /// What a caller sees when the replace is refused for good (Windows: the target or the new file held without delete
    /// sharing for longer than the retries): an `Err` of the OS error, the old file whole under its name, no temp file
    /// left. Every caller of `write_atomic` already takes an `io::Result` (propagated as the user's error, or dropped
    /// by a best-effort writer that writes again on its next tick), so a lasting refusal is that and nothing worse.
    #[test]
    fn a_lasting_refusal_is_an_error_for_the_caller_and_costs_nothing_else() {
        use crate::sys::{RetryBudget, planned_waits, refusal};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_atomic(&path, b"old", Mode::Default).unwrap();
        refusal::refuse_with(5); // ERROR_ACCESS_DENIED, every time, on every OS
        let err = write_atomic(&path, b"new", Mode::Default).unwrap_err();
        let tries = refusal::tries();
        refusal::lift();
        assert_eq!(err.raw_os_error(), Some(5));
        // A write whose loss matters keeps trying for the whole full budget (about 1.2 s) before it fails.
        assert_eq!(
            tries as usize,
            planned_waits(5, RetryBudget::Full).len() + 1
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
        // The next write, the refusal gone, lands.
        write_atomic(&path, b"new", Mode::Default).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    /// The per-tick snapshots (`.tui-status.json`, `.tui-metrics.json`) are rewritten by the next tick anyway, so under
    /// a lasting refusal their write gives up after the brief budget, counted in attempts (a few, about 63 ms of
    /// waits) and not in wall time, instead of stalling the runtime manager's tick for 1.2 s. The old file stays whole
    /// and the next snapshot lands.
    #[test]
    fn a_snapshot_write_under_a_lasting_refusal_gives_up_after_the_brief_budget_and_the_next_one_lands()
     {
        use crate::sys::{RetryBudget, planned_waits, refusal};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".tui-status.json");
        write_atomic(&path, b"old", Mode::Default).unwrap();
        refusal::refuse_with(5);
        let err = write_atomic_snapshot(&path, b"new", Mode::Default).unwrap_err();
        let tries = refusal::tries();
        refusal::lift();
        assert_eq!(err.raw_os_error(), Some(5));
        assert_eq!(
            tries as usize,
            planned_waits(5, RetryBudget::Brief).len() + 1
        );
        let waited: std::time::Duration = planned_waits(5, RetryBudget::Brief).iter().sum();
        assert!(
            waited < std::time::Duration::from_millis(300),
            "{tries} attempts, {waited:?} of waits"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
        write_atomic_snapshot(&path, b"new", Mode::Default).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    /// A file nobody is replacing is "not found" (the status of a project that never ran), also beside the temp file
    /// of another file; the retries are for a replace in flight only.
    #[test]
    fn a_missing_file_with_no_replace_in_flight_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.json");
        fs::write(dir.path().join("other.json.tmp-1-0"), "x").unwrap();
        assert_eq!(
            read_replaced(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!replace_in_flight(&path));
        fs::write(dir.path().join("status.json.tmp-1-0"), "x").unwrap();
        assert!(replace_in_flight(&path));
        assert!(!replace_in_flight(&dir.path().join("status")));
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
