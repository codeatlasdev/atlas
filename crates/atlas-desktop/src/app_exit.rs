//! The one way the app ends: `AppHandle::exit` is disallowed everywhere else (`clippy.toml`).
//!
//! `AppHandle::exit` ends in `process::exit`, which never drops a static, so the virtual display of a macOS agent
//! instance (`agent_display::DISPLAY`) would be left to the OS. macOS defers removing a display while the Mac is
//! asleep, and one left then outlives the process (incident 2026-11-19: five orphan displays). Every exit therefore
//! comes through here and destroys the display first; `RunEvent::Exit` (`main.rs`) is the second line.

use tauri::AppHandle;

/// Destroy the agent display (macOS; nothing to do when there is none), then exit with `code`.
#[allow(
    clippy::disallowed_methods,
    reason = "the one place that may end the app: it destroys the agent display first"
)]
pub fn exit(app: &AppHandle, code: i32) {
    #[cfg(target_os = "macos")]
    crate::agent_display::destroy();
    app.exit(code);
}
