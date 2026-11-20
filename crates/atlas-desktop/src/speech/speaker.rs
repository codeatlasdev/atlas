//! The speaker: one utterance at a time, on a thread of its own that owns the platform [`Engine`]. The engine sits
//! behind a trait so the rules (a new utterance cuts the old one, `stop` is final, an utterance ends exactly once,
//! a blank text never reaches the engine) are tested with a fake, no sound card needed.
//!
//! The engine is built on the speaker's thread (`make`), never moved: a COM voice (Windows) and an Objective-C
//! synthesizer (macOS) live and die where they were made, and neither has to be `Send`.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// The languages the app speaks in (its display languages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    PtBr,
}

impl Lang {
    /// `en` / `pt-BR` as the page sends it (also `pt`, `pt_BR`, any case); anything else reads as English.
    pub fn parse(tag: &str) -> Self {
        let t = tag.trim().to_ascii_lowercase().replace('_', "-");
        if t == "pt" || t.starts_with("pt-") {
            Self::PtBr
        } else {
            Self::En
        }
    }

    /// The BCP-47 tag.
    #[cfg_attr(windows, allow(dead_code, reason = "SAPI picks voices by language id"))]
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::PtBr => "pt-BR",
        }
    }
}

/// What a platform provides.
pub trait Engine {
    /// Start speaking `text`; returns once the sound is queued, not when it is over.
    fn speak(&mut self, text: &str, lang: Lang) -> Result<(), String>;
    /// Silence at once. Safe when nothing is speaking.
    fn stop(&mut self);
    /// Still producing sound.
    fn speaking(&mut self) -> bool;
}

enum Cmd {
    Speak {
        text: String,
        lang: Lang,
        reply: Sender<Result<u64, String>>,
    },
    Stop,
}

/// How often a running utterance is checked for its end.
const POLL: Duration = Duration::from_millis(100);
/// A synthesizer reports "not speaking" for a moment after it was handed the text: that is not the end.
pub const GRACE: Duration = Duration::from_millis(600);

pub struct Speaker {
    tx: Sender<Cmd>,
}

