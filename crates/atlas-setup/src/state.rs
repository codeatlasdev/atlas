//! What the worker thread tells the window: a phase, a status line, the progress and, on failure,
//! the message the "Copy details" button puts on the clipboard.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::cli::Mode;
use crate::progress::Progress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Working,
    /// Uninstall only: waiting for the user to pick what happens with `~/.atlas`.
    AskData,
    Done,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub mode: Mode,
    pub phase: Phase,
    pub status: String,
    pub progress: f32,
    /// Full explanation of a failure (what the log says), for "Copy details".
    pub details: String,
}

#[derive(Debug)]
struct Inner {
    mode: Mode,
    phase: Phase,
    status: String,
    progress: Progress,
    details: String,
    /// The user's answer to [`Phase::AskData`]: `Some(true)` = remove `~/.atlas`.
    answer: Option<bool>,
}

/// Cheap to clone; the worker and the window each hold one.
#[derive(Clone, Debug)]
pub struct Shared(Arc<Mutex<Inner>>);

impl Shared {
    pub fn new(mode: Mode) -> Self {
        let status = match mode {
            Mode::Install => "Installing Atlas…",
            Mode::Uninstall => "Removing Atlas…",
        };
        Self(Arc::new(Mutex::new(Inner {
            mode,
            phase: Phase::Working,
            status: status.into(),
            progress: Progress::default(),
            details: String::new(),
            answer: None,
        })))
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panicking worker must not take the window (and its error message) down with it.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot(&self) -> Snapshot {
        let i = self.lock();
        Snapshot {
            mode: i.mode,
            phase: i.phase,
            status: i.status.clone(),
            progress: i.progress.value(),
            details: i.details.clone(),
        }
    }

    pub fn status(&self, text: impl Into<String>) {
        self.lock().status = text.into();
    }

    pub fn raise(&self, v: f32) {
        self.lock().progress.raise(v);
    }

    pub fn running(&self, written: u64, expected: u64, secs: f32) {
        self.lock().progress.running(written, expected, secs);
    }

    /// Success: the bar fills and the phase is final.
    pub fn done(&self, text: impl Into<String>) {
        let mut i = self.lock();
        if i.phase == Phase::Failed {
            return;
        }
        i.progress.finish();
        i.status = text.into();
        i.phase = Phase::Done;
    }

    /// Failure: `title` is the one line shown, `details` everything else. The bar stays where it
    /// was (never full).
    pub fn fail(&self, title: impl Into<String>, details: impl Into<String>) {
        let mut i = self.lock();
        i.status = title.into();
        i.details = details.into();
        i.phase = Phase::Failed;
    }

    pub fn ask_data(&self) {
        let mut i = self.lock();
        i.status = "Keep your Atlas data?".into();
        i.phase = Phase::AskData;
    }

    /// The window's answer to [`Shared::ask_data`].
    pub fn answer_data(&self, remove: bool) {
        let mut i = self.lock();
        i.answer = Some(remove);
        if i.phase == Phase::AskData {
            i.phase = Phase::Working;
            i.status = "Removing Atlas…".into();
        }
    }

    /// The worker polls this while asking; `None` until the user answers.
    pub fn data_answer(&self) -> Option<bool> {
        self.lock().answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_never_shows_a_full_bar() {
        let s = Shared::new(Mode::Install);
        s.running(900, 1_000, 5.0);
        s.fail("Could not install Atlas", "exit code 1");
        let snap = s.snapshot();
        assert_eq!(snap.phase, Phase::Failed);
        assert!(snap.progress < 1.0);
        assert_eq!(snap.details, "exit code 1");
    }

    #[test]
    fn done_after_a_failure_does_not_turn_the_error_into_success() {
        // Incident guard: a late "done" from a racing step must not hide the error.
        let s = Shared::new(Mode::Install);
        s.fail("Could not install Atlas", "boom");
        s.done("Atlas is ready");
        assert_eq!(s.snapshot().phase, Phase::Failed);
        assert!(s.snapshot().progress < 1.0);
    }

    #[test]
    fn done_fills_the_bar() {
        let s = Shared::new(Mode::Install);
        s.raise(0.3);
        s.done("Atlas is ready");
        let snap = s.snapshot();
        assert_eq!((snap.phase, snap.progress), (Phase::Done, 1.0));
    }

    #[test]
    fn the_data_question_waits_for_an_answer_and_resumes_working() {
        let s = Shared::new(Mode::Uninstall);
        s.ask_data();
        assert_eq!(s.snapshot().phase, Phase::AskData);
        assert_eq!(s.data_answer(), None);
        s.answer_data(false);
        assert_eq!(s.data_answer(), Some(false));
        assert_eq!(s.snapshot().phase, Phase::Working);
    }

    #[test]
    fn a_poisoned_lock_still_reports() {
        let s = Shared::new(Mode::Install);
        let s2 = s.clone();
        let _ = std::thread::spawn(move || {
            let _g = s2.0.lock().unwrap();
            panic!("worker died");
        })
        .join();
        s.fail("Could not install Atlas", "worker panicked");
        assert_eq!(s.snapshot().phase, Phase::Failed);
    }
}
