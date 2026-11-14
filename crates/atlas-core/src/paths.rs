//! Well-known locations and executable lookup shared by the CLI, TUI and desktop app.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The user's home: `$HOME` when set to an absolute path (tests and sandboxes point it elsewhere),
/// else the OS's own (`%USERPROFILE%` on Windows, where cmd and PowerShell set no `HOME`).
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|h| h.is_absolute())
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `~/.atlas` (falls back to `/.atlas` when there is no home directory).
pub fn atlas_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".atlas")
}

/// Control socket of a running Atlas Desktop (`atlas wt|term|browser|file` talk to it).
pub fn desktop_socket() -> PathBuf {
    atlas_home().join("desktop.sock")
}

/// Per-run auth token for [`desktop_socket`].
pub fn desktop_token() -> PathBuf {
    atlas_home().join("desktop.token")
}

/// Debug-build bridge socket (`atlas desktop debug …`). `ATLAS_DEBUG_SOCK` (an absolute path) picks another
/// instance's bridge: agents run isolated instances and address them without changing `$HOME`.
pub fn desktop_debug_socket() -> PathBuf {
    debug_socket_in(
        std::env::var_os("ATLAS_DEBUG_SOCK").as_deref(),
        &atlas_home(),
    )
}

/// [`desktop_debug_socket`] for an explicit override and home (pure, for tests).
pub fn debug_socket_in(over: Option<&OsStr>, home: &Path) -> PathBuf {
    match over.map(Path::new) {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        _ => home.join("desktop-debug.sock"),
    }
}

/// A path in the form Atlas stores and compares: unchanged on Unix. On Windows forward slashes
/// (`C:/Users/me/repo`, the form git prints and every Windows API accepts) and no `\\?\` verbatim
/// prefix, so the app's `root + "/" + rel` joins and prefix checks hold on every OS.
pub fn normalize(p: &Path) -> PathBuf {
    if !cfg!(windows) {
        return p.to_path_buf();
    }
    let s = p.to_string_lossy();
    let s = match s.strip_prefix(r"\\?\") {
        Some(rest) => match rest.strip_prefix(r"UNC\") {
            Some(unc) => format!(r"\\{unc}"),
            None => rest.to_string(),
        },
        None => s.into_owned(),
    };
    PathBuf::from(s.replace('\\', "/"))
}

/// `std::fs::canonicalize`, [`normalize`]d (Windows' canonical form is a `\\?\` verbatim path).
pub fn canonical(p: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(p).map(|c| normalize(&c))
}

/// First executable named `name` in the `PATH`-style list `path`. On Windows `name` without an
/// extension also matches `name` plus each `PATHEXT` extension (`git` finds `git.exe`).
pub fn which_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    let names = exe_names(name);
    std::env::split_paths(path)
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| is_executable(p))
}

/// Windows `PATHEXT`, lowercase (`.com;.exe;.bat;.cmd` when unset).
#[cfg(windows)]
fn pathext() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The file names `name` may have on disk: itself, and on Windows (when it has no extension) with
/// each `PATHEXT` extension appended.
pub fn exe_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    if Path::new(name).extension().is_none() {
        return pathext()
            .into_iter()
            .map(|e| format!("{name}{e}"))
            .collect();
    }
    vec![name.to_string()]
}

/// First executable named `name` on `$PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &std::env::var_os("PATH")?)
}

/// A file the OS would run: an execute bit on Unix, an executable extension (`PATHEXT`) on Windows.
pub fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        let ext = p
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_ascii_lowercase()));
        p.metadata().is_ok_and(|m| m.is_file()) && ext.is_some_and(|e| pathext().contains(&e))
    }
}

/// Windows: the `atlas.exe` the desktop installer puts next to `atlas-desktop.exe`, so the app
/// finds a CLI (to start hostd) before `install.ps1` ever ran. `None` elsewhere, or when absent.
pub fn bundled_cli() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let cli = exe.parent()?.join(self::exe("atlas"));
    (cli != exe && cli.is_file()).then_some(cli)
}

/// The `atlas` CLI the desktop app runs (stacks, hostd, updates), as the first runnable file among, in order:
/// `$ATLAS_BIN`, `~/.atlas/bin/atlas`, the installer-bundled one (Windows), then `atlas` on the login shell's `PATH`
/// and on the process `PATH`. A candidate that is not an executable file (an empty override, a directory, a
/// half-written install) is skipped, never returned. The login-shell `PATH` comes before the process one: an app
/// started from Finder or the Dock has a minimal `PATH` that misses `/opt/homebrew/bin` and friends. Pure: every
/// input is an argument.
pub fn resolve_atlas_cli(
    env_override: Option<&OsStr>,
    home: Option<&Path>,
    bundled: Option<PathBuf>,
    login_path: Option<&OsStr>,
    process_path: Option<&OsStr>,
) -> Option<PathBuf> {
    env_override
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain(home.map(|h| h.join(".atlas/bin").join(exe("atlas"))))
        .chain(bundled)
        .find(|p| is_executable(p))
        .or_else(|| {
            login_path
                .filter(|p| !p.is_empty())
                .and_then(|p| which_in("atlas", p))
        })
        .or_else(|| {
            process_path
                .filter(|p| !p.is_empty())
                .and_then(|p| which_in("atlas", p))
        })
}

