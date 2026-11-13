//! Copy-on-write clone of a directory tree (`sys::clone_dir`): the blocks are shared until one
//! side writes, so cloning a 3 GB `target/` costs milliseconds and no disk, and writing the clone
//! never changes the source (unlike a hardlink tree, which would).
//!
//! macOS: `clonefile(2)` on the whole directory (APFS). Linux: `FICLONE` per file on a reflink
//! file system (btrfs, XFS with reflink, bcachefs). Anywhere else, or across volumes, the result
//! is [`io::ErrorKind::Unsupported`] and nothing is left at `dst`: callers skip, they never fall
//! back to a full copy. Windows block cloning (ReFS) needs a per-volume probe and
//! `FSCTL_DUPLICATE_EXTENTS_TO_FILE` per file; it is not attempted, so Windows reports
//! `Unsupported`.

use std::io;
use std::path::Path;

/// Clone the directory `src` to the new path `dst` (whose parent must exist). Fails with
/// `AlreadyExists` when `dst` exists and `Unsupported` when the file system cannot clone; in
/// every error case no partial `dst` remains.
pub fn clone_dir(src: &Path, dst: &Path) -> io::Result<()> {
    imp::clone_dir(src, dst)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    /// `<sys/clonefile.h>`: do not follow a symlink at `src`.
    const CLONE_NOFOLLOW: u32 = 0x0001;

    unsafe extern "C" {
        fn clonefile(src: *const libc::c_char, dst: *const libc::c_char, flags: u32)
        -> libc::c_int;
    }

    pub fn clone_dir(src: &Path, dst: &Path) -> io::Result<()> {
        let c = |p: &Path| {
            CString::new(p.as_os_str().as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
        };
        let (s, d) = (c(src)?, c(dst)?);
        // SAFETY: both pointers are valid NUL-terminated strings for the call's duration.
        if unsafe { clonefile(s.as_ptr(), d.as_ptr(), CLONE_NOFOLLOW) } == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        Err(match e.raw_os_error() {
            // Not APFS, or src and dst on different volumes.
            Some(libc::ENOTSUP | libc::EXDEV) => io::ErrorKind::Unsupported.into(),
            _ => e,
        })
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::os::unix::io::AsRawFd;

    /// `_IOW(0x94, 9, int)` from `<linux/fs.h>`.
    const FICLONE: libc::c_ulong = 0x4004_9409;

    pub fn clone_dir(src: &Path, dst: &Path) -> io::Result<()> {
        if dst.symlink_metadata().is_ok() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let r = tree(src, dst);
        if r.is_err() {
            let _ = fs::remove_dir_all(dst);
        }
        r
    }

    fn tree(src: &Path, dst: &Path) -> io::Result<()> {
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let (from, to) = (entry.path(), dst.join(entry.file_name()));
            let ty = entry.file_type()?;
            if ty.is_dir() {
                tree(&from, &to)?;
            } else if ty.is_symlink() {
                symlink(fs::read_link(&from)?, &to)?;
            } else if ty.is_file() {
                file(&from, &to)?;
            }
            // Sockets, fifos and devices have no place in a dependency tree: left out.
        }
        fs::set_permissions(dst, fs::metadata(src)?.permissions())
    }

    fn file(from: &Path, to: &Path) -> io::Result<()> {
        let input = fs::File::open(from)?;
        let output = fs::File::options().write(true).create_new(true).open(to)?;
        // SAFETY: both fds are open for the call; FICLONE takes the source fd as its argument.
        if unsafe { libc::ioctl(output.as_raw_fd(), FICLONE, input.as_raw_fd()) } != 0 {
            let e = io::Error::last_os_error();
            return Err(match e.raw_os_error() {
                // No reflink on this file system (ext4, tmpfs), another volume, or not a regular file.
                Some(libc::EOPNOTSUPP | libc::EXDEV | libc::EINVAL | libc::ENOTTY) => {
                    io::ErrorKind::Unsupported.into()
                }
                _ => e,
            });
        }
        output.set_permissions(fs::Permissions::from_mode(
            input.metadata()?.permissions().mode(),
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::*;

    pub fn clone_dir(_src: &Path, _dst: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn an_existing_destination_is_refused_and_left_untouched() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("s"), t.path().join("d"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("mine"), "1").unwrap();
        let e = clone_dir(&src, &dst).unwrap_err();
        // Unsupported (non-cloning volume) is also a refusal; what matters is dst is intact.
        assert!(matches!(
            e.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::Unsupported
        ));
        assert_eq!(fs::read_to_string(dst.join("mine")).unwrap(), "1");
    }

    #[test]
    fn a_missing_source_is_an_error_and_creates_nothing() {
        let t = tempfile::tempdir().unwrap();
        let dst = t.path().join("d");
        assert!(clone_dir(&t.path().join("nope"), &dst).is_err());
        assert!(dst.symlink_metadata().is_err());
    }

    /// The fallback contract: whatever the volume, the call either clones completely or leaves
    /// no `dst` and says `Unsupported`; it never leaves half a tree and never copies in full.
    #[test]
    fn clone_is_all_or_nothing() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("s"), t.path().join("d"));
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("a/b/f.txt"), "data").unwrap();
        fs::write(src.join("top.txt"), "top").unwrap();
        match clone_dir(&src, &dst) {
            Ok(()) => {
                assert_eq!(fs::read_to_string(dst.join("a/b/f.txt")).unwrap(), "data");
                assert_eq!(fs::read_to_string(dst.join("top.txt")).unwrap(), "top");
            }
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::Unsupported, "{e}");
                assert!(dst.symlink_metadata().is_err(), "partial tree left behind");
            }
        }
    }

    /// An empty directory has no file to reflink: it must clone (or be unsupported), not error.
    #[test]
    fn an_empty_directory_clones_or_is_unsupported() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("s"), t.path().join("d"));
        fs::create_dir_all(&src).unwrap();
        match clone_dir(&src, &dst) {
            Ok(()) => assert!(dst.is_dir()),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::Unsupported, "{e}"),
        }
    }
}
