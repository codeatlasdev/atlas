//! The uninstall sequence, behind [`Host`] like the install.
//!
//! ```text
//! (ask: keep %USERPROFILE%\.atlas? deleting asks a second time) ─ close Atlas ─ NSIS
//! `uninstall.exe /S _?=<dir>` (synchronous) ─ [remove data: `atlas.exe uninstall --yes --purge`] ─ clean
//! what NSIS leaves ─ verify the app is gone ─ schedule our own copy for deletion
//! ```
//!
//! The default, everywhere (the window's default button, a silent run), is to **keep** the user's
//! data: removing it is always an explicit, twice-confirmed choice. Closing the window on the
//! question cancels the uninstall before anything is touched.

use std::path::Path;
use std::time::Duration;

use crate::cli::DataChoice;
use crate::fsutil::Log;
use crate::install::{CLOSE_GRACE, failure_details};
use crate::paths::{Env, is_product_dir};
use crate::state::{Answer, Shared};

/// The window's note while the uninstall runs and the data is kept (S1: never contradicts the choice).
pub const KEEPING: &str = "Your data in %USERPROFILE%\\.atlas stays.";
/// The same while the data is being deleted.
pub const DELETING: &str = "Deleting your Atlas data.";

pub trait Host {
    fn close_app(&self, install_dir: &Path, grace: Duration);
    /// `atlas.exe uninstall --yes --purge`: removes `%USERPROFILE%\.atlas`, the CLI and its PATH entry. `Err` carries
    /// what it printed.
    fn remove_cli_and_data(&self, cli: &Path) -> Result<(), String>;
    /// Run the NSIS uninstaller silently and wait; its exit code.
    fn run_uninstaller(&self, uninstaller: &Path, install_dir: &Path) -> Result<i32, String>;
    /// Delete `path` once this process has exited (a running program cannot delete itself on
    /// Windows). Best effort; the files are left only if the OS refuses.
    fn delete_after_exit(&self, path: &Path) -> Result<(), String>;
}

pub struct Options {
    pub data: DataChoice,
    /// Wait between polls for the window's answer; tests use zero.
    pub tick: Duration,
}

/// Does the answer remove `%USERPROFILE%\.atlas`? `Ask` has no answer yet and means keep.
pub fn removes_data(choice: DataChoice) -> bool {
    choice == DataChoice::Remove
}

/// `Err` of a run the user cancelled at the question (the window shows nothing: it just closes).
pub const CANCELLED: &str = "cancelled";

