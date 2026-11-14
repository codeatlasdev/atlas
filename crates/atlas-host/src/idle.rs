//! When the daemon exits on its own: nothing running, nobody connected, for a while.
//!
//! A daemon outlives the app by design (terminals keep running), but one with no terminal
//! and no client is just a stale process that never picks up a new binary. The decisions are
//! pure so they can be tested without a clock or a socket.

use std::path::Path;
use std::time::Duration;

use crate::paths;

/// Default quiet period before an idle daemon exits.
pub const IDLE_EXIT: Duration = Duration::from_secs(10 * 60);

/// Env override of the quiet period, in seconds (tests, or a host that wants it shorter).
pub const IDLE_ENV: &str = "ATLAS_HOSTD_IDLE_SECS";

/// How often the daemon looks (bounded by the limit so a short one is honoured).
pub fn tick(limit: Duration) -> Duration {
    Duration::from_secs(15)
        .min(limit / 2)
        .max(Duration::from_millis(100))
}

/// Parse the quiet period from an env-var value: `"never"` → `None`,
/// a decimal integer → `Some(that many seconds)`, anything else → `Some(IDLE_EXIT)`.
/// Empty string (var unset) also yields `Some(IDLE_EXIT)`.
pub(crate) fn parse_limit(v: &str) -> Option<Duration> {
    match v.trim() {
        "" => Some(IDLE_EXIT),
        "never" => None,
        s => Some(
            s.parse::<u64>()
                .map(Duration::from_secs)
                .unwrap_or(IDLE_EXIT),
        ),
    }
}

/// The quiet period from `ATLAS_HOSTD_IDLE_SECS`. `None` → idle exit disabled.
pub fn limit_from_env() -> Option<Duration> {
    parse_limit(&std::env::var(IDLE_ENV).unwrap_or_default())
}

/// True once the daemon has been idle for `limit`: never while a terminal runs or a client is
/// connected (`idle_for` is how long both have been zero).
pub fn idle_expired(live_ptys: usize, clients: usize, idle_for: Duration, limit: Duration) -> bool {
    live_ptys == 0 && clients == 0 && idle_for >= limit
}

/// True when the directory that holds the pid file is gone (a dev or test dir that was
/// removed): nobody can reach this daemon any more.
pub fn dir_gone(dir: &Path) -> bool {
    !dir.is_dir() || !paths::pid(dir).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: Duration = Duration::from_secs(600);

    #[test]
    fn expires_only_when_nothing_runs_nobody_is_connected_and_the_limit_passed() {
        assert!(idle_expired(0, 0, LIMIT, LIMIT));
        assert!(idle_expired(0, 0, LIMIT * 2, LIMIT));
        assert!(!idle_expired(0, 0, LIMIT - Duration::from_secs(1), LIMIT));
    }

    #[test]
    fn a_running_pty_never_expires_however_long_it_has_been() {
        assert!(!idle_expired(1, 0, LIMIT * 100, LIMIT));
        assert!(!idle_expired(3, 0, Duration::MAX, LIMIT));
    }

    #[test]
    fn a_connected_client_never_expires() {
        assert!(!idle_expired(0, 1, LIMIT * 100, LIMIT));
        assert!(!idle_expired(2, 2, LIMIT * 100, LIMIT));
    }

    #[test]
    fn tick_is_short_for_short_limits_and_capped_for_long_ones() {
        assert_eq!(tick(Duration::from_secs(600)), Duration::from_secs(15));
        assert_eq!(tick(Duration::from_secs(1)), Duration::from_millis(500));
        assert_eq!(tick(Duration::ZERO), Duration::from_millis(100));
    }

    #[test]
    fn never_disables_idle_exit() {
        assert!(parse_limit("never").is_none());
    }

    #[test]
    fn numeric_env_overrides_default() {
        assert_eq!(parse_limit("300"), Some(Duration::from_secs(300)));
        assert_eq!(parse_limit("0"), Some(Duration::from_secs(0)));
    }

    #[test]
    fn unset_or_invalid_env_uses_default_limit() {
        assert_eq!(parse_limit(""), Some(IDLE_EXIT));
        assert_eq!(parse_limit("banana"), Some(IDLE_EXIT));
    }
    #[test]
    fn dir_gone_when_the_dir_or_the_pid_file_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(dir_gone(tmp.path()), "no pid file yet");
        std::fs::write(paths::pid(tmp.path()), "1").unwrap();
        assert!(!dir_gone(tmp.path()));
        let p = tmp.path().to_path_buf();
        drop(tmp);
        assert!(dir_gone(&p));
    }
}
