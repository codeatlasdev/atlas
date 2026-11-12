//! The install sequence, with every side effect behind [`Host`] so the order, the progress and the
//! failure handling are tested for real with a fake host that writes files like NSIS does.
//!
//! ```text
//! unpack the embedded NSIS installer ─ close a running Atlas ─ run it `/S /D=<dir>` and watch the
//! folder grow ─ verify (exit 0 AND atlas-desktop.exe present) ─ open Atlas
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::fsutil::{Log, dir_bytes};
use crate::paths::Env;
use crate::state::Shared;

/// What the NSIS installer lays down, uncompressed: the denominator of the progress. Measured on a
/// real build (atlas-desktop.exe 26.8 MB + atlas.exe 30.3 MB + the uninstaller, about 54.6 MiB); only the bar's shape
/// depends on it (the bar is clamped, and 100 % needs the verification, not the byte count).
pub const EXPECTED_INSTALLED_BYTES: u64 = 55 * 1024 * 1024;

/// How long a running Atlas gets to close on its own before the installer ends it.
pub const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// A running NSIS installer.
pub trait Running {
    /// `Some(exit code)` once it ended; `None` while it runs.
    fn poll(&mut self) -> Result<Option<i32>, String>;
}

/// Everything the install does to the machine.
pub trait Host {
    /// Write the embedded installer into `dir` and return its path.
    fn unpack(&self, dir: &Path) -> Result<PathBuf, String>;
    /// Ask a running Atlas Desktop to close (and end it after `grace`). Never fails the install:
    /// the NSIS installer closes what is left.
    fn close_app(&self, install_dir: &Path, grace: Duration);
    /// Start the NSIS installer silently into `install_dir`.
    fn start_installer(
        &self,
        exe: &Path,
        install_dir: &Path,
    ) -> Result<Box<dyn Running + '_>, String>;
    /// Put this program into `install_dir` as the uninstaller Settings › Apps opens. Best effort:
    /// the NSIS uninstaller keeps working without it.
    fn register_uninstaller(&self, install_dir: &Path) -> Result<(), String>;
    /// Repair what older builds left behind (a stale CLI, an old terminal host, a window-state file
    /// that restores a second title bar, temp files; see [`crate::repair`]). `announce` is called
    /// once, before the first change, when there is something to do. Never fails the install:
    /// every action's failure is logged and the next one runs. Default: nothing (the test hosts).
    fn repair(&self, _env: &Env, _log: &Log, _announce: &dyn Fn()) {}
    /// Open Atlas.
    fn launch(&self, app: &Path) -> Result<(), String>;
}

pub struct Options {
    pub launch: bool,
    /// Sleep between polls of the installer; tests use zero.
    pub tick: Duration,
}

/// Run the install. On success the shared state is `Done`; on failure it is `Failed` with the
/// reason and a pointer to the log, and `Err` carries the same title.
pub fn run(
    env: &Env,
    host: &dyn Host,
    shared: &Shared,
    log: &Log,
    opt: &Options,
) -> Result<(), String> {
    let result = steps(env, host, shared, log, opt);
    // The unpacked installer goes on success and on every failure (a failed start or poll too).
    cleanup(&env.payload_dir(std::process::id()), log);
    if let Err(e) = &result {
        log.line(format!("FAILED: {e}"));
        shared.fail("Atlas could not be installed", failure_details(e, log));
    }
    result
}

/// What "Copy details" puts on the clipboard and the window shows under the title: the reason, the
/// log, and what to do next.
pub fn failure_details(reason: &str, log: &Log) -> String {
    format!("{reason}\nLog: {}\n{NEXT_STEP}", log.path().display())
}

/// Where an issue goes; added to the text "Copy details" puts on the clipboard.
pub const ISSUES_URL: &str = "github.com/codeatlasdev/atlas/issues";

/// The way out of a failed run, shown on screen and copied with the details.
pub const NEXT_STEP: &str =
    "Close Atlas and run Atlas-Setup.exe again. Still failing? Use Copy details and open an issue.";

