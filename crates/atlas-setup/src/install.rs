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
    if let Err(e) = &result {
        log.line(format!("FAILED: {e}"));
        shared.fail(
            "Atlas could not be installed",
            format!("{e}\n\nLog: {}", log.path().display()),
        );
    }
    result
}

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
    let mut child = host.start_installer(&installer, &install_dir)?;
    let started = Instant::now();
    let code = loop {
        match child.poll()? {
            Some(c) => break c,
            None => {
                let written = dir_bytes(&install_dir);
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
    cleanup(&work, log);
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
    shared.done("Atlas is ready");

    if opt.launch {
        match host.launch(&app) {
            Ok(()) => log.line("launched Atlas"),
            // The install itself succeeded: a launch failure is logged, not an error screen.
            Err(e) => log.line(format!("could not launch Atlas: {e}")),
        }
    }
    Ok(())
}

fn cleanup(work: &Path, log: &Log) {
    if let Err(e) = std::fs::remove_dir_all(work) {
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
    fn no_launch_installs_without_opening_atlas() {
        let (_t, e, log) = setup();
        let host = FakeHost::new(vec![("atlas-desktop.exe", 10)], 0);
        let shared = Shared::new(Mode::Install);
        run(&e, &host, &shared, &log, &opt(false)).unwrap();
        assert_eq!(shared.snapshot().phase, Phase::Done);
        assert!(host.launched.borrow().is_empty());
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
