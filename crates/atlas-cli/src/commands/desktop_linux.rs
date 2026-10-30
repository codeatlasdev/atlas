//! Linux side of `atlas desktop`: the app is one AppImage (`Atlas-<version>-<platform>.AppImage`),
//! installed as `~/.local/bin/atlas-desktop` with a menu entry and icon under `~/.local/share`.
//! A distro package (.deb, .rpm, Arch) installs `/usr/bin/atlas-desktop` instead; that one is
//! opened but never replaced, its package manager updates it.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Menu entry and icon id; the same reverse-DNS id as the macOS bundle (`tauri.conf.json`).
const APP_ID: &str = "br.com.codeatlas.atlas";
/// The id before 1.6; its menu entry and icon are removed when the new ones are written.
const LEGACY_APP_ID: &str = "dev.codeatlas.atlas";
const ICON: &[u8] = include_bytes!("../../../atlas-desktop/icons/128x128.png");

/// The AppImage carries no readable version, so the install records it next to atlas's other
/// desktop state.
fn version_file() -> PathBuf {
    atlas_core::paths::atlas_home()
        .join("desktop")
        .join("appimage-version")
}

/// Version recorded when `app` was installed; none for a link (`atlas link --desktop`) or a
/// `--from` install of a file whose version is unknown.
pub(super) fn app_version(app: &Path) -> Option<String> {
    if super::desktop::is_symlink(app) {
        return None;
    }
    let v = std::fs::read_to_string(version_file()).ok()?;
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Put the AppImage `source` at `target` and record `version`; then (re)write the menu entry.
pub(super) fn install(source: &Path, target: &Path, version: Option<&str>) -> Result<()> {
    install_at(source, target, version, &version_file())?;
    if let Some(home) = dirs::home_dir()
        && let Err(e) = write_menu_entry(&home, target)
    {
        eprintln!(
            "\x1b[33m◆\x1b[0m No menu entry ({e:#}); start it with: {}",
            target.display()
        );
    }
    Ok(())
}

/// The previous AppImage, kept beside `target` until the new one is in place and recorded.
fn prev_path(target: &Path) -> PathBuf {
    target.with_file_name(".atlas-desktop.prev")
}

/// [`install_file`] plus the version record at `vf`, as one step: the previous app is kept as
/// `.atlas-desktop.prev` until both are done and put back when either fails, so an update never
/// leaves a missing or unrecorded app. A kill in between is healed by the next install.
fn install_at(source: &Path, target: &Path, version: Option<&str>, vf: &Path) -> Result<()> {
    let prev = prev_path(target);
    // A previous run killed between its renames: `prev` is the app that was there.
    if target.symlink_metadata().is_err() && prev.symlink_metadata().is_ok() {
        let _ = std::fs::rename(&prev, target);
    }
    let had_old = target.symlink_metadata().is_ok();
    let old_version = std::fs::read(vf).ok();
    install_file(source, target, &prev)?;
    if let Err(e) = record_version(vf, version) {
        restore(target, &prev, had_old, vf, old_version);
        return Err(e);
    }
    let _ = std::fs::remove_file(&prev);
    Ok(())
}

/// Undo a failed install: the old app (when there was one) and its version record come back.
fn restore(target: &Path, prev: &Path, had_old: bool, vf: &Path, old_version: Option<Vec<u8>>) {
    if had_old {
        let _ = std::fs::rename(prev, target);
    } else {
        let _ = std::fs::remove_file(target);
    }
    match old_version {
        Some(v) => {
            let _ = std::fs::write(vf, v);
        }
        None => {
            let _ = std::fs::remove_file(vf);
        }
    }
}

fn record_version(vf: &Path, version: Option<&str>) -> Result<()> {
    match version {
        Some(v) => {
            if let Some(dir) = vf.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(vf, v)
                .with_context(|| format!("cannot record version in {}", vf.display()))
        }
        None => {
            let _ = std::fs::remove_file(vf);
            Ok(())
        }
    }
}

/// Stage `source` next to `target`, move the existing app to `prev`, and rename the staged file
/// into place. The staging file is removed on every path; when the final rename fails `prev`
/// goes back.
fn install_file(source: &Path, target: &Path, prev: &Path) -> Result<()> {
    if !is_elf(source) {
        bail!(
            "{} is not an AppImage (expected an executable ELF file)",
            source.display()
        );
    }
    let parent = target.parent().context("bad install path")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    let staging = Staging(parent.join(".atlas-desktop.new"));
    std::fs::copy(source, &staging.0)
        .with_context(|| format!("cannot copy {}", source.display()))?;
    std::fs::set_permissions(&staging.0, std::fs::Permissions::from_mode(0o755))?;
    let had_old = target.symlink_metadata().is_ok();
    if had_old {
        std::fs::rename(target, prev)
            .with_context(|| format!("cannot move {}", target.display()))?;
    }
    // rename(2) replaces atomically; a running instance keeps its (unlinked) inode.
    if let Err(e) = std::fs::rename(&staging.0, target) {
        if had_old {
            let _ = std::fs::rename(prev, target);
        }
        return Err(e).with_context(|| format!("cannot install {}", target.display()));
    }
    Ok(())
}

/// Removes the staged file when dropped (also after the successful rename, where it is gone).
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn is_elf(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok_and(|()| magic == *b"\x7fELF")
}

/// `~/.local/share/applications/<id>.desktop` and its icon, so the app shows up in the launcher.
fn write_menu_entry(home: &Path, exec: &Path) -> Result<()> {
    let share = home.join(".local/share");
    let icon = share.join(format!("icons/hicolor/128x128/apps/{APP_ID}.png"));
    let entry = share.join(format!("applications/{APP_ID}.desktop"));
    for f in [&icon, &entry] {
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::write(&icon, ICON)?;
    std::fs::write(&entry, desktop_entry(exec))?;
    let _ =
        std::fs::remove_file(share.join(format!("icons/hicolor/128x128/apps/{LEGACY_APP_ID}.png")));
    let _ = std::fs::remove_file(share.join(format!("applications/{LEGACY_APP_ID}.desktop")));
    Ok(())
}

/// A freedesktop.org desktop entry starting `exec`.
fn desktop_entry(exec: &Path) -> String {
    // Exec quoting (Desktop Entry spec): the path in double quotes, with `"`, `` ` ``, `$` and `\` escaped.
    let quoted: String = exec
        .to_string_lossy()
        .chars()
        .flat_map(|c| {
            let esc = matches!(c, '"' | '`' | '$' | '\\').then_some('\\');
            esc.into_iter().chain(std::iter::once(c))
        })
        .collect();
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Atlas\n\
         Comment=The developer platform for everyone\n\
         Exec=\"{quoted}\"\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Development;IDE;\n\
         StartupWMClass=atlas-desktop\n"
    )
}

/// Start the app detached from this terminal (own session, no stdio), so closing the shell or
/// hitting Ctrl-C does not take the app down. `args` are its argv (a path to open).
pub(super) fn open(app: &Path, args: &[std::ffi::OsString]) -> Result<()> {
    let mut cmd = Command::new(app);
    cmd.args(args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no memory of the parent.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
        .with_context(|| format!("cannot start {}", app.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_appimage(path: &Path, body: &[u8]) {
        std::fs::write(path, [b"\x7fELF".as_slice(), body].concat()).unwrap();
    }

    /// Installs through the real path with a version file inside the sandbox.
    fn put(src: &Path, target: &Path, version: Option<&str>) -> Result<()> {
        install_at(src, target, version, &target.with_file_name("version"))
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn install_replaces_the_appimage_in_place_executable_and_leaves_no_staging_or_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fake_appimage(&a, b"one");
        fake_appimage(&b, b"two");
        let target = tmp.path().join("bin").join("atlas-desktop");

        put(&a, &target, Some("1.0.0")).unwrap();
        put(&b, &target, Some("1.1.0")).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFtwo");
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(
            std::fs::read_to_string(target.with_file_name("version")).unwrap(),
            "1.1.0"
        );
        assert_eq!(
            names(target.parent().unwrap()),
            ["atlas-desktop", "version"]
        );
    }

    /// A truncated download or an HTML error page must never replace a working app.
    #[test]
    fn install_refuses_a_file_that_is_not_an_executable_and_keeps_the_old_app() {
        let tmp = tempfile::tempdir().unwrap();
        let good = tmp.path().join("good");
        fake_appimage(&good, b"ok");
        let target = tmp.path().join("atlas-desktop");
        put(&good, &target, Some("1.0.0")).unwrap();

        for bad in [b"<html>404</html>".as_slice(), b"\x7fEL", b""] {
            let p = tmp.path().join("bad");
            std::fs::write(&p, bad).unwrap();
            assert!(put(&p, &target, Some("2.0.0")).is_err(), "{bad:?}");
            assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFok");
        }
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("version")).unwrap(),
            "1.0.0"
        );
    }

    /// The new file is in place but its version cannot be recorded: the old app and the old
    /// record come back instead of an app whose version disagrees with the CLI's.
    #[test]
    fn a_failure_after_the_swap_restores_the_previous_app_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fake_appimage(&a, b"one");
        fake_appimage(&b, b"two");
        let bin = tmp.path().join("bin");
        let target = bin.join("atlas-desktop");
        let vf = bin.join("version");
        install_at(&a, &target, Some("1.0.0"), &vf).unwrap();

        // A directory where the version file goes: the write fails after the swap.
        std::fs::remove_file(&vf).unwrap();
        std::fs::create_dir(&vf).unwrap();
        assert!(install_at(&b, &target, Some("2.0.0"), &vf).is_err());

        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFone");
        assert!(vf.is_dir());
        assert_eq!(names(&bin), ["atlas-desktop", "version"]);
    }

    /// A kill between "old → .prev" and "new → target" leaves no app. The next install puts the
    /// old one back first, and ends on the new one with nothing else around.
    #[test]
    fn an_install_killed_between_its_renames_is_recovered_by_the_next_one() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fake_appimage(&a, b"one");
        fake_appimage(&b, b"two");
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let target = bin.join("atlas-desktop");
        std::fs::copy(&a, prev_path(&target)).unwrap();
        assert!(target.symlink_metadata().is_err());

        // The recovery runs even when the follow-up install is refused.
        let junk = tmp.path().join("junk");
        std::fs::write(&junk, "<html>").unwrap();
        assert!(put(&junk, &target, Some("2.0.0")).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFone");

        put(&b, &target, Some("2.0.0")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFtwo");
        assert_eq!(names(&bin), ["atlas-desktop", "version"]);
    }

    /// The install fails before the swap (the folder is read-only, so nothing can be staged):
    /// the old app and its record stay, and no staging or backup file appears.
    #[test]
    fn a_failure_before_the_swap_leaves_no_staging_file_and_keeps_the_old_app() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let target = bin.join("atlas-desktop");
        let src = tmp.path().join("src");
        fake_appimage(&src, b"x");
        put(&src, &target, Some("1.0.0")).unwrap();
        // A target whose folder is read-only: staging cannot even be created.
        let ro = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions(&bin, ro).unwrap();
        let r = put(&src, &target, Some("2.0.0"));
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        if r.is_ok() {
            return; // root ignores the mode; nothing to assert
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELFx");
        assert_eq!(names(&bin), ["atlas-desktop", "version"]);
    }

    #[test]
    fn menu_entry_quotes_the_exec_path_and_names_the_icon() {
        let e = desktop_entry(Path::new("/home/a b/.local/bin/atlas-desktop"));
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert!(e.contains("\nExec=\"/home/a b/.local/bin/atlas-desktop\"\n"));
        assert!(e.contains("\nIcon=br.com.codeatlas.atlas\n"));
        let odd = desktop_entry(Path::new("/h/$x\"`y\\z"));
        assert!(odd.contains(r#"Exec="/h/\$x\"\`y\\z""#), "{odd}");
    }

    /// The app id moved from dev.codeatlas.atlas (≤ 1.5): an update must not leave two "Atlas"
    /// entries in the launcher, the old one pointing at a stale icon.
    #[test]
    fn menu_entry_replaces_the_pre_rename_entry_and_icon() {
        let tmp = tempfile::tempdir().unwrap();
        let share = tmp.path().join(".local/share");
        let old_entry = share.join("applications/dev.codeatlas.atlas.desktop");
        let old_icon = share.join("icons/hicolor/128x128/apps/dev.codeatlas.atlas.png");
        for f in [&old_entry, &old_icon] {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "old").unwrap();
        }
        write_menu_entry(tmp.path(), Path::new("/bin/atlas-desktop")).unwrap();
        assert!(!old_entry.exists() && !old_icon.exists());
        assert!(
            share
                .join("applications/br.com.codeatlas.atlas.desktop")
                .is_file()
        );
        assert!(
            share
                .join("icons/hicolor/128x128/apps/br.com.codeatlas.atlas.png")
                .is_file()
        );
    }
}
