//! The app's stderr goes to `<base>/desktop/stderr.log` when nobody watches it (#273).
//!
//! Started from the launcher or `atlas desktop`, the app's fd 2 is `/dev/null`, so every line
//! that explained #265 (`Could not create default EGL display: EGL_BAD_PARAMETER` from the web
//! process, `GStreamer element appsrc not found`, `Gtk-CRITICAL …`) was lost. Early in `main`
//! fd 2 is pointed at the log; GTK, WebKit and the web processes (which inherit fd 2) write
//! there, and a crash loses nothing (no pipe, no thread for the data). On a terminal
//! (`desktop:dev`, a hand run) stderr is left alone so it stays live. Linux only: that is where
//! WebKitGTK prints what explains a blank window.
//!
//! The log is rotated past [`MAX`] at startup and then every [`CHECK_EVERY`] while the app runs,
//! by copying to `stderr.log.1` and truncating in place: every `O_APPEND` writer (this app's fd 2,
//! its web processes, a second launch that exits through single-instance) keeps writing into the
//! current `stderr.log`, which is what diagnostics read.

use std::path::{Path, PathBuf};

/// Past this size the log moves to `stderr.log.1` (one generation).
#[cfg(target_os = "linux")]
pub const MAX: u64 = 2 * 1024 * 1024;

/// How often a running app re-checks the size.
#[cfg(target_os = "linux")]
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// `<base>/desktop/stderr.log`.
pub fn path(base: &Path) -> PathBuf {
    base.join("desktop").join("stderr.log")
}

/// Copy a log at or past `max` to `<name>.1` (replacing the previous one), then truncate it in
/// place. Not a rename: the running app's descriptor would follow the file to `.1`.
#[cfg(target_os = "linux")]
fn rotate(p: &Path, max: u64) {
    if std::fs::metadata(p).is_ok_and(|m| m.len() >= max) {
        let mut old = p.as_os_str().to_os_string();
        old.push(".1");
        if std::fs::copy(p, PathBuf::from(old)).is_ok() {
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .open(p)
                .and_then(|f| f.set_len(0));
        }
    }
}

/// Re-check the log's size every `every`, for the life of the process.
#[cfg(target_os = "linux")]
fn spawn_trimmer(p: PathBuf, max: u64, every: std::time::Duration) {
    let _ = std::thread::Builder::new()
        .name("stderr-log-trim".into())
        .spawn(move || {
            loop {
                std::thread::sleep(every);
                rotate(&p, max);
            }
        });
}

/// Point `fd` at the log under `base` (append), unless `fd` is a terminal. `Ok(true)` when it
/// now writes to the log.
#[cfg(target_os = "linux")]
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

/// Whether this process captures its stderr. `env` = `ATLAS_AGENT`: it is `<agent id>` in every
/// agent terminal (so `atlas desktop` typed there is the user's app), and `1` only for an agent
/// instance, whose launcher already points its stderr at that instance's `app.log`.
#[cfg(target_os = "linux")]
fn wanted(env: Option<&str>, debug_build: bool) -> bool {
    crate::agent_mode::Mode::resolve(env, debug_build) != crate::agent_mode::Mode::Agent
}

/// Capture this process's stderr (fd 2) and keep the log bounded. Errors are not fatal: the app
/// runs with what it had.
pub fn install() {
    #[cfg(target_os = "linux")]
    if wanted(
        std::env::var(crate::agent_mode::ENV).ok().as_deref(),
        cfg!(debug_assertions),
    ) {
        match capture_into(&atlas_host::paths::base(), 2, MAX) {
            Ok(true) => spawn_trimmer(path(&atlas_host::paths::base()), MAX, CHECK_EVERY),
            Ok(false) => {}
            Err(e) => crate::diag::log(&format!("stderr log: {e}")),
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
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

    /// A running app holds an O_APPEND fd on stderr.log. Rotation must not move the file away
    /// from it (a second launch that exits through single-instance rotates too): the live writer
    /// keeps landing in the current stderr.log, which diagnostics read.
    #[test]
    fn an_append_writer_keeps_writing_into_the_current_log_after_rotation() {
        let base = tempfile::tempdir().unwrap();
        let p = path(base.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut w = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .unwrap();
        writeln!(w, "before rotation, long enough to pass the limit").unwrap();
        rotate(&p, 16);
        writeln!(w, "after rotation").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "after rotation\n");
        let old = std::fs::read_to_string(p.with_extension("log.1")).unwrap();
        assert_eq!(old, "before rotation, long enough to pass the limit\n");
    }

    /// The size check is not only at startup: a long-running app cannot grow the log unbounded.
    #[test]
    fn the_trimmer_rotates_a_log_that_grows_while_the_app_runs() {
        let base = tempfile::tempdir().unwrap();
        let p = path(base.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut w = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .unwrap();
        spawn_trimmer(p.clone(), 64, std::time::Duration::from_millis(10));
        w.write_all(&[b'x'; 128]).unwrap();
        let old = p.with_extension("log.1");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !old.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(std::fs::metadata(&old).unwrap().len(), 128);
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 0);
    }

    /// `ATLAS_AGENT` is `<agent id>` in every agent terminal (agent/launch.rs), so `atlas desktop`
    /// typed there is the user's app and keeps its stderr; only the agent-instance mode skips.
    #[test]
    fn only_an_agent_instance_skips_the_capture() {
        assert!(wanted(None, true));
        assert!(
            wanted(Some("a1b2c3"), true),
            "an agent terminal's id is not agent mode"
        );
        assert!(
            !wanted(Some("1"), true),
            "an agent instance logs to its own app.log"
        );
        assert!(
            wanted(Some("1"), false),
            "a release app ignores the variable"
        );
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
