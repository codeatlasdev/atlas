//! The real machine for [`crate::repair`]: ask the CLIs what they are, list the monitors, run the
//! two process actions. Every child has `no_window()`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use atlas_core::sys::CommandExt as _;
use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

use crate::fsutil::Log;
use crate::paths::Env;
use crate::repair::{self, Cli, Exec, Hostd, Rect, State};

/// A wedged binary must not hang the installer.
const ASK: Duration = Duration::from_secs(5);
/// `atlas hostd --reconcile` may restart the host (and wait for it to come back).
const RECONCILE: Duration = Duration::from_secs(60);

/// Run `cmd` to completion within `limit`: its stdout, or an error (spawn, non-zero exit, timeout).
fn run(cmd: &mut Command, limit: Duration) -> Result<String, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .no_window()
        .spawn()
        .map_err(|e| e.to_string())?;
    // stdout is read on a thread: a child that fills the pipe must not stall the poll below.
    let mut pipe = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut pipe, &mut s);
        s
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                let out = reader.join().unwrap_or_default();
                return if st.success() {
                    Ok(out)
                } else {
                    Err(format!("exited with {st}"))
                };
            }
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                return Err(format!("no answer in {}s", limit.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn cli_state(cli: &Path) -> Cli {
    match std::fs::symlink_metadata(cli) {
        Err(_) => Cli::Missing,
        Ok(m) if m.file_type().is_symlink() => Cli::Linked,
        Ok(_) => run(Command::new(cli).arg("version"), ASK)
            .ok()
            .and_then(|o| repair::parse_cli_version(&o))
            .map_or(Cli::Unreadable, Cli::Version),
    }
}

/// The running terminal host, asked through the bundled CLI (`hostd --status`: the protocol, the
/// token and the exact-pid rule are the CLI's, not ours). `None` when none answers: a host that
/// is not running needs nothing, and one that runs but cannot be asked is left to the app, which
/// retires an unusable host by its exact pid on connect.
fn hostd_state(bundled_cli: &Path) -> Option<Hostd> {
    let out = run(Command::new(bundled_cli).args(["hostd", "--status"]), ASK).ok()?;
    Some(Hostd {
        version: repair::parse_hostd_version(&out),
    })
}

fn monitors() -> Vec<Rect> {
    unsafe extern "system" fn each(_: HMONITOR, _: HDC, r: *mut RECT, out: LPARAM) -> i32 {
        // SAFETY: `out` is the `&mut Vec<Rect>` passed below, alive for the whole call; `r` is the
        // monitor rectangle EnumDisplayMonitors hands out.
        let (v, r) = unsafe { (&mut *(out as *mut Vec<Rect>), &*r) };
        v.push(Rect {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        });
        1
    }
    let mut v: Vec<Rect> = Vec::new();
    // SAFETY: a null DC and clip enumerate every monitor; `each` matches MONITORENUMPROC and `v`
    // outlives the call.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(each),
            &mut v as *mut Vec<Rect> as LPARAM,
        )
    };
    v
}

/// `~/.atlas/.update.lock` whose holder pid is dead (the file holds the pid `atlas update` wrote).
/// The lock itself is an OS lock the kernel drops with the process, so this is only the leftover
/// file; a live holder, or a file that holds no pid, is left alone.
fn stale_lock(data_dir: &Path) -> Option<PathBuf> {
    let p = data_dir.join(".update.lock");
    let pid: u32 = std::fs::read_to_string(&p).ok()?.trim().parse().ok()?;
    (!atlas_core::sys::pid_alive(pid)).then_some(p)
}

/// Read the machine and plan the repairs; the caller announces them and runs [`run_repair`].
/// `own_payload` (this run's unpacked installer) is never listed as a leftover.
pub fn plan_only(env: &Env, bundled_cli: &Path, own_payload: &Path) -> Vec<repair::Action> {
    let bundled = match bundled_version(bundled_cli) {
        Some(v) => v,
        // Without the bundled version nothing can be judged: do nothing rather than guess.
        None => return Vec::new(),
    };
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home.join("AppData").join("Roaming"));
    let f = repair::collect_files(env, &app_data, own_payload);
    repair::plan(&State {
        bundled,
        cli: cli_state(&env.cli_exe()),
        hostd: hostd_state(bundled_cli),
        window_state: f.window_state,
        monitors: monitors(),
        settings: f.settings,
        has_projects: f.has_projects,
        temp_leftovers: f.temp_leftovers,
        stale_lock: stale_lock(&env.data_dir()),
    })
}

fn bundled_version(bundled_cli: &Path) -> Option<semver::Version> {
    match cli_state(bundled_cli) {
        Cli::Version(v) => Some(v),
        _ => None,
    }
}

pub struct Real<'a> {
    pub bundled_cli: &'a Path,
}

impl Exec for Real<'_> {
    fn replace_cli(&self) -> Result<(), String> {
        run(
            Command::new(self.bundled_cli).args(["install", "--upgrade"]),
            RECONCILE,
        )
        .map(|_| ())
    }

    /// `atlas hostd --reconcile`: no live upgrade on Windows, so an empty host is restarted and
    /// one with terminals running is kept (the app asks the user, as it always did).
    fn reconcile_host(&self) -> Result<(), String> {
        run(
            Command::new(self.bundled_cli).args(["hostd", "--reconcile"]),
            RECONCILE,
        )
        .map(|_| ())
    }
}

/// Perform `actions` (from [`plan_only`]) with the bundled CLI, logging each.
pub fn run_repair(actions: &[repair::Action], bundled_cli: &Path, log: &Log) {
    let done = repair::execute(actions, &Real { bundled_cli }, log);
    log.line(format!("repair: {done} of {} done", actions.len()));
}
