//! Agent attention: native notification, optional sound, Dock badge.

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

#[tauri::command]
pub fn agent_notify(
    app: AppHandle,
    title: String,
    body: String,
    sound: bool,
) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())?;
    if sound {
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
