//! The real machine: unpack the embedded installer, close Atlas, run NSIS, open Atlas. Every child
//! is started with `no_window()` (a console flash would spoil the one-click window).

use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use atlas_core::sys::CommandExt as _;
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};

use crate::install::{self, Running};
use crate::paths::{self, APP_EXE};
use crate::uninstall;

/// The per-user uninstall entry the NSIS installer registers (Settings > Apps).
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Atlas";

pub struct Real;

struct Nsis(Child);

impl Running for Nsis {
    fn poll(&mut self) -> Result<Option<i32>, String> {
        self.0
            .try_wait()
            .map(|s| s.map(|s| s.code().unwrap_or(-1)))
            .map_err(|e| format!("lost track of the installer: {e}"))
    }
}

impl install::Host for Real {
    fn unpack(&self, dir: &Path) -> Result<PathBuf, String> {
        let bytes = crate::payload::bytes()?;
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let p = dir.join(paths::payload_name());
        std::fs::write(&p, bytes).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
        Ok(p)
    }

    fn close_app(&self, install_dir: &Path, grace: Duration) {
        close_running_app(install_dir, grace);
    }

    fn start_installer(&self, exe: &Path, install_dir: &Path) -> Result<Box<dyn Running>, String> {
        nsis(exe, install_dir, false).map(|c| Box::new(Nsis(c)) as Box<dyn Running>)
    }

    fn register_uninstaller(&self, install_dir: &Path) -> Result<(), String> {
        let me = std::env::current_exe().map_err(|e| format!("cannot find myself: {e}"))?;
        let copy = install_dir.join(paths::OWN_COPY);
        std::fs::copy(&me, &copy).map_err(|e| format!("cannot write {}: {e}", copy.display()))?;
        let quoted = format!("\"{}\"", copy.display());
        for (name, value) in [
            ("UninstallString", format!("{quoted} --uninstall")),
            ("DisplayIcon", quoted.clone()),
        ] {
            let st = Command::new("reg")
                .args(["add", UNINSTALL_KEY, "/v", name, "/t", "REG_SZ", "/d"])
                .arg(&value)
                .arg("/f")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .no_window()
                .status()
                .map_err(|e| format!("cannot run reg.exe: {e}"))?;
            if !st.success() {
                return Err(format!("reg add {name} failed ({st})"));
            }
        }
        Ok(())
    }

    fn launch(&self, app: &Path) -> Result<(), String> {
        // Detached from this process: closing the setup window must not take Atlas with it.
        atlas_core::sys::spawn_detached(
            Command::new(app).current_dir(app.parent().unwrap_or(Path::new("."))),
        )
        .map(|_| ())
        .map_err(|e| format!("cannot start {}: {e}", app.display()))
    }
}

impl uninstall::Host for Real {
    fn close_app(&self, install_dir: &Path, grace: Duration) {
        close_running_app(install_dir, grace);
    }

    fn remove_cli_and_data(&self, cli: &Path) -> Result<(), String> {
        let out = Command::new(cli)
            .args(["uninstall", "--yes"])
            .stdin(Stdio::null())
            .no_window()
            .output()
            .map_err(|e| format!("cannot run {}: {e}", cli.display()))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "`atlas uninstall` failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    fn delete_after_exit(&self, path: &Path) -> Result<(), String> {
        // A running program cannot delete itself, but a detached `cmd` can once it has exited.
        let line = if path.is_dir() {
            cleanup_command(&[], &[path])
        } else {
            cleanup_command(&[path], &[])
        };
        let mut cmd = Command::new("cmd");
        cmd.args(["/d", "/c"])
            .raw_arg(&line)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .no_window();
        atlas_core::sys::spawn_detached(&mut cmd)
            .map(|_| ())
            .map_err(|e| format!("cannot start the cleanup: {e}"))
    }

