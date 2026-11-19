//! Agent attention: native notification, optional sound, Dock badge.

use crate::native_ui::{self, Verdict};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn agent_notify(
    app: AppHandle,
    title: String,
    body: String,
    sound: bool,
) -> Result<(), String> {
    // An agent instance shows no banner and plays no sound (`native_ui`).
    if native_ui::notify(&app, title, body)? == Verdict::Suppressed {
        return Ok(());
    }
    // `afplay` and the system sound exist on macOS only; elsewhere the toast carries the platform's own sound.
    if sound && cfg!(target_os = "macos") {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
    Ok(())
}

/// Dock badge = number of unread worktrees (0 clears it).
#[tauri::command]
pub fn set_badge(app: AppHandle, count: u32) {
    if let Some(w) = app.get_window("main") {
        let _ = w.set_badge_count((count > 0).then_some(count as i64));
    }
}
