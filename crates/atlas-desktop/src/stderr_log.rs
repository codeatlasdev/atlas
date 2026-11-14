//! The app's stderr goes to `<base>/desktop/stderr.log` when nobody watches it (#273).
//!
//! Started from the launcher or `atlas desktop`, the app's fd 2 is `/dev/null`, so every line
//! that explained #265 (`Could not create default EGL display: EGL_BAD_PARAMETER` from the web
//! process, `GStreamer element appsrc not found`, `Gtk-CRITICAL …`) was lost. Early in `main`
//! fd 2 is pointed at the log; GTK, WebKit and the web processes (which inherit fd 2) write
//! there, and a crash loses nothing (no pipe, no thread). On a terminal (`desktop:dev`, a hand
//! run) stderr is left alone so it stays live. The log is rotated at startup past [`MAX`].

use std::path::{Path, PathBuf};

/// Past this size at startup the log moves to `stderr.log.1` (one generation).
pub const MAX: u64 = 2 * 1024 * 1024;

/// `<base>/desktop/stderr.log`.
pub fn path(base: &Path) -> PathBuf {
    base.join("desktop").join("stderr.log")
}

/// Move a log at or past `max` to `<name>.1`, replacing the previous one.
fn rotate(p: &Path, max: u64) {
    if std::fs::metadata(p).is_ok_and(|m| m.len() >= max) {
        let mut old = p.as_os_str().to_os_string();
        old.push(".1");
        let _ = std::fs::rename(p, PathBuf::from(old));
    }
}

/// Point `fd` at the log under `base` (append), unless `fd` is a terminal. `Ok(true)` when it
/// now writes to the log.
#[cfg(unix)]
pub fn capture_into(base: &Path, fd: std::os::fd::RawFd, max: u64) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    // SAFETY: isatty only inspects the descriptor.
    if unsafe { libc::isatty(fd) } == 1 {
        return Ok(false);
    }
    let p = path(base);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    rotate(&p, max);
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    // SAFETY: both descriptors are valid; dup2 replaces `fd` atomically and `f` closes after.
    if unsafe { libc::dup2(f.as_raw_fd(), fd) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(true)
}

/// Capture this process's stderr (fd 2). Not for agent instances, whose launcher reads their
/// stderr through a pipe. Errors are not fatal: the app runs with what it had.
pub fn install() {
    #[cfg(unix)]
    if std::env::var_os("ATLAS_AGENT").is_none()
        && let Err(e) = capture_into(&atlas_host::paths::base(), 2, MAX)
    {
        crate::diag::log(&format!("stderr log: {e}"));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::AsRawFd;

    /// #273: a child that inherits the redirected descriptor (a web process inherits fd 2) writes
    /// into the log, and so does this process.
    #[test]
    fn writes_to_the_captured_descriptor_and_a_childs_land_in_the_log() {
        let base = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempfile().unwrap(); // stands in for fd 2 = /dev/null
        let fd = scratch.as_raw_fd();
        assert!(capture_into(base.path(), fd, MAX).unwrap());
        let mut w =
            unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(libc::dup(fd)) };
        writeln!(w, "Gtk-CRITICAL: from the app").unwrap();
        let st = std::process::Command::new("sh")
            .args(["-c", "echo 'EGL_BAD_PARAMETER from a child' >&3"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_with_fd3(fd);
        assert!(st.success());
        let log = std::fs::read_to_string(path(base.path())).unwrap();
        assert!(log.contains("Gtk-CRITICAL: from the app"), "{log}");
        assert!(log.contains("EGL_BAD_PARAMETER from a child"), "{log}");
    }

    #[test]
    fn rotates_at_startup_past_the_limit_keeping_one_generation() {
        let base = tempfile::tempdir().unwrap();
        let p = path(base.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![b'x'; 64]).unwrap();
        let scratch = tempfile::tempfile().unwrap();
        assert!(capture_into(base.path(), scratch.as_raw_fd(), 64).unwrap());
        let old = std::fs::read(p.with_extension("log.1")).unwrap();
        assert_eq!(old.len(), 64, "the full log moved aside");
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 0, "a fresh log");
    }

    #[test]
    fn a_small_log_is_appended_to_not_rotated() {
        let base = tempfile::tempdir().unwrap();
        let p = path(base.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "earlier run\n").unwrap();
        let scratch = tempfile::tempfile().unwrap();
        capture_into(base.path(), scratch.as_raw_fd(), MAX).unwrap();
        assert!(!p.with_extension("log.1").exists());
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .starts_with("earlier run\n")
        );
    }

    trait Fd3 {
        fn spawn_with_fd3(&mut self, fd: std::os::fd::RawFd) -> std::process::ExitStatus;
    }
    impl Fd3 for std::process::Command {
        /// Run with `fd` as the child's fd 3 (how a web process inherits the app's fd 2).
        fn spawn_with_fd3(&mut self, fd: std::os::fd::RawFd) -> std::process::ExitStatus {
            use std::os::unix::process::CommandExt;
            // SAFETY: dup2 is async-signal-safe; `fd` stays open in the parent until wait returns.
            unsafe {
                self.pre_exec(move || {
                    if libc::dup2(fd, 3) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            self.status().unwrap()
        }
    }
}
