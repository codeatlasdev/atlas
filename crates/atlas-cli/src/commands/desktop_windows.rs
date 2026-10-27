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
/// it, and check the app is there at `version`. The installer closes a running Atlas Desktop and
/// writes the files in place; the previous files are kept aside until the new ones are verified
/// and put back when the installer fails or leaves another version behind.
pub(super) fn install(source: &Path, target: &Path, version: Option<&str>) -> Result<()> {
    if !is_pe(source) {
        bail!(
            "{} is not a Windows installer (expected an .exe)",
            source.display()
        );
    }
    // The silent installer terminates a running Atlas Desktop: bring it back afterwards (the terminal
    // host and the stacks keep running on their own, so it reattaches where it was).
    let was_running = app_running(target);
    install_with(
        target,
        version,
        &version_file(),
        |dir| run_installer(source, dir),
        registered_version,
    )?;
    if was_running && !app_running(target) {
        let _ = open(target, &[]);
    }
    Ok(())
}

/// An Atlas Desktop runs from `target` (not a dev build elsewhere with the same image name).
fn app_running(target: &Path) -> bool {
    let Some(name) = target
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
    else {
        return false;
    };
    let want = target.to_string_lossy().to_lowercase().replace('/', "\\");
    atlas_core::sys::process_rows().iter().any(|r| {
        r.exe.to_lowercase() == name
            && atlas_core::sys::command_line(r.pid)
                .is_some_and(|c| c.to_lowercase().replace('/', "\\").contains(&want))
    })
}

/// NSIS: `/S` silent; `/D=` must come last and unquoted, even with spaces.
fn run_installer(source: &Path, dir: &Path) -> Result<()> {
    let st = Command::new(source)
        .arg("/S")
        .raw_arg(format!("/D={}", dir.display()))
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("cannot run {}", source.display()))?;
    if !st.success() {
        bail!("the Atlas installer failed ({st})");
    }
    Ok(())
}

/// Suffix of a file moved aside by the install run of process `pid`: `atlas.exe` →
/// `atlas.exe.atlas-old-1234`.
fn aside_suffix(pid: u32) -> String {
    format!(".atlas-old-{pid}")
}

/// `(original name, run pid)` when `name` is a file some install run moved aside.
fn parse_aside(name: &str) -> Option<(&str, u32)> {
    let (orig, pid) = name.rsplit_once(".atlas-old-")?;
    Some((orig, pid.parse().ok()?))
}

/// Put back what an earlier run (killed mid-install) moved aside, and delete leftovers of earlier
/// successful runs. Windows cannot delete an image that is still running (the old hostd keeps
/// `atlas.exe` open until it exits), so an undeletable leftover simply waits for the next run.
fn recover(dir: &Path, own: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some((orig, pid)) = parse_aside(&name) else {
            continue;
        };
        if pid == own {
            continue;
        }
        let orig = dir.join(orig);
        if orig.exists() {
            let _ = remove(&e.path());
        } else {
            let _ = std::fs::rename(e.path(), orig);
        }
    }
}

