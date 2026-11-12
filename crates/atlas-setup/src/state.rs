//! What the worker thread tells the window: a phase, a status line, the progress and, on failure,
//! the message the "Copy details" button puts on the clipboard.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::cli::Mode;
use crate::progress::Progress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Working,
    /// Uninstall only: waiting for the user to pick what happens with `%USERPROFILE%\.atlas`.
    AskData,
    /// Uninstall only: the user chose to delete the data; one more click confirms it.
    ConfirmRemove,
    Done,
    Failed,
    /// Uninstall only: the user backed out before anything was touched. The window closes.
    Cancelled,
}

/// The user's answer to [`Phase::AskData`] / [`Phase::ConfirmRemove`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Keep,
    Remove,
    Cancel,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub mode: Mode,
    pub phase: Phase,
    pub status: String,
    /// The line under the title while working or done (what is about to happen, or what to do
    /// next). Empty: the renderer's default for the phase.
    pub note: String,
    pub progress: f32,
    /// Full explanation of a failure (what the log says), for "Copy details".
    pub details: String,
}

impl Snapshot {
    /// What a screen reader should announce: the window title plus the current screen's text.
    /// (The window is custom-drawn and exposes no accessible text of its own.)
    pub fn accessible_text(&self) -> String {
        let mut t = String::from("Atlas Setup");
        for part in [self.status.as_str(), self.note.as_str()] {
            if !part.is_empty() {
                t.push_str(" - ");
                t.push_str(part);
            }
        }
        t
    }
}

#[derive(Debug)]
struct Inner {
    mode: Mode,
    phase: Phase,
    status: String,
    note: String,
    progress: Progress,
    details: String,
    answer: Option<Answer>,
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
            note: String::new(),
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
            note: i.note.clone(),
            progress: i.progress.value(),
            details: i.details.clone(),
        }
    }

    pub fn status(&self, text: impl Into<String>) {
        self.lock().status = text.into();
    }

    /// The line under the title (see [`Snapshot::note`]).
    pub fn note(&self, text: impl Into<String>) {
        self.lock().note = text.into();
    }

    pub fn raise(&self, v: f32) {
        self.lock().progress.raise(v);
    }

    pub fn running(&self, written: u64, expected: u64, secs: f32) {
        self.lock().progress.running(written, expected, secs);
    }

    /// Success: the bar fills and the phase is final. `note` is the line under the title.
    pub fn done(&self, text: impl Into<String>, note: impl Into<String>) {
        let mut i = self.lock();
        if i.phase == Phase::Failed {
            return;
        }
        i.progress.finish();
        i.status = text.into();
        i.note = note.into();
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

    /// The window's answer to [`Shared::ask_data`] or the confirmation that follows "delete".
    ///
    /// `Remove` on the first question only moves to the confirmation (nothing is decided yet);
    /// the worker sees `Remove` once the user confirmed it.
    pub fn answer_data(&self, a: Answer) {
        let mut i = self.lock();
        match (i.phase, a) {
            (Phase::AskData, Answer::Remove) => i.phase = Phase::ConfirmRemove,
            (Phase::AskData | Phase::ConfirmRemove, Answer::Cancel) => {
                i.answer = Some(Answer::Cancel);
                i.phase = Phase::Cancelled;
            }
            (Phase::ConfirmRemove, Answer::Keep) => {
                // "Go back": the same question again, nothing decided.
                i.phase = Phase::AskData;
            }
            (Phase::AskData | Phase::ConfirmRemove, a) => {
                i.answer = Some(a);
                i.phase = Phase::Working;
                i.status = "Removing Atlas…".into();
            }
            _ => {}
        }
    }

    /// The worker polls this while asking; `None` until the user answers.
    pub fn data_answer(&self) -> Option<Answer> {
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
        s.done("Atlas is ready", "");
        assert_eq!(s.snapshot().phase, Phase::Failed);
        assert!(s.snapshot().progress < 1.0);
    }

    #[test]
    fn done_fills_the_bar() {
        let s = Shared::new(Mode::Install);
        s.raise(0.3);
        s.done("Atlas is ready", "Opening Atlas…");
        let snap = s.snapshot();
        assert_eq!((snap.phase, snap.progress), (Phase::Done, 1.0));
        assert_eq!(snap.note, "Opening Atlas…");
    }

    #[test]
    fn keep_resumes_working_without_a_second_step() {
        let s = Shared::new(Mode::Uninstall);
        s.ask_data();
        assert_eq!(s.snapshot().phase, Phase::AskData);
        assert_eq!(s.data_answer(), None);
        s.answer_data(Answer::Keep);
        assert_eq!(s.data_answer(), Some(Answer::Keep));
        assert_eq!(s.snapshot().phase, Phase::Working);
    }

    #[test]
    fn deleting_data_needs_a_second_confirmation() {
        // Incident guard: one click must never reach the worker as "remove".
        let s = Shared::new(Mode::Uninstall);
        s.ask_data();
        s.answer_data(Answer::Remove);
        assert_eq!(s.snapshot().phase, Phase::ConfirmRemove);
        assert_eq!(s.data_answer(), None, "the worker is still waiting");
        s.answer_data(Answer::Remove);
        assert_eq!(s.data_answer(), Some(Answer::Remove));
        assert_eq!(s.snapshot().phase, Phase::Working);
    }

    #[test]
    fn going_back_from_the_confirmation_asks_again() {
        let s = Shared::new(Mode::Uninstall);
        s.ask_data();
        s.answer_data(Answer::Remove);
        s.answer_data(Answer::Keep);
        assert_eq!(s.snapshot().phase, Phase::AskData);
        assert_eq!(s.data_answer(), None);
    }

    #[test]
    fn cancelling_stops_the_uninstall_before_anything_is_touched() {
        for first in [false, true] {
            let s = Shared::new(Mode::Uninstall);
            s.ask_data();
            if first {
                s.answer_data(Answer::Remove);
            }
            s.answer_data(Answer::Cancel);
            assert_eq!(s.data_answer(), Some(Answer::Cancel));
            assert_eq!(s.snapshot().phase, Phase::Cancelled);
        }
    }

    #[test]
    fn an_answer_outside_the_question_changes_nothing() {
        let s = Shared::new(Mode::Uninstall);
        s.answer_data(Answer::Remove);
        assert_eq!(
            (s.snapshot().phase, s.data_answer()),
            (Phase::Working, None)
        );
    }

    #[test]
    fn the_accessible_text_carries_the_title_status_and_note() {
        let s = Shared::new(Mode::Install);
        s.done("Atlas is ready", "Opening Atlas…");
        assert_eq!(
            s.snapshot().accessible_text(),
            "Atlas Setup - Atlas is ready - Opening Atlas…"
        );
        assert_eq!(
            Shared::new(Mode::Install).snapshot().accessible_text(),
            "Atlas Setup - Installing Atlas…"
        );
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