    fn run_uninstaller(&self, uninstaller: &Path, install_dir: &Path) -> Result<i32, String> {
        // `_?=<dir>` makes NSIS run in place and wait, instead of copying itself to %TEMP% and
        // returning at once (which would leave us nothing to wait for).
        let mut child = nsis(uninstaller, install_dir, true)?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(s)) => return Ok(s.code().unwrap_or(-1)),
                Ok(None) if started.elapsed() > Duration::from_secs(300) => {
                    let _ = child.kill();
                    return Err("The Atlas uninstaller did not finish in 5 minutes.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(format!("lost track of the uninstaller: {e}")),
            }
        }
    }
}

/// The command line of the detached cleanup: wait until this process is gone, then delete `files`
/// and, last, the folders (`rmdir` refuses a folder that still holds anything, so it never takes
/// a user's files). Pure, so the quoting is tested.
pub fn cleanup_command(files: &[&Path], dirs: &[&Path]) -> String {
    // `ping -n 4` is the usual no-dependency sleep (about 3 s): enough for this process to exit.
    let mut c = String::from("ping -n 4 127.0.0.1 >nul");
    for f in files {
        c.push_str(&format!(" & del /f /q \"{}\"", f.display()));
    }
    for d in dirs {
        c.push_str(&format!(" & rmdir \"{}\"", d.display()));
    }
    c
}

/// An NSIS program, silent. `/D=` (install) or `_?=` (uninstall) must be last and unquoted.
fn nsis(exe: &Path, dir: &Path, uninstall: bool) -> Result<Child, String> {
    let mut c = Command::new(exe);
    c.arg("/S")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_window();
    c.raw_arg(if uninstall {
        format!("_?={}", dir.display())
    } else {
        paths::nsis_dir_arg(dir)
    });
    c.spawn()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))
}

/// Pids of Atlas Desktop processes that run from `install_dir` (a dev build elsewhere with the
/// same image name is not ours to close).
fn app_pids(install_dir: &Path) -> Vec<u32> {
    atlas_core::sys::process_rows()
        .into_iter()
        .filter(|r| r.exe.eq_ignore_ascii_case(APP_EXE))
        .filter(|r| {
            atlas_core::sys::exe_path(r.pid).is_some_and(|p| paths::is_inside(&p, install_dir))
        })
        .map(|r| r.pid)
        .collect()
}

/// Ask every window of those processes to close (`WM_CLOSE`: the app saves its state), wait up to
/// `grace`, and end what is left. The terminal host and the stacks run as their own processes and
/// keep going.
fn close_running_app(install_dir: &Path, grace: Duration) {
    let pids = app_pids(install_dir);
    if pids.is_empty() {
        return;
    }
    struct Ctx<'a>(&'a [u32]);
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` is the `&Ctx` passed below, alive for the whole EnumWindows call.
        let ctx = unsafe { &*(lparam as *const Ctx) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` comes from EnumWindows.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: as above.
        if ctx.0.contains(&pid) && unsafe { IsWindowVisible(hwnd) } != 0 {
            // SAFETY: posting a message to a foreign window is always allowed to fail.
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
        }
        1
    }
    let ctx = Ctx(&pids);
    // SAFETY: `each` matches WNDENUMPROC and `ctx` outlives the call.
    unsafe { EnumWindows(Some(each), &ctx as *const Ctx as LPARAM) };
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline && pids.iter().any(|p| atlas_core::sys::pid_alive(*p)) {
        std::thread::sleep(Duration::from_millis(100));
    }
    for p in pids {
        if atlas_core::sys::pid_alive(p) {
            atlas_core::sys::force_kill(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cleanup_waits_then_deletes_files_before_folders_with_quoted_paths() {
        let line = cleanup_command(
            &[Path::new(
                r"C:\Users\Ana Maria\AppData\Local\Atlas\Atlas Uninstall.exe",
            )],
            &[Path::new(r"C:\Users\Ana Maria\AppData\Local\Atlas")],
        );
        assert_eq!(
            line,
            "ping -n 4 127.0.0.1 >nul & del /f /q \"C:\\Users\\Ana Maria\\AppData\\Local\\Atlas\\Atlas Uninstall.exe\" & rmdir \"C:\\Users\\Ana Maria\\AppData\\Local\\Atlas\""
        );
        assert!(
            !line.contains("rmdir /s"),
            "the folder goes only when empty: never a user's files"
        );
    }
}