fn steps(
    env: &Env,
    host: &dyn Host,
    shared: &Shared,
    log: &Log,
    opt: &Options,
) -> Result<(), String> {
    let install_dir = env.install_dir();
    let app = env.app_exe();
    log.line(format!("install into {}", install_dir.display()));

    shared.status("Preparing…");
    let work = env.payload_dir(std::process::id());
    let installer = host.unpack(&work)?;
    log.line(format!("unpacked installer: {}", installer.display()));
    shared.raise(0.04);

    shared.status("Closing Atlas…");
    host.close_app(&install_dir, CLOSE_GRACE);
    shared.raise(0.08);

    shared.status("Installing Atlas…");
    // A reinstall starts with the old install already on disk: only what the installer adds
    // counts, or the bar would jump straight to the end.
    let baseline = dir_bytes(&install_dir);
    let mut child = host.start_installer(&installer, &install_dir)?;
    let started = Instant::now();
    let code = loop {
        match child.poll()? {
            Some(c) => break c,
            None => {
                let written = dir_bytes(&install_dir).saturating_sub(baseline);
                shared.running(
                    written,
                    EXPECTED_INSTALLED_BYTES,
                    started.elapsed().as_secs_f32(),
                );
                std::thread::sleep(opt.tick);
            }
        }
    };
    log.line(format!("installer exited with {code}"));
    if code != 0 {
        return Err(format!("The Atlas installer failed (exit code {code})."));
    }

    // Exit 0 is not enough: the app must be where `atlas desktop` and the shortcuts expect it.
    shared.status("Finishing…");
    shared.raise(0.95);
    if !app.is_file() {
        return Err(format!(
            "The installer finished but {} is missing.",
            app.display()
        ));
    }
    log.line("verified: app present");
    if let Err(e) = host.register_uninstaller(&install_dir) {
        // The app is installed and works; Settings › Apps just keeps the plain NSIS uninstaller.
        log.line(format!("could not register the branded uninstaller: {e}"));
    }
    // The new files are in place: bring the leftovers of older builds in step with them.
    host.repair(env, log, &|| shared.status("Updating your setup…"));
    // The install itself succeeded: a launch that is skipped or fails is not an error screen, but
    // the window must not claim it is opening Atlas.
    let note = if opt.launch {
        match host.launch(&app) {
            Ok(()) => {
                log.line("launched Atlas");
                "Opening Atlas…"
            }
            Err(e) => {
                log.line(format!("could not launch Atlas: {e}"));
                NOT_OPENED
            }
        }
    } else {
        NOT_OPENED
    };
    shared.done("Atlas is ready", note);
    Ok(())
}

/// The note of a finished install that did not open Atlas (`--no-launch`, or the launch failed).
/// The window stays open on it.
pub const NOT_OPENED: &str = "Atlas is installed. Open it from the Start menu.";

/// Does the window close by itself after this finished install? Only when Atlas is opening.
pub fn closes_itself(note: &str) -> bool {
    note != NOT_OPENED
}

