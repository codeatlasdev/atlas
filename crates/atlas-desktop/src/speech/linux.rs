//! Linux (and other Unixes): speech-dispatcher through `spd-say`, which is optional. Without it the toggle says the
//! system cannot read aloud. `-w` makes the process live as long as the speech does, so the child is the utterance:
//! its exit is the end, and killing it (plus `spd-say -C`) is the stop. The text goes in on stdin (`-e`), never
//! in the argument list another user could read with `ps`. The voice and the rate are speech-dispatcher's own.

// Macs and Windows compile this module for its tests only (the fake `spd-say`), so the production entry points are unused.
#![cfg_attr(
    any(target_os = "macos", windows),
    allow(dead_code, reason = "only the unix tests compile this module there")
)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use atlas_core::sys::CommandExt as _;

use super::speaker::{Engine, Lang};

const BIN: &str = "spd-say";

pub fn available() -> bool {
    find().is_some()
}

/// The `spd-say` arguments for speaking in `lang`: pipe mode (`-e`, text on stdin), wait (`-w`), the language.
pub fn args(lang: Lang) -> [&'static str; 4] {
    [
        "-e",
        "-w",
        "-l",
        match lang {
            Lang::En => "en",
            Lang::PtBr => "pt",
        },
    ]
}

/// `spd-say` where it is installed.
fn find() -> Option<PathBuf> {
    atlas_core::paths::which(BIN)
}

pub struct Spd {
    /// Where `spd-say` is, looked up at each utterance when not fixed (tests fix it).
    bin: Option<PathBuf>,
    child: Option<Child>,
}

impl Spd {
    pub fn new() -> Self {
        Self {
            bin: None,
            child: None,
        }
    }

    fn bin(&self) -> Result<PathBuf, String> {
        self.bin
            .clone()
            .or_else(find)
            .ok_or_else(|| format!("{BIN} is not installed"))
    }
}

impl Engine for Spd {
    fn speak(&mut self, text: &str, lang: Lang) -> Result<(), String> {
        let mut child = Command::new(self.bin()?)
            .args(args(lang))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .no_window()
            .spawn()
            .map_err(|e| format!("could not start {BIN}: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("could not hand the text to {BIN}: {e}"))?;
        // Closing the pipe is what tells pipe mode the text is complete.
        drop(stdin);
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
        // Messages already queued in the daemon go too.
        if let Ok(bin) = self.bin() {
            let _ = Command::new(bin)
                .arg("-C")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .no_window()
                .status();
        }
    }

    fn speaking(&mut self) -> bool {
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.child = None;
                false
            }
            None => false,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::time::{Duration, Instant};

    /// A stand-in `spd-say`: records what arrives on stdin and the arguments, then talks for `secs`; `-C` is logged.
    fn fake(dir: &std::path::Path, secs: u32) -> PathBuf {
        let bin = dir.join("spd-say");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = -C ]; then echo cancel >> '{d}/log'; exit 0; fi\nprintf '%s\\n' \"$*\" > '{d}/args'\ncat > '{d}/text'\nsleep {secs}\n",
            d = dir.display()
        );
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn until(what: &str, mut ok: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn spd(bin: PathBuf) -> Spd {
        Spd {
            bin: Some(bin),
            child: None,
        }
    }

    #[test]
    fn the_text_goes_in_on_stdin_never_in_the_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = spd(fake(dir.path(), 30));
        s.speak("secret --flag text", Lang::PtBr).unwrap();
        until("the text", || {
            std::fs::read_to_string(dir.path().join("text"))
                .is_ok_and(|t| t == "secret --flag text")
        });
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert_eq!(args.trim(), "-e -w -l pt");
        assert!(s.speaking());
        s.stop();
    }

    #[test]
    fn stop_kills_the_child_and_cancels_the_daemons_queue() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = spd(fake(dir.path(), 30));
        s.speak("hello", Lang::En).unwrap();
        assert!(s.speaking());
        s.stop();
        assert!(!s.speaking());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("log"))
                .unwrap()
                .trim(),
            "cancel"
        );
        // Nothing to stop: no second cancel.
        s.stop();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    #[test]
    fn the_child_exiting_is_the_end_of_the_speech() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = spd(fake(dir.path(), 0));
        s.speak("hello", Lang::En).unwrap();
        until("the end", || !s.speaking());
    }

    #[test]
    fn a_missing_spd_say_is_an_error_not_a_silent_success() {
        let mut s = spd(PathBuf::from("/nonexistent/spd-say"));
        assert!(s.speak("hello", Lang::En).is_err());
        assert!(!s.speaking());
    }
}
