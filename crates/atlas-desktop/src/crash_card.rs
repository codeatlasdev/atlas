//! "Atlas closed unexpectedly last time" (#322): at startup, the newest thing that went wrong in an earlier session and
//! that the user has not acknowledged, for the card the page shows. Nothing is uploaded: the card offers the existing
//! redacted report screen and a dismiss.
//!
//! Two sources, both written by [`crate::diag`] and `atlas_core::crash` long before this session started:
//! - a crash report (`crash-<unix_ms>-<pid>.toml`, #330) in `atlas_core::crash::crash_dir()`: a panic of the app,
//!   `atlas hostd` or the CLI, already redacted by the writer;
//! - a `hostd-exited` line of the app log (#438): the terminal host died with no shutdown, upgrade or restart from the
//!   app, which leaves no crash file when it was killed (OOM, `kill -9`). A line that names a crash report is that
//!   report's duplicate and is left to it.
//!
//! The acknowledged stamp is `<base>/desktop/crash-ack` (unix ms of the newest event the user dismissed): anything
//! newer than it shows, anything at or before it does not. With no stamp yet (first launch of a build that has this
//! card) only the last [`FIRST_RUN_WINDOW_MS`] count, so an old crash file does not greet a new user. Events at or after
//! the start of this session are never shown: they belong to the session that is running.

use std::path::{Path, PathBuf};

use atlas_core::probe::crash_entries;
use serde::Serialize;
use tauri::State;

/// Without a stamp, how far back an event still counts (7 days).
const FIRST_RUN_WINDOW_MS: u64 = 7 * 24 * 3600 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A crash report file.
    Crash,
    /// The terminal host ended on its own (`hostd-exited` incident).
    HostdExited,
}

/// What the page shows. camelCase: `apps/desktop/src/lib/stores/crashCard.svelte.ts` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Unix ms of the event; what a dismiss acknowledges.
    pub at_ms: u64,
    pub kind: Kind,
    /// Redacted: `atlas <version>: <message> (<location>)` for a crash, the incident text for a host exit.
    pub summary: String,
    /// What names this crash across reports (`atlas_core::crash_fingerprint`); a host exit has none.
    pub fingerprint: Option<String>,
}

fn stamp_path(base: &Path) -> PathBuf {
    base.join("desktop").join("crash-ack")
}

fn read_stamp(base: &Path) -> Option<u64> {
    std::fs::read_to_string(stamp_path(base))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Acknowledge everything up to `at_ms`. A stamp only moves forward, so a late or repeated dismiss of an older card
/// cannot bring back a newer one that was already dismissed.
pub fn acknowledge(base: &Path, at_ms: u64) -> std::io::Result<()> {
    let at_ms = at_ms.max(read_stamp(base).unwrap_or(0));
    let p = stamp_path(base);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, at_ms.to_string())
}

/// The `hostd-exited` lines of an app log (`<unix secs> hostd-exited: <detail>`) as (unix ms, detail), skipping the ones
/// that point at a crash report: that report is the better record.
fn hostd_exits(app_log: &str) -> impl Iterator<Item = (u64, String)> + '_ {
    app_log.lines().filter_map(|l| {
        let (secs, rest) = l.split_once(' ')?;
        let detail = rest.strip_prefix("hostd-exited: ")?;
        if detail.contains("Crash report: crash-") {
            return None;
        }
        Some((
            secs.parse::<u64>().ok()?.checked_mul(1000)?,
            detail.to_string(),
        ))
    })
}

/// The newest unacknowledged event from an earlier session, if any. `started_ms` is when this session began.
pub fn pending(base: &Path, crash_dir: &Path, started_ms: u64) -> Option<Card> {
    let floor = read_stamp(base).unwrap_or(started_ms.saturating_sub(FIRST_RUN_WINDOW_MS));
    let earlier = |at_ms: &u64| *at_ms > floor && *at_ms < started_ms;

    let crash = crash_entries(crash_dir)
        .find(|c| c.at_ms < started_ms)
        .filter(|c| earlier(&c.at_ms))
        .map(|c| Card {
            at_ms: c.at_ms,
            kind: Kind::Crash,
            summary: c.summary,
            fingerprint: c.fingerprint,
        });
    let log = std::fs::read_to_string(atlas_host::paths::app_log(base)).unwrap_or_default();
    let exit = hostd_exits(&log)
        .filter(|(at, _)| earlier(at))
        .max_by_key(|(at, _)| *at)
        .map(|(at_ms, detail)| Card {
            at_ms,
            kind: Kind::HostdExited,
            summary: atlas_core::redact::redact(&detail),
            fingerprint: None,
        });
    match (crash, exit) {
        (Some(c), Some(e)) => Some(if e.at_ms > c.at_ms { e } else { c }),
        (c, e) => c.or(e),
    }
}

