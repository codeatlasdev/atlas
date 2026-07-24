use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::Path;

#[derive(Debug)]
pub struct SingletonLock {
    _file: File,
}

impl SingletonLock {
    pub fn acquire(lock_path: &Path) -> anyhow::Result<Self> {
        let file = File::create(lock_path)?;
        let fd = file.as_raw_fd();

        // Use libc::flock directly (nix 0.28+ deprecated nix::fcntl::flock)
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            return Err(anyhow::anyhow!("another atlas dev instance is running"));
        }

        // Write our PID
        std::fs::write(lock_path, format!("{}", std::process::id()))?;
        Ok(Self { _file: file })
    }
}

impl Drop for SingletonLock {
    fn drop(&mut self) {
        // Lock is released automatically when file descriptor is closed
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

        // File should contain our PID
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
        assert!(err.to_string().contains("another atlas dev instance is running"));
    }
}
