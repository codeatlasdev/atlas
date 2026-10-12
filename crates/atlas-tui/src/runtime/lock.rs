use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Pure: is this (lowercase) executable path / process name the atlas binary?
/// Exact basename match — never a substring of the directory path.
fn exe_is_atlas(exe: &str) -> bool {
    exe.rsplit('/').next() == Some("atlas")
}

#[derive(Debug)]
pub struct SingletonLock {
    _file: File,
}

impl SingletonLock {
    /// Try to acquire the lock without killing anything.
    /// Returns Err if another live instance holds the lock.
    pub fn acquire(lock_path: &Path) -> anyhow::Result<Self> {
        use std::io::Write;
        // No truncate: a failed flock must leave the holder's pid intact.
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        let fd = file.as_raw_fd();

        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            return Err(anyhow::anyhow!("another atlas dev instance is running"));
        }

        file.set_len(0)?;
        file.write_all(std::process::id().to_string().as_bytes())?;
        file.flush()?;
        Ok(Self { _file: file })
    }

    /// Returns the PID of a live prior instance, or None.
    /// Safe to call before raw mode is enabled.
    pub fn existing_pid(lock_path: &Path) -> Option<libc::pid_t> {
        let our_pid = std::process::id() as libc::pid_t;
        let our_ppid = unsafe { libc::getppid() };
        let contents = std::fs::read_to_string(lock_path).ok()?;
        let pid = contents.trim().parse::<libc::pid_t>().ok()?;
        if pid <= 0 || pid == our_pid || pid == our_ppid {
            return None;
        }
        // kill -0: probe existence without sending a signal
        let alive = unsafe { libc::kill(pid, 0) == 0 };
        if !alive {
            return None;
        }
        // Verify the candidate is actually an atlas process, not a recycled PID.
        // On macOS use proc_pidpath; on Linux read /proc/<pid>/comm.
        let is_atlas = Self::pid_is_atlas(pid);
        if is_atlas { Some(pid) } else { None }
    }

    /// Returns true if `pid` is an atlas process (executable basename exactly
    /// `atlas`). Prevents killing unrelated processes whose PID was recycled from
    /// a previous atlas run — including binaries that merely live under a path
    /// containing "atlas" (e.g. `…/atlas-org/app/…/turbo`).
    fn pid_is_atlas(pid: libc::pid_t) -> bool {
        exe_is_atlas(&crate::runtime::process::proc_name(pid))
    }

    /// Kill a prior instance gracefully (SIGTERM → 3s → SIGKILL).
    pub async fn kill_prior(pid: libc::pid_t) {
        crate::runtime::process::kill_exact_gracefully(pid, std::time::Duration::from_secs(3))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_singleton_lock_acquire() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("atlas.lock");

        let lock = SingletonLock::acquire(&lock_path);
        assert!(lock.is_ok());

        let contents = std::fs::read_to_string(&lock_path).unwrap();
        assert_eq!(contents, format!("{}", std::process::id()));
    }

    #[test]
    fn test_singleton_lock_double_acquire_fails() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("atlas.lock");

        let _lock1 = SingletonLock::acquire(&lock_path).unwrap();
        let lock2 = SingletonLock::acquire(&lock_path);

        assert!(lock2.is_err());
        let err = lock2.unwrap_err();
        assert!(
            err.to_string()
                .contains("another atlas dev instance is running")
        );
    }

    #[test]
    fn test_existing_pid_no_lock_file() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("atlas.lock");
        assert_eq!(SingletonLock::existing_pid(&lock_path), None);
    }

    #[test]
    fn exe_is_atlas_matches_basename_only() {
        assert!(exe_is_atlas("/home/u/.atlas/bin/atlas"));
        assert!(exe_is_atlas("atlas"));
        assert!(!exe_is_atlas(""));
        assert!(!exe_is_atlas(
            "/users/m/projects/atlas-org/app/node_modules/turbo"
        ));
        assert!(!exe_is_atlas("/users/m/atlas-org/app/apps/api/tmp/api"));
        assert!(!exe_is_atlas("/usr/local/bin/atlas-mcp-helper"));
        assert!(!exe_is_atlas("/opt/atlas/bin/"));
    }
}