/// When this session began, to tell its own events from an earlier one's.
pub struct CrashCard {
    started_ms: u64,
}

impl CrashCard {
    pub fn new() -> Self {
        Self {
            started_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
        }
    }
}

/// The card to show at startup, or `None`.
#[tauri::command]
pub async fn crash_card_pending(state: State<'_, CrashCard>) -> Result<Option<Card>, String> {
    let started = state.started_ms;
    Ok(pending(
        &atlas_host::paths::base(),
        &atlas_core::crash::crash_dir(),
        started,
    ))
}

/// Dismiss: the stamp moves to the card's time, so it never comes back (and neither does anything older).
#[tauri::command]
pub async fn crash_card_dismiss(at_ms: u64) -> Result<(), String> {
    acknowledge(&atlas_host::paths::base(), at_ms).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    struct Home {
        base: tempfile::TempDir,
        crashes: tempfile::TempDir,
    }

    fn home() -> Home {
        Home {
            base: tempfile::tempdir().unwrap(),
            crashes: tempfile::tempdir().unwrap(),
        }
    }

    impl Home {
        fn crash_at(&self, ms: u64, msg: &str) {
            std::fs::write(
                self.crashes.path().join(format!("crash-{ms}-77.toml")),
                format!("version = \"1.5.0\"\nmsg = {msg:?}\nlocation = \"a.rs:1\"\n"),
            )
            .unwrap();
        }
        fn log(&self, secs: u64, line: &str) {
            let p = atlas_host::paths::app_log(self.base.path());
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut s = std::fs::read_to_string(&p).unwrap_or_default();
            s.push_str(&format!("{secs} {line}\n"));
            std::fs::write(p, s).unwrap();
        }
        fn pending(&self) -> Option<Card> {
            pending(self.base.path(), self.crashes.path(), NOW)
        }
    }

    /// #322: a crash newer than the acknowledged stamp is the card; one at or before it is none.
    #[test]
    fn a_newer_crash_shows_and_an_older_or_acknowledged_one_does_not() {
        let h = home();
        h.crash_at(NOW - 5_000, "boom");
        let card = h.pending().expect("an unacknowledged crash shows");
        assert_eq!(card.kind, Kind::Crash);
        assert_eq!(card.at_ms, NOW - 5_000);
        assert!(card.summary.contains("boom"), "{}", card.summary);

        acknowledge(h.base.path(), NOW - 5_000).unwrap();
        assert_eq!(h.pending(), None, "the stamp is the crash's own time");

        h.crash_at(NOW - 1_000, "newer");
        assert_eq!(
            h.pending().unwrap().at_ms,
            NOW - 1_000,
            "a crash after the stamp shows again"
        );
    }

    /// #320: the card carries the fingerprint of the crash report (its own line for a panic, the one computed from the
    /// frames for a native crash), so "Report issue" can look for the issue; a host exit has none to give.
    #[test]
    fn a_crash_card_carries_the_fingerprint_of_its_report() {
        let h = home();
        std::fs::write(
            h.crashes.path().join(format!("crash-{}-77.toml", NOW - 5_000)),
            "version = \"1.5.0\"\nmsg = \"boom\"\nlocation = \"a.rs:1\"\nfingerprint = \"0123456789abcdef\"\n",
        )
        .unwrap();
        assert_eq!(
            h.pending().unwrap().fingerprint.as_deref(),
            Some("0123456789abcdef")
        );

        let h = home();
        h.log(NOW / 1000 - 30, "hostd-exited: terminal host pid 41 ended");
        assert_eq!(h.pending().unwrap().fingerprint, None);
    }

    /// Dismiss persists across launches (a new `pending` call reads the file) and never moves back.
    #[test]
    fn a_dismiss_persists_and_a_stale_dismiss_cannot_bring_a_card_back() {
        let h = home();
        h.crash_at(NOW - 2_000, "second");
        acknowledge(h.base.path(), NOW - 2_000).unwrap();
        acknowledge(h.base.path(), NOW - 9_000).unwrap();
        assert_eq!(h.pending(), None);
        assert_eq!(read_stamp(h.base.path()), Some(NOW - 2_000));
    }

    /// With no stamp, a crash from months ago must not greet the user; one from yesterday does.
    #[test]
    fn without_a_stamp_only_the_last_week_counts() {
        let h = home();
        h.crash_at(NOW - FIRST_RUN_WINDOW_MS - 1, "months ago");
        assert_eq!(h.pending(), None);
        h.crash_at(NOW - 24 * 3600 * 1000, "yesterday");
        assert!(h.pending().unwrap().summary.contains("yesterday"));
    }

    /// An event of this very session is not "last time": a panic after start-up shows at the next one.
    #[test]
    fn this_sessions_own_events_are_not_the_card() {
        let h = home();
        h.crash_at(NOW + 1, "this session");
        h.log(NOW / 1000 + 1, "hostd-exited: terminal host pid 1 ended");
        assert_eq!(h.pending(), None);
    }

    /// #438: the terminal host dying unasked (killed, so no crash file) is a card; one that left a crash report is
    /// that report's card, not a second one; a live upgrade or restart is not logged as hostd-exited at all.
    #[test]
    fn a_host_exit_without_a_report_shows_and_one_with_a_report_defers_to_it() {
        let h = home();
        h.log(
            NOW / 1000 - 30,
            "hostd-exited: terminal host pid 41 ended without a shutdown, upgrade or restart from the app; the app is now connected to pid 42. Its terminals ended.",
        );
        let card = h.pending().expect("a killed host shows");
        assert_eq!(card.kind, Kind::HostdExited);
        assert_eq!(card.at_ms, (NOW / 1000 - 30) * 1000);

        let h = home();
        h.log(
            NOW / 1000 - 30,
            "hostd-exited: terminal host pid 41 ended without a shutdown. Crash report: crash-1799999970000-41.toml.",
        );
        h.crash_at(NOW - 30_000, "hostd panicked");
        let card = h.pending().unwrap();
        assert_eq!(card.kind, Kind::Crash, "{card:?}");
    }

    /// The newest of the two sources wins; other app-log lines never count.
    #[test]
    fn the_newest_event_wins_and_other_log_lines_are_ignored() {
        let h = home();
        h.crash_at(NOW - 60_000, "older crash");
        h.log(NOW / 1000 - 10, "tray: popover could not be built: x");
        h.log(
            NOW / 1000 - 20,
            "hostd-exited: terminal host pid 1 ended. Its terminals ended.",
        );
        assert_eq!(h.pending().unwrap().kind, Kind::HostdExited);
        h.crash_at(NOW - 1_000, "newest crash");
        assert_eq!(h.pending().unwrap().kind, Kind::Crash);
    }

    /// The real writer's file is what the card reads, redacted (round trip through `atlas_core::crash`).
    #[test]
    fn the_card_reads_what_the_real_crash_writer_wrote_and_redacted() {
        let h = home();
        let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        atlas_core::crash::write_report_in(
            h.crashes.path(),
            &format!("boom TOKEN={token}"),
            "src/main.rs:1:1",
            &["atlas".to_string()],
            "frame",
        )
        .unwrap();
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 60_000;
        let card = pending(h.base.path(), h.crashes.path(), started).unwrap();
        assert!(card.summary.contains("boom"), "{}", card.summary);
        assert!(!card.summary.contains(token), "{}", card.summary);
    }

    /// The page reads these keys (`crashCard.svelte.ts`); the real value is serialized, never hand-written.
    #[test]
    fn the_card_serializes_the_keys_the_page_reads() {
        let v = serde_json::to_value(Card {
            at_ms: 5,
            kind: Kind::HostdExited,
            summary: "s".into(),
            fingerprint: Some("0123456789abcdef".into()),
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"atMs": 5, "kind": "hostd-exited", "summary": "s", "fingerprint": "0123456789abcdef"})
        );
        assert_eq!(
            serde_json::to_value(Kind::Crash).unwrap(),
            serde_json::json!("crash")
        );
    }
}
