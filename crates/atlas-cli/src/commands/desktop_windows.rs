//! Windows side of `atlas desktop`: the app ships as a per-user installer
//! (`Atlas-<version>-<platform>-setup.exe`, NSIS), run silently into `%LOCALAPPDATA%\Atlas`: no
//! admin rights, a Start menu entry, and an uninstaller in Settings › Apps like any Windows app.

use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use atlas_core::sys::CommandExt as _;

/// Where the installer registers itself (per user), with the installed version as
/// `DisplayVersion`: the Tauri installer's uninstall key, named after the product.
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Atlas";

/// Recorded by `install`, for a `--from` install whose installer did not register (or a registry
/// that cannot be read).
fn version_file() -> PathBuf {
    atlas_core::paths::atlas_home()
        .join("desktop")
        .join("app-version")
}

/// The installed version: the installer's own record, else the one `install` wrote.
pub(super) fn app_version(app: &Path) -> Option<String> {
    if super::desktop::is_symlink(app) || !app.is_file() {
        return None;
    }
    registered_version().or_else(|| {
        let v = std::fs::read_to_string(version_file()).ok()?;
        Some(v.trim().to_string()).filter(|v| !v.is_empty())
    })
}

fn registered_version() -> Option<String> {
    let out = Command::new("reg")
        .args(["query", UNINSTALL_KEY, "/v", "DisplayVersion"])
        .no_window()
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| reg_value(&String::from_utf8_lossy(&out.stdout), "DisplayVersion"))
        .flatten()
}

/// The data of `name` in `reg query` output (`    DisplayVersion    REG_SZ    1.5.0`).
fn reg_value(out: &str, name: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        (f.next()? == name && f.next()?.starts_with("REG_"))
            .then(|| f.collect::<Vec<_>>().join(" "))
            .filter(|v| !v.is_empty())
    })
}

/// Run the installer `source` silently with its install dir set to `target`'s folder, wait for
/// it, and check the app is there. The installer closes a running Atlas first and replaces the
/// files in place.
pub(super) fn install(source: &Path, target: &Path, version: Option<&str>) -> Result<()> {
    if !is_pe(source) {
        bail!(
            "{} is not a Windows installer (expected an .exe)",
            source.display()
        );
    }
    let dir = target.parent().context("bad install path")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    // NSIS: `/S` silent; `/D=` must come last and unquoted, even with spaces.
    let st = Command::new(source)
        .arg("/S")
        .raw_arg(format!("/D={}", dir.display()))
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("cannot run {}", source.display()))?;
    if !st.success() {
        bail!("the Atlas installer failed ({st})");
    }
    if !target.is_file() {
        bail!("the installer finished but {} is missing", target.display());
    }
    let vf = version_file();
    match version {
        Some(v) => {
            if let Some(d) = vf.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&vf, v)?;
        }
        None => {
            let _ = std::fs::remove_file(&vf);
        }
    }
    Ok(())
}

/// `MZ`: a Windows executable, not a truncated download or an HTML error page.
fn is_pe(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 2];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok_and(|()| magic == *b"MZ")
}

/// Start the app detached from this console, so closing it or Ctrl-C does not take the app down.
pub(super) fn open(app: &Path) -> Result<()> {
    let mut cmd = Command::new(app);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    atlas_core::sys::spawn_detached(&mut cmd)
        .with_context(|| format!("cannot start {}", app.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_version_is_read_from_reg_query_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Atlas\r\n    DisplayVersion    REG_SZ    1.5.0-alpha.6\r\n\r\n";
        assert_eq!(
            reg_value(out, "DisplayVersion").as_deref(),
            Some("1.5.0-alpha.6")
        );
        assert_eq!(reg_value(out, "Publisher"), None);
        assert_eq!(reg_value("", "DisplayVersion"), None);
        // A value line without data, or a header that merely mentions the name, is not a value.
        assert_eq!(
            reg_value("    DisplayVersion    REG_SZ\r\n", "DisplayVersion"),
            None
        );
        assert_eq!(
            reg_value("HKEY_X\\DisplayVersion\r\n", "DisplayVersion"),
            None
        );
    }

    /// A truncated download or an HTML error page must never be run as the installer.
    #[test]
    fn install_refuses_a_file_that_is_not_a_windows_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Atlas").join("atlas-desktop.exe");
        for bad in [b"<html>404</html>".as_slice(), b"M", b""] {
            let p = tmp.path().join("setup.exe");
            std::fs::write(&p, bad).unwrap();
            let e = install(&p, &target, Some("1.0.0")).unwrap_err();
            assert!(e.to_string().contains("not a Windows installer"), "{e}");
        }
        assert!(!target.exists());
    }
}
