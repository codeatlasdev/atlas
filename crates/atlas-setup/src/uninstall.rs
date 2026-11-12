//! The uninstall sequence, behind [`Host`] like the install.
//!
//! ```text
//! close Atlas ─ (ask: keep ~/.atlas?) ─ [remove data: `atlas.exe uninstall --yes`] ─ NSIS
//! `uninstall.exe /S _?=<dir>` (synchronous) ─ clean what NSIS leaves ─ verify the app is gone
//! ```
//!
//! The default, everywhere (the window's default button, a silent run, a closed window), is to
//! **keep** the user's data: removing it is always an explicit choice.

use std::path::Path;
use std::time::Duration;

use crate::cli::DataChoice;
use crate::fsutil::Log;
use crate::install::CLOSE_GRACE;
use crate::paths::{Env, is_product_dir};
use crate::state::Shared;

pub trait Host {
    fn close_app(&self, install_dir: &Path, grace: Duration);
    /// `atlas.exe uninstall --yes`: removes `~/.atlas`, the CLI and its PATH entry. `Err` carries
    /// what it printed.
    fn remove_cli_and_data(&self, cli: &Path) -> Result<(), String>;
    /// Run the NSIS uninstaller silently and wait; its exit code.
    fn run_uninstaller(&self, uninstaller: &Path, install_dir: &Path) -> Result<i32, String>;
}

pub struct Options {
    pub data: DataChoice,
    /// Wait between polls for the window's answer; tests use zero.
    pub tick: Duration,
}

/// Does the answer remove `~/.atlas`? `Ask` has no answer yet and means keep.
pub fn removes_data(choice: DataChoice) -> bool {
    choice == DataChoice::Remove
}

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
            "Atlas could not be removed",
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
    log.line(format!("uninstall from {}", install_dir.display()));

    let choice = match opt.data {
        DataChoice::Ask => {
            shared.ask_data();
            loop {
                if let Some(remove) = shared.data_answer() {
                    break if remove {
                        DataChoice::Remove
                    } else {
                        DataChoice::Keep
                    };
                }
                std::thread::sleep(opt.tick);
            }
        }
        fixed => fixed,
    };
    log.line(format!(
        "user data: {}",
        if removes_data(choice) {
            "remove"
        } else {
            "keep"
        }
    ));

    shared.status("Closing Atlas…");
    host.close_app(&install_dir, CLOSE_GRACE);
    shared.raise(0.10);

    shared.status("Removing Atlas…");
    let uninstaller = env.nsis_uninstaller();
    if uninstaller.is_file() {
        let code = host.run_uninstaller(&uninstaller, &install_dir)?;
        log.line(format!("uninstaller exited with {code}"));
        if code != 0 {
            return Err(format!("The Atlas uninstaller failed (exit code {code})."));
        }
    } else {
        // Nothing registered the app here (a half install, or already removed): not an error, the
        // leftovers below still get cleaned.
        log.line("no uninstall.exe: nothing to run");
    }
    shared.raise(0.7);

    if removes_data(choice) {
        shared.status("Removing your Atlas data…");
        let cli = env.cli_exe();
        if cli.is_file() {
            host.remove_cli_and_data(&cli)?;
        } else {
            // No CLI to do it (it was never installed, or already gone): the data folder is ours
            // to remove only by name.
            remove_data_dir(&env.data_dir(), log)?;
        }
    }
    shared.raise(0.9);

    // `_?=` keeps NSIS from copying itself away, so uninstall.exe and our own copy are still here.
    leftovers(env, &install_dir, log);
    if env.app_exe().exists() {
        return Err(format!(
            "{} is still there after the uninstall.",
            env.app_exe().display()
        ));
    }
    shared.done("Atlas was removed");
    Ok(())
}

fn remove_data_dir(dir: &Path, log: &Log) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            log.line(format!("could not remove {}: {e}", dir.display()));
            Err(format!("Could not remove {}: {e}", dir.display()))
        }
    }
}

