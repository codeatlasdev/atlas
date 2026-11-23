//! What installing and uninstalling Atlas does differently per OS, behind `sys::` so `atlas install`
//! and `atlas uninstall` carry no `cfg` of their own: marking a file executable, replacing a program
//! file that may be running, moving the running executable out of the way, and the user's `Path`.

use std::io;
use std::path::Path;

/// Mark `path` runnable: mode 0755 on Unix. Windows runs a file by its extension; nothing to do.
pub fn make_executable(path: &Path) -> io::Result<()> {
    imp::make_executable(path)
}

/// Put `from` (a file in the same folder as `to`, so the swap is atomic) at the program path `to`.
///
/// Unix: `rename(2)` over it, which works while the old file runs. Windows: a running `.exe`
/// cannot be overwritten or deleted, only renamed. So an existing file is moved aside to
/// `.atlas-old-<pid>.exe`, the new one takes its place, and the aside file is deleted right away;
/// when it is still running that fails and the next replace in that folder sweeps it. If `from`
/// cannot be put in place the old file is moved back, so there is never no program at `to`.
pub fn replace_program(from: &Path, to: &Path) -> io::Result<()> {
    imp::replace_program(from, to)
}

/// Windows: move the running executable `exe` (this one, or `atlas hostd`) out of the way to the
/// temp dir, where nothing refers to it, because it cannot be deleted while it runs. A no-op elsewhere
/// (Unix deletes a running file).
pub fn move_running_exe_aside(exe: &Path) {
    imp::move_running_exe_aside(exe);
}

/// Whether `dir` is on the user's `Path` (Windows: the registry's `HKCU\Environment`). Reads only.
/// Always `false` elsewhere: shells read their own startup files, not a user `Path`.
pub fn user_path_has(dir: &Path) -> io::Result<bool> {
    imp::edit_user_path(dir, "has")
}

/// Put `dir` on the user's `Path`, the way Windows installers do, so new terminals find it.
/// `Ok(false)` when it is already there (and always elsewhere, where nothing is written).
pub fn user_path_add(dir: &Path) -> io::Result<bool> {
    imp::edit_user_path(dir, "add")
}

/// Take `dir` off the user's `Path`. `Ok(false)` when it was not there (and always elsewhere).
pub fn user_path_remove(dir: &Path) -> io::Result<bool> {
    imp::edit_user_path(dir, "remove")
}

#[cfg(unix)]
mod imp {
    use std::io;
    use std::path::Path;

    pub fn make_executable(path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }

    pub fn replace_program(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    pub fn move_running_exe_aside(_exe: &Path) {}

    pub fn edit_user_path(_dir: &Path, _op: &str) -> io::Result<bool> {
        Ok(false)
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::path::Path;

    use crate::sys::CommandExt as _;

    pub fn make_executable(_path: &Path) -> io::Result<()> {
        Ok(())
    }

    pub fn replace_program(from: &Path, to: &Path) -> io::Result<()> {
        let dir = to.parent().unwrap_or_else(|| Path::new("."));
        // Only a file is moved aside: anything else at `to` is left for the rename to refuse.
        let aside = {
            let aside = dir.join(format!(".atlas-old-{}.exe", std::process::id()));
            sweep_old(dir);
            (to.is_file() && std::fs::rename(to, &aside).is_ok()).then_some(aside)
        };
        let installed = std::fs::rename(from, to);
        if let Some(aside) = aside {
            if installed.is_ok() {
                let _ = std::fs::remove_file(&aside);
            } else {
                let _ = std::fs::rename(&aside, to);
            }
        }
        installed
    }

    /// Delete the `.atlas-old-<pid>.exe` files earlier replaces moved aside; one still in use
    /// refuses and waits for the next run.
    fn sweep_old(dir: &Path) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(".atlas-old-") && name.ends_with(".exe") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    pub fn move_running_exe_aside(exe: &Path) {
        if exe.exists() {
            let aside =
                std::env::temp_dir().join(format!("atlas-uninstalled-{}.exe", std::process::id()));
            let _ = std::fs::rename(exe, aside);
        }
    }

    /// Runs the one PowerShell script behind add/remove/has. Its answer is `changed` (written),
    /// `same` (nothing to do) or, for `has`, `yes`/`no`; `Ok(true)` is `changed` or `yes`.
    /// PowerShell's `SetEnvironmentVariable` writes the full value (never `setx`, which truncates
    /// at 1024 characters) and tells running programs the environment changed.
    pub fn edit_user_path(dir: &Path, op: &str) -> io::Result<bool> {
        // The dir travels in the environment, never inside the script text.
        const SCRIPT: &str = "$d = $env:ATLAS_PATH_DIR; $op = $env:ATLAS_PATH_OP; \
            $cur = [Environment]::GetEnvironmentVariable('Path', 'User'); \
            $parts = @(if ($cur) { $cur -split ';' | Where-Object { $_ -ne '' } }); \
            $has = @($parts | Where-Object { $_.TrimEnd('\\') -ieq $d.TrimEnd('\\') }).Count -gt 0; \
            if ($op -eq 'has') { if ($has) { 'yes' } else { 'no' }; exit }; \
            if ($op -eq 'add') { if ($has) { 'same'; exit } ; $parts += $d } \
            else { if (-not $has) { 'same'; exit } ; $parts = @($parts | Where-Object { $_.TrimEnd('\\') -ine $d.TrimEnd('\\') }) } \
            [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User'); 'changed'";
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("ATLAS_PATH_DIR", dir)
            .env("ATLAS_PATH_OP", op)
            .no_window()
            .stdin(std::process::Stdio::null())
            .output()?;
        if !out.status.success() {
            return Err(io::Error::other(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ));
        }
        Ok(matches!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "changed" | "yes"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_staged_binary_becomes_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("atlas");
        std::fs::write(&f, "bin").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        make_executable(&f).unwrap();
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn replacing_a_program_swaps_the_content_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("staged"), dir.path().join("atlas.exe"));
        std::fs::write(&to, "old").unwrap();
        std::fs::write(&from, "new").unwrap();
        replace_program(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
        let mut left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["atlas.exe"]);
    }

    /// Unix shells read their own startup files: there is no user `Path` to read or write, and
    /// asking must not fail or touch anything (uninstall asks on every OS).
    #[cfg(unix)]
    #[test]
    fn there_is_no_user_path_off_windows() {
        let dir = Path::new("/home/u/.atlas/bin");
        assert!(!user_path_has(dir).unwrap());
        assert!(!user_path_add(dir).unwrap());
        assert!(!user_path_remove(dir).unwrap());
    }

    #[test]
    fn moving_a_missing_exe_aside_is_a_no_op() {
        move_running_exe_aside(Path::new("/no/such/dir/atlas.exe"));
    }

    /// Windows, with the real registry of the machine: an entry that is not on the user `Path` is
    /// added, seen, and removed again, and `Path` is what it was.
    #[cfg(windows)]
    #[test]
    fn the_user_path_round_trips_through_the_registry() {
        let dir = std::env::temp_dir().join(format!("atlas-user-path-test-{}", std::process::id()));
        assert!(!user_path_has(&dir).unwrap());
        assert!(user_path_add(&dir).unwrap(), "first add changes the Path");
        assert!(user_path_has(&dir).unwrap());
        assert!(!user_path_add(&dir).unwrap(), "second add is a no-op");
        assert!(user_path_remove(&dir).unwrap());
        assert!(!user_path_has(&dir).unwrap());
        assert!(!user_path_remove(&dir).unwrap(), "second remove is a no-op");
    }
}
