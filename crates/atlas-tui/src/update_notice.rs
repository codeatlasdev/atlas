//! Newer-release notice shared between the CLI's background update check and
//! the TUI. The CLI publishes; the dashboard reads it on render. The TUI
//! reports back a user's "update when I quit" decision through `request_on_exit`.

use std::sync::Mutex;

static AVAILABLE: Mutex<Option<String>> = Mutex::new(None);
static ON_EXIT: Mutex<Option<String>> = Mutex::new(None);

/// Record that `version` (no leading `v`) is newer than the running binary.
pub fn publish(version: String) {
    if let Ok(mut slot) = AVAILABLE.lock() {
        *slot = Some(version);
    }
}

/// The newer version, if one was published.
pub fn available() -> Option<String> {
    AVAILABLE.lock().ok().and_then(|s| s.clone())
}

/// The user accepted "update when you quit" for `version`.
pub fn request_on_exit(version: String) {
    if let Ok(mut slot) = ON_EXIT.lock() {
        *slot = Some(version);
    }
}

/// Version the user asked to install after the TUI exits, if any.
pub fn on_exit_requested() -> Option<String> {
    ON_EXIT.lock().ok().and_then(|s| s.clone())
}

/// Whether the TUI is attached to a runner (which keeps running the old binary
/// after an on-exit update). Set by the TUI so the CLI can word its post-update note.
static ATTACHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_attached(v: bool) {
    ATTACHED.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn attached() -> bool {
    ATTACHED.load(std::sync::atomic::Ordering::Relaxed)
}