/// `name` as an executable file name on this OS: `atlas` on Unix, `atlas.exe` on Windows.
pub fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The Atlas Desktop that `atlas desktop`/`atlas update` manage, when installed (symlinks count).
/// macOS: `/Applications/Atlas.app`, then `~/Applications/Atlas.app`. Linux: the AppImage at
/// [`linux_app`]. Windows: the per-user install at [`windows_app`]. A distro package's app is not
/// one of these: see [`packaged_app`].
pub fn installed_app() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    app_candidates(&home)
        .into_iter()
        .find(|p| p.symlink_metadata().is_ok())
}

fn app_candidates(home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        [PathBuf::from("/Applications"), home.join("Applications")]
            .into_iter()
            .map(|d| d.join("Atlas.app"))
            .collect()
    } else if cfg!(windows) {
        vec![windows_app(&local_app_data(home))]
    } else {
        vec![linux_app(home)]
    }
}

/// Windows: `%LOCALAPPDATA%` (`<home>/AppData/Local` when unset).
pub fn local_app_data(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"))
}

/// Windows: Atlas Desktop as its per-user installer lays it out,
/// `%LOCALAPPDATA%/Atlas/atlas-desktop.exe` (no admin rights, like `~/.atlas/bin` for the CLI).
pub fn windows_app(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Atlas")
        .join(format!("{LINUX_APP_BIN}.exe"))
}

/// Linux: where `atlas desktop` puts the AppImage, `~/.local/bin/atlas-desktop` (on `PATH` for most
/// desktops, and the same command name the .deb/.rpm/Arch packages install in `/usr/bin`).
pub fn linux_app(home: &Path) -> PathBuf {
    home.join(".local/bin").join(LINUX_APP_BIN)
}

/// Binary name of Atlas Desktop on Linux (AppImage and distro packages alike) and, with `.exe`, on
/// Windows.
pub const LINUX_APP_BIN: &str = "atlas-desktop";