pub fn run(
    env: &Env,
    host: &dyn Host,
    shared: &Shared,
    log: &Log,
    opt: &Options,
) -> Result<(), String> {
    let result = steps(env, host, shared, log, opt);
    if let Err(e) = &result
        && e != CANCELLED
    {
        log.line(format!("FAILED: {e}"));
        shared.fail("Atlas could not be removed", failure_details(e, log));
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
            let answer = loop {
                if let Some(a) = shared.data_answer() {
                    break a;
                }
                std::thread::sleep(opt.tick);
            };
            match answer {
                Answer::Keep => DataChoice::Keep,
                Answer::Remove => DataChoice::Remove,
                Answer::Cancel => {
                    // Nothing was touched yet: the user backed out.
                    log.line("uninstall cancelled by the user");
                    return Err(CANCELLED.into());
                }
            }
        }
        fixed => fixed,
    };
    shared.note(if removes_data(choice) {
        DELETING
    } else {
        KEEPING
    });
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
    let was_installed = uninstaller.is_file() || env.app_exe().exists();
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
        shared.status("Deleting your Atlas data…");
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
    leftovers(env, host, &install_dir, log);
    if env.app_exe().exists() {
        return Err(format!(
            "{} is still there after the uninstall.",
            env.app_exe().display()
        ));
    }
    if !was_installed {
        shared.done("Atlas was not installed here", "Nothing to remove.");
    } else if removes_data(choice) {
        shared.done("Atlas was removed", "Your Atlas data was deleted too.");
    } else {
        shared.done(
            "Atlas was removed",
            "Your data and the atlas command stay in %USERPROFILE%\\.atlas.",
        );
    }
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
/// Our own copy is running (Settings › Apps starts it from here), so Windows refuses to delete it:
/// it is handed to [`Host::delete_after_exit`], which also takes the folder.
fn leftovers(env: &Env, host: &dyn Host, install_dir: &Path, log: &Log) {
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
            log.line(format!(
                "{} is in use, deleting it after exit: {e}",
                p.display()
            ));
            if let Err(e) = host.delete_after_exit(&p) {
                log.line(format!(
                    "could not schedule the deletion of {}: {e}",
                    p.display()
                ));
            }
        }
    }
    // Only an empty folder goes (`remove_dir` refuses a full one): never a user's files. When our
    // own copy is still running the folder is not empty yet; the scheduled cleanup takes it.
    if std::fs::remove_dir(install_dir).is_err()
        && install_dir.exists()
        && let Err(e) = host.delete_after_exit(install_dir)
    {
        log.line(format!(
            "could not schedule the deletion of {}: {e}",
            install_dir.display()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Mode;
    use crate::state::Phase;
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct Fake {
        env: Env,
        code: i32,
        calls: RefCell<Vec<String>>,
        /// What the uninstaller deletes (the app, like NSIS does).
        deletes_app: bool,
        /// Files Windows would refuse to delete (our own running copy): they are scheduled instead.
        running: Vec<PathBuf>,
        scheduled: RefCell<Vec<PathBuf>>,
    }

    impl Fake {
        fn new(env: &Env, code: i32, deletes_app: bool) -> Self {
            Self {
                env: env.clone(),
                code,
                calls: Default::default(),
                deletes_app,
                running: Vec::new(),
                scheduled: Default::default(),
            }
        }
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
        fn delete_after_exit(&self, path: &Path) -> Result<(), String> {
            self.scheduled.borrow_mut().push(path.to_path_buf());
            Ok(())
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
        let host = Fake::new(&e, 0, true);
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
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap();
        assert_eq!(*host.calls.borrow(), ["close", "nsis", "cli+data"]);
        assert!(!e.data_dir().exists());
    }

    #[test]
    fn remove_data_without_a_cli_removes_the_data_folder_by_name() {
        let (_t, e, log) = installed();
        std::fs::remove_file(e.cli_exe()).unwrap();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap();
        assert!(!host.calls.borrow().contains(&"cli+data".to_string()));
        assert!(!e.data_dir().exists());
    }

    #[test]
    fn the_question_blocks_until_answered_and_keep_wins_a_no() {
        let (_t, e, log) = installed();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        let s2 = shared.clone();
        let answer = std::thread::spawn(move || {
            while s2.snapshot().phase != Phase::AskData {
                std::thread::yield_now();
            }
            s2.answer_data(Answer::Keep);
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
        let host = Fake::new(&e, 5, false);
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
        let host = Fake::new(&e, 0, false);
        let shared = Shared::new(Mode::Uninstall);
        let err = run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap_err();
        assert!(err.contains("still there"), "{err}");
    }

    #[test]
    fn nothing_installed_is_not_an_error() {
        let (_t, e, log) = installed();
        std::fs::remove_dir_all(e.install_dir()).unwrap();
        let host = Fake::new(&e, 0, false);
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        assert_eq!(shared.snapshot().phase, Phase::Done);
    }

    /// Answer the way the window does: each answer waits for an open question, then for the
    /// phase to move on (so two answers in a row are two separate clicks).
    fn answering(shared: &Shared, answers: Vec<Answer>) -> std::thread::JoinHandle<()> {
        let s = shared.clone();
        std::thread::spawn(move || {
            for a in answers {
                while !matches!(s.snapshot().phase, Phase::AskData | Phase::ConfirmRemove) {
                    std::thread::yield_now();
                }
                let before = s.snapshot().phase;
                s.answer_data(a);
                while s.snapshot().phase == before {
                    std::thread::yield_now();
                }
            }
        })
    }

    fn ask_opt() -> Options {
        Options {
            data: DataChoice::Ask,
            tick: Duration::from_millis(1),
        }
    }

    #[test]
    fn delete_needs_two_clicks_and_then_removes_the_data() {
        let (_t, e, log) = installed();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        let t = answering(&shared, vec![Answer::Remove, Answer::Remove]);
        run(&e, &host, &shared, &log, &ask_opt()).unwrap();
        t.join().unwrap();
        assert_eq!(*host.calls.borrow(), ["close", "nsis", "cli+data"]);
        assert!(!e.data_dir().exists());
        let snap = shared.snapshot();
        assert_eq!(snap.note, "Your Atlas data was deleted too.");
    }

    #[test]
    fn one_click_on_delete_followed_by_go_back_keeps_everything() {
        let (_t, e, log) = installed();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        let t = answering(&shared, vec![Answer::Remove, Answer::Keep, Answer::Keep]);
        run(&e, &host, &shared, &log, &ask_opt()).unwrap();
        t.join().unwrap();
        assert_eq!(*host.calls.borrow(), ["close", "nsis"]);
        assert!(e.data_dir().join("settings.json").is_file());
    }

    #[test]
    fn closing_the_question_cancels_before_anything_is_touched() {
        let (_t, e, log) = installed();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        let t = answering(&shared, vec![Answer::Cancel]);
        let err = run(&e, &host, &shared, &log, &ask_opt()).unwrap_err();
        t.join().unwrap();
        assert_eq!(err, CANCELLED);
        assert!(host.calls.borrow().is_empty(), "no close, no uninstall");
        assert_eq!(shared.snapshot().phase, Phase::Cancelled);
        assert!(e.app_exe().is_file() && e.data_dir().join("settings.json").is_file());
    }

    #[test]
    fn the_running_own_copy_is_deleted_after_exit_not_left_behind() {
        // Settings > Apps runs "Atlas Uninstall.exe" from the install folder: Windows refuses to
        // delete it while it runs, so the cleanup is scheduled for after the process ends.
        let (_t, e, log) = installed();
        let own = e.install_dir().join(crate::paths::OWN_COPY);
        let mut host = Fake::new(&e, 0, true);
        host.running = vec![own.clone()];
        // Model the refusal: a directory cannot be removed with remove_file, standing in for a
        // locked image.
        std::fs::remove_file(&own).unwrap();
        std::fs::create_dir(&own).unwrap();
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        let scheduled = host.scheduled.borrow();
        assert!(scheduled.contains(&own), "{scheduled:?}");
        assert!(
            scheduled.contains(&e.install_dir()),
            "the folder is handed over too: {scheduled:?}"
        );
        assert_eq!(shared.snapshot().phase, Phase::Done);
    }

    #[test]
    fn nothing_installed_says_so_instead_of_claiming_a_removal() {
        let (_t, e, log) = installed();
        std::fs::remove_dir_all(e.install_dir()).unwrap();
        let host = Fake::new(&e, 0, false);
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        let snap = shared.snapshot();
        assert_eq!(snap.status, "Atlas was not installed here");
        assert_eq!(snap.note, "Nothing to remove.");
    }

    #[test]
    fn the_note_never_contradicts_the_choice() {
        let (_t, e, log) = installed();
        let shared = Shared::new(Mode::Uninstall);
        let host = Fake::new(&e, 5, false);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap_err();
        assert_eq!(shared.snapshot().note, KEEPING);
        let (_t, e, log) = installed();
        let shared = Shared::new(Mode::Uninstall);
        let host = Fake::new(&e, 5, false);
        run(&e, &host, &shared, &log, &opt(DataChoice::Remove)).unwrap_err();
        assert_eq!(shared.snapshot().note, DELETING);
    }

    #[test]
    fn keep_says_where_the_data_and_the_cli_stay() {
        let (_t, e, log) = installed();
        let host = Fake::new(&e, 0, true);
        let shared = Shared::new(Mode::Uninstall);
        run(&e, &host, &shared, &log, &opt(DataChoice::Keep)).unwrap();
        assert!(shared.snapshot().note.contains("%USERPROFILE%\\.atlas"));
    }

    #[test]
    fn only_remove_means_remove() {
        assert!(removes_data(DataChoice::Remove));
        assert!(!removes_data(DataChoice::Keep));
        assert!(!removes_data(DataChoice::Ask));
    }
}