/// What NSIS leaves with `_?=`: the uninstaller itself, our own copy, and the folder. Only ever
/// the product folder, and only what is in it (a bad environment never aims this at another path).
fn leftovers(env: &Env, install_dir: &Path, log: &Log) {
    if !is_product_dir(install_dir, &env.local_app_data) {
        log.line(format!(
            "not cleaning {}: not the product folder",
            install_dir.display()
        ));
        return;
    }
    for name in [crate::paths::NSIS_UNINSTALLER, crate::paths::OWN_COPY] {
        let p = install_dir.join(name);
        if let Err(e) = std::fs::remove_file(&p)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            // Our own running copy cannot be deleted on Windows: it is removed after exit.
            log.line(format!("kept {}: {e}", p.display()));
        }
    }
    // Only an empty folder goes (`remove_dir` refuses a full one): never a user's files.
    let _ = std::fs::remove_dir(install_dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Mode;
    use crate::state::Phase;
    use std::cell::RefCell;

    struct Fake {
        env: Env,
        code: i32,
        calls: RefCell<Vec<String>>,
        /// What the uninstaller deletes (the app, like NSIS does).
        deletes_app: bool,
    }

    impl Host for Fake {
        fn close_app(&self, _: &Path, _: Duration) {
            self.calls.borrow_mut().push("close".into());
        }
        fn remove_cli_and_data(&self, cli: &Path) -> Result<(), String> {
            self.calls.borrow_mut().push("cli+data".into());
            std::fs::remove_dir_all(cli.parent().unwrap().parent().unwrap())
                .map_err(|e| e.to_string())
        }
        fn run_uninstaller(&self, _: &Path, _: &Path) -> Result<i32, String> {
            self.calls.borrow_mut().push("nsis".into());
            if self.deletes_app && self.code == 0 {
                // What the real uninstall.exe deletes (installer.nsi, Section Uninstall).
                std::fs::remove_file(self.env.app_exe()).unwrap();
                std::fs::remove_file(self.env.install_dir().join("atlas.exe")).unwrap();
            }
            Ok(self.code)
        }
    }

    /// An installed Atlas plus user data under a temp profile.
    fn installed() -> (tempfile::TempDir, Env, Log) {
        let t = tempfile::tempdir().unwrap();
        let e = Env {
            local_app_data: t.path().join("Local"),
            home: t.path().join("home"),
            temp: t.path().join("tmp"),
        };
        std::fs::create_dir_all(&e.temp).unwrap();
        std::fs::create_dir_all(e.install_dir()).unwrap();
        for f in [
            "atlas-desktop.exe",
            "atlas.exe",
            "uninstall.exe",
            "Atlas Uninstall.exe",
        ] {
            std::fs::write(e.install_dir().join(f), b"MZ").unwrap();
        }
        std::fs::create_dir_all(e.cli_exe().parent().unwrap()).unwrap();
        std::fs::write(e.cli_exe(), b"MZ").unwrap();
        std::fs::write(e.data_dir().join("settings.json"), b"{}").unwrap();
        let log = Log::create(e.log_file());
        (t, e, log)
    }

    fn opt(data: DataChoice) -> Options {
        Options {
            data,
            tick: Duration::ZERO,
        }
    }

    #[test]
    fn keep_removes_the_app_and_leaves_every_byte_of_user_data() {
        let (_t, e, log) = installed();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: true,
        };
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        assert_eq!(shared.snapshot().phase, Phase::Done);
        assert_eq!(
            *host.calls.borrow(),
            ["close", "nsis"],
            "the CLI and data are not touched"
        );
        assert!(e.data_dir().join("settings.json").is_file());
        assert!(e.cli_exe().is_file());
        assert!(
            !e.install_dir().exists(),
            "the empty product folder and our own copy are cleaned"
        );
    }

    #[test]
    fn remove_data_runs_atlas_uninstall_after_the_app_is_gone() {
        let (_t, e, log) = installed();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: true,
        };
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap();
        assert_eq!(*host.calls.borrow(), ["close", "nsis", "cli+data"]);
        assert!(!e.data_dir().exists());
    }

    #[test]
    fn remove_data_without_a_cli_removes_the_data_folder_by_name() {
        let (_t, e, log) = installed();
        std::fs::remove_file(e.cli_exe()).unwrap();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: true,
        };
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap();
        assert!(!host.calls.borrow().contains(&"cli+data".to_string()));
        assert!(!e.data_dir().exists());
    }

    #[test]
    fn the_question_blocks_until_answered_and_keep_wins_a_no() {
        let (_t, e, log) = installed();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: true,
        };
        let shared = Shared::new(Mode::Uninstall);
        let s2 = shared.clone();
        let answer = std::thread::spawn(move || {
            while s2.snapshot().phase != Phase::AskData {
                std::thread::yield_now();
            }
            s2.answer_data(false);
        });
        run(
            &e,
            &host,
            &shared,
            &log,
            &Options {
                data: DataChoice::Ask,
                tick: Duration::from_millis(1),
            },
        )
        .unwrap();
        answer.join().unwrap();
        assert!(e.data_dir().join("settings.json").is_file());
    }

    #[test]
    fn a_failing_uninstaller_is_an_error_and_never_touches_data() {
        let (_t, e, log) = installed();
        let host = Fake {
            env: e.clone(),
            code: 5,
            calls: Default::default(),
            deletes_app: false,
        };
        let shared = Shared::new(Mode::Uninstall);
        let err = run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap_err();
        assert!(err.contains("exit code 5"), "{err}");
        assert_eq!(shared.snapshot().phase, Phase::Failed);
        assert!(
            e.data_dir().join("settings.json").is_file(),
            "data is removed only after the app is"
        );
        assert!(!host.calls.borrow().contains(&"cli+data".to_string()));
    }

    #[test]
    fn exit_zero_with_the_app_still_there_is_a_failure() {
        let (_t, e, log) = installed();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: false,
        };
        let shared = Shared::new(Mode::Uninstall);
        let err = run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap_err();
        assert!(err.contains("still there"), "{err}");
    }

    #[test]
    fn nothing_installed_is_not_an_error() {
        let (_t, e, log) = installed();
        std::fs::remove_dir_all(e.install_dir()).unwrap();
        let host = Fake {
            env: e.clone(),
            code: 0,
            calls: Default::default(),
            deletes_app: false,
        };
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        assert_eq!(shared.snapshot().phase, Phase::Done);
    }

    #[test]
    fn only_remove_means_remove() {
        assert!(removes_data(DataChoice::Remove));
        assert!(!removes_data(DataChoice::Keep));
        assert!(!removes_data(DataChoice::Ask));
    }
}