/// Linux: Atlas Desktop installed by a distro package (.deb, .rpm, Arch). Its package manager
/// owns it: atlas opens it but never replaces it.
pub fn packaged_app() -> Option<PathBuf> {
    if cfg!(any(target_os = "macos", windows)) {
        return None;
    }
    Some(Path::new("/usr/bin").join(LINUX_APP_BIN)).filter(|p| is_executable(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_is_looked_for_where_atlas_desktop_installs_it() {
        let home = Path::new("/home/u");
        let found = app_candidates(home);
        if cfg!(target_os = "macos") {
            assert_eq!(
                found,
                [
                    PathBuf::from("/Applications/Atlas.app"),
                    home.join("Applications/Atlas.app")
                ]
            );
        } else if cfg!(windows) {
            assert_eq!(found.len(), 1);
            assert!(
                found[0].ends_with("Atlas/atlas-desktop.exe"),
                "{}",
                found[0].display()
            );
        } else {
            // Never `/usr/bin/atlas-desktop`: `atlas update` must not overwrite a distro package's file.
            assert_eq!(found, [PathBuf::from("/home/u/.local/bin/atlas-desktop")]);
        }
    }

    #[test]
    fn normalized_paths_use_forward_slashes_and_drop_the_verbatim_prefix_on_windows() {
        let cases = [
            (r"\\?\C:\Users\me\repo", "C:/Users/me/repo"),
            (r"C:\Users\me\repo", "C:/Users/me/repo"),
            ("C:/Users/me/repo", "C:/Users/me/repo"),
            (r"\\?\UNC\server\share\x", "//server/share/x"),
        ];
        for (raw, win) in cases {
            let want = if cfg!(windows) { win } else { raw };
            assert_eq!(normalize(Path::new(raw)), PathBuf::from(want), "{raw}");
        }
        let d = tempfile::tempdir().unwrap();
        let c = canonical(d.path()).unwrap();
        assert!(c.is_dir());
        if cfg!(windows) {
            let s = c.to_string_lossy();
            assert!(!s.contains('\\') && !s.starts_with("//?/"), "{s}");
        }
    }

    #[test]
    fn the_windows_app_lives_in_the_per_user_install_dir() {
        let lad = Path::new("C:/Users/u/AppData/Local");
        assert_eq!(
            windows_app(lad),
            lad.join("Atlas").join("atlas-desktop.exe")
        );
    }

    #[test]
    fn the_debug_socket_defaults_to_the_homes_and_an_absolute_override_wins() {
        let home = Path::new("/h/.atlas");
        assert_eq!(debug_socket_in(None, home), home.join("desktop-debug.sock"));
        let abs = std::env::temp_dir().join("agent-1").join("debug.sock");
        assert_eq!(debug_socket_in(Some(abs.as_os_str()), home), abs);
    }

    #[test]
    fn a_relative_or_empty_override_is_ignored_not_resolved_against_the_cwd() {
        let home = Path::new("/h/.atlas");
        for bad in ["", "debug.sock", "./x.sock", "../x.sock"] {
            assert_eq!(
                debug_socket_in(Some(OsStr::new(bad)), home),
                home.join("desktop-debug.sock"),
                "{bad:?}"
            );
        }
    }

    #[cfg(unix)]
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

    #[cfg(windows)]
    #[test]
    fn which_in_finds_a_bare_name_through_its_executable_extension_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool"), "").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        // An extensionless file is not runnable on Windows.
        assert_eq!(which_in("tool", &path), None);
        let exe = dir.path().join("tool.exe");
        std::fs::write(&exe, "").unwrap();
        assert_eq!(which_in("tool", &path), Some(exe.clone()));
        assert_eq!(which_in("tool.exe", &path), Some(exe));
        assert_eq!(which_in("notes.txt", &path), None);
    }
    /// A runnable fake `atlas` in `dir` (execute bit on Unix, `.exe` on Windows).
    fn fake_cli(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(exe("atlas"));
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    fn cli_in(env: Option<&str>, home: &Path, login: Option<&Path>) -> Option<PathBuf> {
        let login = login.map(|p| std::env::join_paths([p]).unwrap());
        resolve_atlas_cli(
            env.map(OsStr::new),
            Some(home),
            None,
            login.as_deref(),
            None,
        )
    }

    // Incident class: hostd's lookup returned `$ATLAS_BIN` without looking at it and accepted any existing
    // `~/.atlas/bin/atlas` (a directory, a half-written or non-executable file), so a stray variable or an
    // interrupted install made hostd (the terminals) die in a retry loop while a good CLI sat on the PATH.
    #[test]
    fn an_unusable_atlas_bin_override_falls_through_to_the_installed_cli() {
        let t = tempfile::tempdir().unwrap();
        let good = fake_cli(&t.path().join(".atlas/bin"));
        for bad in ["", "/no/such/atlas", t.path().to_str().unwrap()] {
            assert_eq!(
                cli_in(Some(bad), t.path(), None),
                Some(good.clone()),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_usable_override_wins_over_everything() {
        let t = tempfile::tempdir().unwrap();
        fake_cli(&t.path().join(".atlas/bin"));
        let mine = fake_cli(&t.path().join("mine"));
        assert_eq!(
            cli_in(Some(mine.to_str().unwrap()), t.path(), None),
            Some(mine)
        );
    }

    #[test]
    fn a_directory_or_empty_stub_at_the_install_path_does_not_hide_a_cli_on_the_login_path() {
        let t = tempfile::tempdir().unwrap();
        // `~/.atlas/bin/atlas` is a directory (a botched install)
        std::fs::create_dir_all(t.path().join(".atlas/bin").join(exe("atlas"))).unwrap();
        let on_path = fake_cli(&t.path().join("brew/bin"));
        assert_eq!(
            cli_in(None, t.path(), Some(&t.path().join("brew/bin"))),
            Some(on_path)
        );
        // nothing usable anywhere
        assert_eq!(cli_in(None, t.path(), None), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_non_executable_file_is_not_a_cli() {
        let t = tempfile::tempdir().unwrap();
        let p = fake_cli(&t.path().join(".atlas/bin"));
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(cli_in(None, t.path(), None), None);
    }

    #[test]
    fn the_login_shell_path_is_tried_before_the_apps_own_minimal_path() {
        let t = tempfile::tempdir().unwrap();
        let login = fake_cli(&t.path().join("login"));
        let proc_ = fake_cli(&t.path().join("proc"));
        let lp = std::env::join_paths([t.path().join("login")]).unwrap();
        let pp = std::env::join_paths([t.path().join("proc")]).unwrap();
        let r = resolve_atlas_cli(None, Some(t.path()), None, Some(&lp), Some(&pp));
        assert_eq!(r, Some(login));
        let r = resolve_atlas_cli(None, Some(t.path()), None, None, Some(&pp));
        assert_eq!(r, Some(proc_));
    }

    // `PATH=""` splits into one empty entry, which joined to `atlas` is the current directory: a stray `./atlas`
    // must never be run because the variable was empty.
    #[test]
    fn an_empty_path_variable_is_not_the_current_directory() {
        let t = tempfile::tempdir().unwrap();
        fake_cli(t.path());
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(t.path()).unwrap();
        let empty = OsStr::new("");
        let r = resolve_atlas_cli(None, None, None, Some(empty), Some(empty));
        std::env::set_current_dir(cwd).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn the_bundled_cli_comes_after_the_installed_one() {
        let t = tempfile::tempdir().unwrap();
        let bundled = fake_cli(&t.path().join("app"));
        let r = resolve_atlas_cli(None, Some(t.path()), Some(bundled.clone()), None, None);
        assert_eq!(r, Some(bundled));
        let installed = fake_cli(&t.path().join(".atlas/bin"));
        let r = resolve_atlas_cli(
            None,
            Some(t.path()),
            Some(t.path().join("app").join(exe("atlas"))),
            None,
            None,
        );
        assert_eq!(r, Some(installed));
    }
}