impl Speaker {
    /// Start the thread. `emit(id)` is called once when utterance `id` ended by itself (never after a `stop`, and
    /// never for an utterance a newer one cut).
    pub fn spawn<E: Engine>(
        make: impl FnOnce() -> E + Send + 'static,
        emit: impl Fn(u64) + Send + 'static,
        grace: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("speech".into())
            .spawn(move || {
                let mut engine = make();
                let mut last = 0u64;
                // The utterance being waited on, and when it started.
                let mut active: Option<(u64, Instant)> = None;
                loop {
                    let cmd = if active.is_some() {
                        rx.recv_timeout(POLL)
                    } else {
                        rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    };
                    match cmd {
                        Ok(Cmd::Speak { text, lang, reply }) => {
                            engine.stop();
                            active = None;
                            let said = engine.speak(&text, lang);
                            let _ = reply.send(said.map(|()| {
                                last += 1;
                                active = Some((last, Instant::now()));
                                last
                            }));
                        }
                        Ok(Cmd::Stop) => {
                            engine.stop();
                            active = None;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if let Some((id, since)) = active
                                && since.elapsed() >= grace
                                && !engine.speaking()
                            {
                                active = None;
                                emit(id);
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            engine.stop();
                            return;
                        }
                    }
                }
            })
            .expect("the speech thread starts");
        Self { tx }
    }

    /// Speak `text`, cutting whatever is being said. The id names this utterance in the end event. A blank text is
    /// refused: there is nothing to say.
    pub fn speak(&self, text: &str, lang: Lang) -> Result<u64, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("nothing to say".into());
        }
        let (reply, got) = mpsc::channel();
        self.tx
            .send(Cmd::Speak {
                text: text.to_string(),
                lang,
                reply,
            })
            .map_err(|_| "the speech thread is gone".to_string())?;
        got.recv()
            .map_err(|_| "the speech thread is gone".to_string())?
    }

    /// Silence at once.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[derive(Default)]
    struct Log {
        /// What the engine was asked, in order: `speak:<text>` and `stop`.
        calls: Vec<String>,
        speaking: bool,
        fail: bool,
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Log>>);

    impl Engine for Fake {
        fn speak(&mut self, text: &str, _: Lang) -> Result<(), String> {
            let mut l = self.0.lock();
            if l.fail {
                return Err("no voice".into());
            }
            l.calls.push(format!("speak:{text}"));
            l.speaking = true;
            Ok(())
        }
        fn stop(&mut self) {
            let mut l = self.0.lock();
            l.calls.push("stop".into());
            l.speaking = false;
        }
        fn speaking(&mut self) -> bool {
            self.0.lock().speaking
        }
    }

    fn rig(grace: Duration) -> (Speaker, Fake, mpsc::Receiver<u64>) {
        let fake = Fake::default();
        let (tx, ended) = mpsc::channel();
        let f = fake.clone();
        let sp = Speaker::spawn(
            move || f,
            move |id| {
                let _ = tx.send(id);
            },
            grace,
        );
        (sp, fake, ended)
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn an_utterance_ends_once_with_its_own_id() {
        let (sp, fake, ended) = rig(Duration::ZERO);
        let id = sp.speak("hello", Lang::En).unwrap();
        fake.0.lock().speaking = false;
        assert_eq!(ended.recv_timeout(WAIT), Ok(id));
        assert!(ended.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn stop_is_final_nothing_is_announced_after_it() {
        let (sp, fake, ended) = rig(Duration::ZERO);
        sp.speak("hello", Lang::En).unwrap();
        sp.stop();
        // The queue is ordered: once this returns, `stop` was handled.
        sp.speak("again", Lang::En).unwrap();
        fake.0.lock().speaking = false;
        let calls = fake.0.lock().calls.clone();
        assert_eq!(
            calls,
            ["stop", "speak:hello", "stop", "stop", "speak:again"]
        );
        // Only the second utterance ends.
        assert_eq!(ended.recv_timeout(WAIT), Ok(2));
        assert!(ended.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn a_stopped_utterance_never_reports_an_end() {
        let (sp, fake, ended) = rig(Duration::ZERO);
        sp.speak("hello", Lang::En).unwrap();
        sp.stop();
        std::thread::sleep(Duration::from_millis(400));
        assert!(!fake.0.lock().speaking);
        assert!(ended.try_recv().is_err());
    }

    #[test]
    fn a_new_utterance_cuts_the_old_one_first() {
        let (sp, fake, _ended) = rig(Duration::from_secs(60));
        sp.speak("one", Lang::En).unwrap();
        sp.speak("two", Lang::PtBr).unwrap();
        let calls = fake.0.lock().calls.clone();
        let pos = |s: &str| calls.iter().position(|c| c == s).unwrap();
        assert!(pos("speak:one") < pos("speak:two"));
        assert_eq!(
            calls[pos("speak:two") - 1],
            "stop",
            "the old one is cut before the new one starts"
        );
    }

    #[test]
    fn a_blank_text_never_reaches_the_engine() {
        let (sp, fake, _ended) = rig(Duration::ZERO);
        assert!(sp.speak("  \n\t ", Lang::En).is_err());
        assert!(sp.speak("", Lang::En).is_err());
        sp.stop();
        sp.speak("x", Lang::En).unwrap();
        assert_eq!(fake.0.lock().calls, ["stop", "stop", "speak:x"]);
    }

    #[test]
    fn an_engine_failure_is_the_error_and_nothing_ends() {
        let (sp, fake, ended) = rig(Duration::ZERO);
        fake.0.lock().fail = true;
        assert_eq!(sp.speak("hello", Lang::En), Err("no voice".to_string()));
        std::thread::sleep(Duration::from_millis(300));
        assert!(ended.try_recv().is_err());
    }

    #[test]
    fn a_synthesizer_that_is_not_yet_speaking_is_not_over() {
        // The grace: the engine says "not speaking" right after the text was handed over; the end must wait.
        let (sp, fake, ended) = rig(Duration::from_millis(700));
        let id = sp.speak("hello", Lang::En).unwrap();
        fake.0.lock().speaking = false;
        assert!(ended.recv_timeout(Duration::from_millis(300)).is_err());
        assert_eq!(ended.recv_timeout(WAIT), Ok(id));
    }

    #[test]
    fn the_language_is_the_apps_display_language() {
        for (tag, want) in [
            ("en", Lang::En),
            ("EN-us", Lang::En),
            ("pt-BR", Lang::PtBr),
            ("pt_br", Lang::PtBr),
            ("pt", Lang::PtBr),
            ("", Lang::En),
            ("fr", Lang::En),
        ] {
            assert_eq!(Lang::parse(tag), want, "{tag}");
        }
    }
}
