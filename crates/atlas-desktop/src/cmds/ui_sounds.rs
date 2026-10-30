//! macOS "Play user interface sound effects" (System Settings → Sound). The app's earcons must follow it.

use std::process::Command;

/// What `defaults read -g com.apple.sound.uiaudio.enabled` printed for the preference.
///
/// The key is absent until the user flips the switch once (the default is ON), so only an explicit `0` turns the
/// sounds off. Anything unreadable also means ON: a broken `defaults` must not silence the app for good, and the
/// per-app volume and "reduce sounds" switches stay in the user's hands.
fn enabled_from(exit_ok: bool, stdout: &str) -> bool {
    if !exit_ok {
        return true;
    }
    !matches!(stdout.trim(), "0" | "false" | "NO" | "no")
}

/// Whether the system lets apps play UI sound effects. Read once at start by the frontend. `defaults` is macOS
/// only: elsewhere there is no such switch to follow, so the answer is "on" without spawning anything.
#[tauri::command]
pub fn system_ui_sounds() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    match Command::new("defaults")
        .args(["read", "-g", "com.apple.sound.uiaudio.enabled"])
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(o) => enabled_from(o.status.success(), &String::from_utf8_lossy(&o.stdout)),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_means_the_default_which_is_on() {
        // Verified on macOS 26.6.2: `defaults read` exits 1 ("does not exist") until the switch was flipped once.
        assert!(enabled_from(false, ""));
    }

    #[test]
    fn explicit_zero_turns_sounds_off() {
        assert!(!enabled_from(true, "0\n"));
    }

    #[test]
    fn explicit_one_keeps_sounds_on() {
        assert!(enabled_from(true, "1\n"));
    }

    #[test]
    fn unexpected_output_does_not_silence_the_app() {
        assert!(enabled_from(true, "garbage"));
        assert!(enabled_from(true, ""));
    }

    #[test]
    fn boolean_spellings_off() {
        for off in ["false", "NO", "no", " 0 "] {
            assert!(!enabled_from(true, off), "{off:?}");
        }
    }
}