fn cleanup(work: &Path, log: &Log) {
    if let Err(e) = std::fs::remove_dir_all(work)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        log.line(format!("could not remove {}: {e}", work.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Mode;
    use crate::state::Phase;
    use std::cell::RefCell;

    /// Writes `files` into the install folder one per poll, then exits with `code`.
    struct FakeRun {
        dir: PathBuf,
        left: Vec<(&'static str, usize)>,
        code: i32,
    }

    impl Running for FakeRun {
        fn poll(&mut self) -> Result<Option<i32>, String> {
            match self.left.pop() {
                Some((name, size)) => {
                    std::fs::create_dir_all(&self.dir).unwrap();
                    std::fs::write(self.dir.join(name), vec![0u8; size]).unwrap();
                    Ok(None)
                }
                None => Ok(Some(self.code)),
            }
        }
    }

    struct FakeHost {
        files: Vec<(&'static str, usize)>,
        code: i32,
        launched: RefCell<Vec<PathBuf>>,
        closed: RefCell<u32>,
        order: RefCell<Vec<&'static str>>,
    }

    impl FakeHost {
        fn new(files: Vec<(&'static str, usize)>, code: i32) -> Self {
            Self {
                files,
                code,
                launched: Default::default(),
                closed: Default::default(),
                order: Default::default(),
            }
        }
    }

    impl Host for FakeHost {
        fn unpack(&self, dir: &Path) -> Result<PathBuf, String> {
            self.order.borrow_mut().push("unpack");
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let p = dir.join("Atlas-installer.exe");
            std::fs::write(&p, b"MZ").map_err(|e| e.to_string())?;
            Ok(p)
        }
        fn close_app(&self, _: &Path, _: Duration) {
            self.order.borrow_mut().push("close");
            *self.closed.borrow_mut() += 1;
        }
        fn start_installer(&self, _: &Path, dir: &Path) -> Result<Box<dyn Running + '_>, String> {
            self.order.borrow_mut().push("run");
            Ok(Box::new(FakeRun {
                dir: dir.to_path_buf(),
                left: self.files.clone(),
                code: self.code,
            }))
        }
        fn register_uninstaller(&self, _: &Path) -> Result<(), String> {
            self.order.borrow_mut().push("register");
            Ok(())
        }
        fn launch(&self, app: &Path) -> Result<(), String> {
            self.order.borrow_mut().push("launch");
            self.launched.borrow_mut().push(app.to_path_buf());
            Ok(())
        }
    }

    fn env(t: &Path) -> Env {
        Env {
            local_app_data: t.join("Local"),
            home: t.join("home"),
            temp: t.join("tmp"),
        }
    }

    fn setup() -> (tempfile::TempDir, Env, Log) {
        let t = tempfile::tempdir().unwrap();
        let e = env(t.path());
        std::fs::create_dir_all(&e.temp).unwrap();
        let log = Log::create(e.log_file());
        (t, e, log)
    }

    fn opt(launch: bool) -> Options {
        Options {
            launch,
            tick: Duration::ZERO,
        }
    }

    #[test]
    fn a_good_install_runs_in_order_reports_growing_progress_and_launches() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(
            vec![
                ("uninstall.exe", 10),
                ("atlas.exe", 10),
                ("atlas-desktop.exe", 10),
            ],
            0,
        );
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap();
        let snap = shared.snapshot();
        assert_eq!((snap.phase, snap.progress), (Phase::Done, 1.0));
        assert_eq!(
            *host.order.borrow(),
            ["unpack", "close", "run", "register", "launch"]
        );
        assert_eq!(*host.launched.borrow(), [e.app_exe()]);
        assert!(
            !e.payload_dir(std::process::id()).exists(),
            "the unpacked installer is removed"
        );
    }

    #[test]
    fn exit_zero_without_the_app_is_a_failure_not_a_success() {
        // Incident guard: 100 % only on success AND the app present.
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("uninstall.exe", 10)], 0);
        let shared = Shared::new(Mode::Install);
        let err = run(&e, &host, &shared, &log, &opt(true)).unwrap_err();
        assert!(err.contains("is missing"), "{err}");
        let snap = shared.snapshot();
        assert_eq!(snap.phase, Phase::Failed);
        assert!(snap.progress < 1.0);
        assert!(
            snap.details.contains("atlas-setup.log"),
            "the log path is in the details: {}",
            snap.details
        );
        assert!(
            host.launched.borrow().is_empty(),
            "never opens a half install"
        );
    }

    #[test]
    fn a_nonzero_exit_fails_with_the_code_even_if_the_app_exists() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("atlas-desktop.exe", 10)], 2);
        let shared = Shared::new(Mode::Install);
        let err = run(&e, &host, &shared, &log, &opt(true)).unwrap_err();
        assert!(err.contains("exit code 2"), "{err}");
        assert_eq!(shared.snapshot().phase, Phase::Failed);
        assert!(host.launched.borrow().is_empty());
        let logged = std::fs::read_to_string(e.log_file()).unwrap();
        assert!(
            logged.contains("exited with 2") && logged.contains("FAILED"),
            "{logged}"
        );
    }

    #[test]
    fn a_failed_uninstaller_copy_does_not_fail_an_install_that_worked() {
        // The app is installed and runs; Settings > Apps just keeps the NSIS uninstaller.
        let (_t, e, log) = setup();
        struct NoCopy(FakeHost);
        impl Host for NoCopy {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, x: &Path, d: &Path) -> Result<Box<dyn Running + '_>, String> {
                self.0.start_installer(x, d)
            }
            fn register_uninstaller(&self, _: &Path) -> Result<(), String> {
                Err("access denied".into())
            }
            fn launch(&self, a: &Path) -> Result<(), String> {
                self.0.launch(a)
            }
        }
        let host = NoCopy(FakeHost::new(vec![("atlas-desktop.exe", 10)], 0));
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap();
        assert_eq!(shared.snapshot().phase, Phase::Done);
        assert_eq!(host.0.launched.borrow().len(), 1, "Atlas still opens");
        let logged = std::fs::read_to_string(e.log_file()).unwrap();
        assert!(logged.contains("access denied"), "{logged}");
    }

    #[test]
    fn no_launch_installs_without_opening_atlas_and_says_so() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("atlas-desktop.exe", 10)], 0);
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(false)).unwrap();
        let snap = shared.snapshot();
        assert_eq!(snap.phase, Phase::Done);
        assert!(host.launched.borrow().is_empty());
        assert_eq!(snap.note, NOT_OPENED, "never claims it is opening Atlas");
        assert!(!closes_itself(&snap.note), "the window stays for the user");
    }

    #[test]
    fn a_launch_that_fails_keeps_the_window_open_with_the_start_menu_hint() {
        let (_t, e, log) = setup();
        struct NoLaunch(FakeHost);
        impl Host for NoLaunch {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, x: &Path, d: &Path) -> Result<Box<dyn Running + '_>, String> {
                self.0.start_installer(x, d)
            }
            fn register_uninstaller(&self, d: &Path) -> Result<(), String> {
                self.0.register_uninstaller(d)
            }
            fn launch(&self, _: &Path) -> Result<(), String> {
                Err("blocked by policy".into())
            }
        }
        let host = NoLaunch(FakeHost::new(vec![("atlas-desktop.exe", 10)], 0));
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap();
        let snap = shared.snapshot();
        assert_eq!((snap.phase, snap.note.as_str()), (Phase::Done, NOT_OPENED));
        assert!(!closes_itself(&snap.note));
    }

    #[test]
    fn a_successful_launch_says_opening_and_closes_itself() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("atlas-desktop.exe", 10)], 0);
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap();
        let snap = shared.snapshot();
        assert_eq!(snap.note, "Opening Atlas…");
        assert!(closes_itself(&snap.note));
    }

    #[test]
    fn the_unpacked_installer_is_removed_when_the_installer_cannot_start() {
        // A failed start (not a bad exit code) used to leave Atlas-installer.exe in %TEMP%.
        let (_t, e, log) = setup();
        struct CantStart(FakeHost);
        impl Host for CantStart {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, _: &Path, _: &Path) -> Result<Box<dyn Running + '_>, String> {
                Err("cannot run installer".into())
            }
            fn register_uninstaller(&self, _: &Path) -> Result<(), String> {
                panic!("must not get here")
            }
            fn launch(&self, _: &Path) -> Result<(), String> {
                panic!("must not get here")
            }
        }
        let host = CantStart(FakeHost::new(vec![], 0));
        let shared = Shared::new(Mode::Install);
        assert!(run(&e, &host, &shared, &log, &opt(true)).is_err());
        assert!(
            !e.payload_dir(std::process::id()).exists(),
            "the temp copy of the installer is gone"
        );
    }

    #[test]
    fn a_reinstall_does_not_start_the_bar_at_the_end() {
        // The old install (55 MiB) is already in the folder: only the growth counts.
        let (_t, e, log) = setup();
        std::fs::create_dir_all(e.install_dir()).unwrap();
        std::fs::write(
            e.install_dir().join("atlas-desktop.exe"),
            vec![0u8; EXPECTED_INSTALLED_BYTES as usize],
        )
        .unwrap();
        struct Probe<'a> {
            shared: &'a Shared,
            seen: &'a RefCell<Vec<f32>>,
            left: u32,
        }
        impl Running for Probe<'_> {
            fn poll(&mut self) -> Result<Option<i32>, String> {
                self.seen.borrow_mut().push(self.shared.snapshot().progress);
                if self.left == 0 {
                    return Ok(Some(0));
                }
                self.left -= 1;
                Ok(None)
            }
        }
        struct H<'a>(FakeHost, &'a Shared, &'a RefCell<Vec<f32>>);
        impl Host for H<'_> {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, _: &Path, _: &Path) -> Result<Box<dyn Running + '_>, String> {
                Ok(Box::new(Probe {
                    shared: self.1,
                    seen: self.2,
                    left: 2,
                }))
            }
            fn register_uninstaller(&self, d: &Path) -> Result<(), String> {
                self.0.register_uninstaller(d)
            }
            fn launch(&self, a: &Path) -> Result<(), String> {
                self.0.launch(a)
            }
        }
        let shared = Shared::new(Mode::Install);
        let seen = RefCell::new(Vec::new());
        let host = H(FakeHost::new(vec![], 0), &shared, &seen);
        run(&e, &host, &shared, &log, &opt(false)).unwrap();
        let first = seen.borrow()[0];
        assert!(first < 0.5, "the bar starts near the beginning: {first}");
    }

    #[test]
    fn a_failure_screen_gives_the_log_and_the_next_step_in_few_lines() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("atlas-desktop.exe", 10)], 2);
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap_err();
        let d = shared.snapshot().details;
        assert!(
            d.contains("exit code 2")
                && d.contains("atlas-setup.log")
                && d.contains("run Atlas-Setup.exe again"),
            "{d}"
        );
        assert!(
            !d.contains("\n\n"),
            "no blank line eats the wrapped lines: {d:?}"
        );
    }

    #[test]
    fn progress_never_goes_back_while_the_folder_grows() {
        let (_t, e, log) = setup();
        // Observe the shared progress at every poll through a host whose Running samples it.
        struct Spy<'a> {
            inner: FakeRun,
            shared: &'a Shared,
            seen: &'a RefCell<Vec<f32>>,
        }
        impl Running for Spy<'_> {
            fn poll(&mut self) -> Result<Option<i32>, String> {
                self.seen.borrow_mut().push(self.shared.snapshot().progress);
                self.inner.poll()
            }
        }
        struct SpyHost<'a> {
            base: FakeHost,
            shared: &'a Shared,
            seen: &'a RefCell<Vec<f32>>,
        }
        impl Host for SpyHost<'_> {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.base.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.base.close_app(a, g)
            }
            fn start_installer(
                &self,
                _: &Path,
                dir: &Path,
            ) -> Result<Box<dyn Running + '_>, String> {
                Ok(Box::new(Spy {
                    inner: FakeRun {
                        dir: dir.to_path_buf(),
                        left: self.base.files.clone(),
                        code: 0,
                    },
                    shared: self.shared,
                    seen: self.seen,
                }))
            }
            fn register_uninstaller(&self, d: &Path) -> Result<(), String> {
                self.base.register_uninstaller(d)
            }
            fn launch(&self, a: &Path) -> Result<(), String> {
                self.base.launch(a)
            }
        }
        let shared = Shared::new(Mode::Install);
        let seen = RefCell::new(Vec::new());
        let host = SpyHost {
            base: FakeHost::new(
                vec![
                    ("atlas-desktop.exe", 4_000_000),
                    ("atlas.exe", 8_000_000),
                    ("uninstall.exe", 1_000),
                ],
                0,
            ),
            shared: &shared,
            seen: &seen,
        };
        run(&e, &host, &shared, &log, &opt(false)).unwrap();
        let seen = seen.borrow();
        assert!(seen.len() >= 3);
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "{seen:?}");
        assert!(
            seen.iter().all(|p| *p < 1.0),
            "full only after verification: {seen:?}"
        );
    }

    /// The repair of older builds' leftovers runs once the new files are verified (the bundled
    /// `atlas.exe` it asks is there) and before Atlas opens (the app must start on a repaired
    /// setup), and tells the window.
    #[test]
    fn leftovers_are_repaired_after_the_install_is_verified_and_before_atlas_opens() {
        let (_t, e, log) = setup();
        struct Repairing(FakeHost);
        impl Host for Repairing {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, x: &Path, d: &Path) -> Result<Box<dyn Running + '_>, String> {
                self.0.start_installer(x, d)
            }
            fn register_uninstaller(&self, d: &Path) -> Result<(), String> {
                self.0.register_uninstaller(d)
            }
            fn repair(&self, env: &Env, _: &Log, announce: &dyn Fn()) {
                assert!(env.app_exe().is_file(), "the new app is already in place");
                self.0.order.borrow_mut().push("repair");
                announce();
            }
            fn launch(&self, a: &Path) -> Result<(), String> {
                self.0.launch(a)
            }
        }
        let host = Repairing(FakeHost::new(vec![("atlas-desktop.exe", 10)], 0));
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(true)).unwrap();
        assert_eq!(
            *host.0.order.borrow(),
            ["unpack", "close", "run", "register", "repair", "launch"]
        );
        assert_eq!(shared.snapshot().phase, Phase::Done);
    }

    /// A failed install runs no repair: nothing was replaced, so nothing is stale yet.
    #[test]
    fn a_failed_install_repairs_nothing() {
        let (_t, e, log) = setup();
        struct Repairing(FakeHost);
        impl Host for Repairing {
            fn unpack(&self, d: &Path) -> Result<PathBuf, String> {
                self.0.unpack(d)
            }
            fn close_app(&self, a: &Path, g: Duration) {
                self.0.close_app(a, g)
            }
            fn start_installer(&self, x: &Path, d: &Path) -> Result<Box<dyn Running + '_>, String> {
                self.0.start_installer(x, d)
            }
            fn register_uninstaller(&self, d: &Path) -> Result<(), String> {
                self.0.register_uninstaller(d)
            }
            fn repair(&self, _: &Env, _: &Log, _: &dyn Fn()) {
                panic!("must not run after a failed install")
            }
            fn launch(&self, a: &Path) -> Result<(), String> {
                self.0.launch(a)
            }
        }
        let host = Repairing(FakeHost::new(vec![("atlas-desktop.exe", 10)], 2));
        let shared = Shared::new(Mode::Install);
        assert!(run(&e, &host, &shared, &log, &opt(true)).is_err());
    }

    #[test]
    fn an_unpack_failure_is_reported_with_the_log_path() {
        let (_t, e, log) = setup();
        struct Broken;
        impl Host for Broken {
            fn unpack(&self, _: &Path) -> Result<PathBuf, String> {
                Err("This setup carries no installer.".into())
            }
            fn close_app(&self, _: &Path, _: Duration) {
                panic!("must not get here")
            }
            fn start_installer(&self, _: &Path, _: &Path) -> Result<Box<dyn Running + '_>, String> {
                panic!("must not get here")
            }
            fn register_uninstaller(&self, _: &Path) -> Result<(), String> {
                panic!("must not get here")
            }
            fn launch(&self, _: &Path) -> Result<(), String> {
                panic!("must not get here")
            }
        }
        let shared = Shared::new(Mode::Install);
        assert!(run(&e, &Broken, &shared, &log, &opt(true)).is_err());
        let snap = shared.snapshot();
        assert_eq!(snap.phase, Phase::Failed);
        assert!(snap.details.contains("carries no installer"));
    }
}