fn remove(p: &Path) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// The install steps around the installer, with the installer (`run`: given the install folder)
/// and the registry read (`registered`) injected. On any failure the previous files and version
/// record are restored; on success the moved-aside files are deleted (or left for the next run
/// when still in use).
///
/// Every entry of the install folder is moved aside by renaming it in place, never the folder
/// itself: Windows refuses to move a folder while any file in it is open, and one always is
/// during `atlas update` (the bundled `atlas.exe` running the update, the terminal host
/// `atlas.exe hostd`), but it does let a running image be renamed.
///
/// The check: `target` is a file and, when the installer registered a version, it equals
/// `version`. An installer that registers nothing (a `--from` build) is judged by the file only.
fn install_with(
    target: &Path,
    version: Option<&str>,
    vf: &Path,
    run: impl FnOnce(&Path) -> Result<()>,
    registered: impl FnOnce() -> Option<String>,
) -> Result<()> {
    let dir = target.parent().context("bad install path")?;
    let pid = std::process::id();
    recover(dir, pid);
    let suffix = aside_suffix(pid);
    let mut aside: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if parse_aside(&name).is_some() {
                continue;
            }
            let from = e.path();
            let to = dir.join(format!("{name}{suffix}"));
            if let Err(err) = std::fs::rename(&from, &to) {
                for (f, t) in aside.iter().rev() {
                    let _ = std::fs::rename(t, f);
                }
                return Err(err)
                    .with_context(|| format!("cannot move {} aside to update it", from.display()));
            }
            aside.push((from, to));
        }
    }
    let had_old = !aside.is_empty();
    let old_version = std::fs::read(vf).ok();

    let result = (|| -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        run(dir)?;
        if !target.is_file() {
            bail!("the installer finished but {} is missing", target.display());
        }
        if let (Some(want), Some(got)) = (version, registered())
            && got != want
        {
            bail!("the installer left Atlas Desktop {got}, expected {want}");
        }
        record_version(vf, version)
    })();

    match result {
        Ok(()) => {
            for (_, t) in &aside {
                let _ = remove(t);
            }
            Ok(())
        }
        Err(e) => {
            // Drop what the installer wrote, then put every original back under its name.
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if parse_aside(&name).is_none() {
                        let _ = remove(&e.path());
                    }
                }
            }
            for (f, t) in &aside {
                let _ = std::fs::rename(t, f);
            }
            if !had_old {
                let _ = std::fs::remove_dir(dir);
            }
            match old_version {
                Some(v) => {
                    let _ = std::fs::write(vf, v);
                }
                None => {
                    let _ = std::fs::remove_file(vf);
                }
            }
            Err(e.context(if had_old {
                "Atlas Desktop update failed; the previous install was restored"
            } else {
                "Atlas Desktop install failed"
            }))
        }
    }
}

