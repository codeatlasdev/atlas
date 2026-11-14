//! Test-only overrides of what a task would run or ask: the agent binary, the summarizer, the
//! GitHub login state and a pause after the spawn. Process-wide, so a test that sets one holds
//! [`TEST_BIN_LOCK`] for its whole run.

use std::time::Duration;

/// Serialise tests that need to override the spawned agent binary.
/// Hold via `.lock()` for the entire test to prevent races between parallel tests.
#[cfg(unix)]
pub(crate) static TEST_BIN_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

/// Absolute path to a fake agent binary, overrides the agent def's bin.
pub(super) static TEST_AGENT_BIN: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Set (or clear) the per-test agent binary override.  Always acquire
/// `TEST_BIN_LOCK` first to prevent concurrent tests from racing.
#[cfg(unix)]
pub(crate) fn set_test_agent_bin(bin: Option<&str>) {
    *TEST_AGENT_BIN.lock() = bin.map(str::to_string);
}

/// A pause right after the agent is spawned, so a test can make the agent finish
/// before the runner gets to watch it.
pub(crate) static TEST_PAUSE_AFTER_SPAWN: parking_lot::Mutex<Option<Duration>> =
    parking_lot::Mutex::new(None);

/// If set, forces the GitHub auth pre-check to return Some(reason).
pub(crate) static TEST_FORCE_GITHUB_AUTH: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Override the result of is_github_logged_in().
pub(crate) static TEST_GITHUB_LOGGED_IN: parking_lot::Mutex<Option<bool>> =
    parking_lot::Mutex::new(None);

/// Absolute path to a fake summarizer binary.
pub(super) static TEST_SUMMARIZER_BIN: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Set (or clear) the per-test summarizer binary override. Its tests run a shell
/// script as the fake summarizer, so they (and this) are unix only.
#[cfg(unix)]
pub(crate) fn set_test_summarizer_bin(bin: Option<&str>) {
    *TEST_SUMMARIZER_BIN.lock() = bin.map(str::to_string);
}
