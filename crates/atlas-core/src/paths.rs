//! Well-known locations and executable lookup shared by the CLI, TUI and desktop app.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `~/.atlas` (falls back to `/.atlas` when there is no home directory).
pub fn atlas_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".atlas")
}

/// First executable named `name` in the `PATH`-style list `path`.
pub fn which_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

/// First executable named `name` on `$PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &std::env::var_os("PATH")?)
}

pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `/Applications/Atlas.app`, then `~/Applications/Atlas.app` (symlinks count).
pub fn installed_app() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    [PathBuf::from("/Applications"), home.join("Applications")]
        .into_iter()
        .map(|d| d.join("Atlas.app"))
        .find(|p| p.symlink_metadata().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_in_finds_only_executables() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("tool");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(which_in("tool", &path), None);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(which_in("tool", &path), Some(exe));
        assert_eq!(which_in("missing", &path), None);
    }
}