fn record_version(vf: &Path, version: Option<&str>) -> Result<()> {
    match version {
        Some(v) => {
            if let Some(d) = vf.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(vf, v)?;
        }
        None => {
            let _ = std::fs::remove_file(vf);
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
/// `args` are its argv (a path to open); a running instance receives them through the app's
/// single-instance forwarding.
pub(super) fn open(app: &Path, args: &[std::ffi::OsString]) -> Result<()> {
    let mut cmd = Command::new(app);
    cmd.args(args);
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

#[cfg(test)]
mod install_tests {
    use super::*;

    /// An installer stand-in that writes `body` as the app into the folder it is given.
    fn installs(body: &'static str) -> impl FnOnce(&Path) -> Result<()> {
        move |dir| {
            std::fs::write(dir.join("atlas-desktop.exe"), body)?;
            Ok(())
        }
    }

    /// A sandbox with Atlas `1.0.0` installed: (root, target, version file).
    fn installed() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Atlas");
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("atlas-desktop.exe");
        std::fs::write(&target, "old").unwrap();
        std::fs::write(dir.join("data.bin"), "keep").unwrap();
        let vf = tmp.path().join("app-version");
        std::fs::write(&vf, "1.0.0").unwrap();
        (tmp, target, vf)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_verified_install_replaces_the_app_records_the_version_and_drops_the_backup() {
        let (tmp, target, vf) = installed();
        install_with(&target, Some("1.1.0"), &vf, installs("new"), || {
            Some("1.1.0".into())
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&vf).unwrap(), "1.1.0");
        assert_eq!(names(tmp.path()), ["Atlas", "app-version"]);
    }

    #[test]
    fn a_failing_installer_restores_the_previous_install_and_version() {
        let (tmp, target, vf) = installed();
        let e = install_with(
            &target,
            Some("1.1.0"),
            &vf,
            |dir| {
                // Half-written, then the installer dies.
                std::fs::write(dir.join("atlas-desktop.exe"), "partial")?;
                bail!("the Atlas installer failed (exit code: 1)")
            },
            || None,
        )
        .unwrap_err();
        assert!(format!("{e:#}").contains("installer failed"), "{e:#}");
        assert!(format!("{e:#}").contains("previous install was restored"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(
            std::fs::read_to_string(target.with_file_name("data.bin")).unwrap(),
            "keep"
        );
        assert_eq!(std::fs::read_to_string(&vf).unwrap(), "1.0.0");
        assert_eq!(names(tmp.path()), ["Atlas", "app-version"]);
    }

    #[test]
    fn an_installer_that_leaves_another_version_is_rolled_back() {
        let (tmp, target, vf) = installed();
        let e = install_with(&target, Some("1.1.0"), &vf, installs("new"), || {
            Some("1.0.5".into())
        })
        .unwrap_err();
        assert!(format!("{e:#}").contains("left Atlas Desktop 1.0.5, expected 1.1.0"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(&vf).unwrap(), "1.0.0");
        assert_eq!(names(tmp.path()), ["Atlas", "app-version"]);
    }

    #[test]
    fn an_installer_that_finishes_without_the_app_is_rolled_back() {
        let (tmp, target, vf) = installed();
        let e = install_with(&target, Some("1.1.0"), &vf, |_| Ok(()), || None).unwrap_err();
        assert!(format!("{e:#}").contains("is missing"), "{e:#}");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(names(tmp.path()), ["Atlas", "app-version"]);
    }

    /// A first install has nothing to restore: a failure leaves no folder and no record.
    #[test]
    fn a_failed_first_install_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Atlas").join("atlas-desktop.exe");
        let vf = tmp.path().join("app-version");
        assert!(install_with(&target, Some("1.0.0"), &vf, |_| bail!("boom"), || None).is_err());
        assert!(names(tmp.path()).is_empty(), "{:?}", names(tmp.path()));
    }

    /// An installer that registers no version (a `--from` build) is judged by the file alone.
    #[test]
    fn an_unregistered_version_is_not_a_failure() {
        let (_tmp, target, vf) = installed();
        install_with(&target, None, &vf, installs("new"), || None).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(!vf.exists());
    }

    /// Killed after the files were moved aside, before the installer ran: the next run puts the
    /// old files back, then replaces them.
    #[test]
    fn an_install_killed_after_the_move_is_recovered_by_the_next_one() {
        let (tmp, target, vf) = installed();
        let dir = tmp.path().join("Atlas");
        for f in ["atlas-desktop.exe", "data.bin"] {
            std::fs::rename(dir.join(f), dir.join(format!("{f}{}", aside_suffix(1)))).unwrap();
        }

        let e = install_with(&target, Some("1.1.0"), &vf, |_| bail!("boom"), || None).unwrap_err();
        assert!(format!("{e:#}").contains("boom"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(names(&dir), ["atlas-desktop.exe", "data.bin"]);

        install_with(&target, Some("1.1.0"), &vf, installs("new"), || {
            Some("1.1.0".into())
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(names(tmp.path()), ["Atlas", "app-version"]);
    }

    /// The bug `atlas update` hit on Windows: a file in the install folder is open (the bundled
    /// `atlas.exe` running the update, `atlas.exe hostd`), so the folder could not be moved. The
    /// open file is now renamed in place and the update goes through; the leftover it leaves while
    /// still open is cleaned by the next run.
    #[cfg(windows)]
    #[test]
    fn an_open_file_in_the_install_folder_does_not_block_the_update() {
        use std::os::windows::fs::OpenOptionsExt;
        let (tmp, target, vf) = installed();
        let dir = tmp.path().join("Atlas");
        let held = dir.join("atlas.exe");
        std::fs::write(&held, "running").unwrap();
        // How Windows opens a running image: others may read and rename it (FILE_SHARE_DELETE), not write.
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x4)
            .open(&held)
            .unwrap();
        assert!(
            std::fs::rename(&dir, tmp.path().join("Atlas.moved")).is_err(),
            "the folder must be unmovable while a file in it is open, else this test proves nothing"
        );

        install_with(&target, Some("1.1.0"), &vf, installs("new"), || {
            Some("1.1.0".into())
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&vf).unwrap(), "1.1.0");

        drop(handle);
        install_with(&target, Some("1.1.0"), &vf, installs("new"), || {
            Some("1.1.0".into())
        })
        .unwrap();
        assert_eq!(names(&dir), ["atlas-desktop.exe"]);
    }
}
