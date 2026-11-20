//! Whether the system lets this app use the microphone, and the way to the place where the person changes that.
//!
//! macOS asks once (the system prompt) and remembers a refusal for good: from then on only System Settings can
//! change it, so a refusal is a state the page explains with a button to Privacy & Security > Microphone. Windows
//! has one per-user switch for desktop apps (Settings > Privacy > Microphone) read from the registry. Linux has no
//! per-app permission: the sound server decides, and an unusable device shows up as an open failure instead.

use serde::Serialize;

/// The microphone permission of this app.
#[allow(
    dead_code,
    reason = "each platform builds only some of these: Linux only `Unknown`, macOS has `NotAsked`"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Granted,
    /// macOS only: never asked, the first recording shows the system prompt.
    NotAsked,
    /// The system refuses it; only a settings change fixes it.
    Denied,
    /// This platform has no permission to read (Linux).
    Unknown,
}

/// The state now, without asking.
pub fn current() -> Access {
    #[cfg(target_os = "macos")]
    return match crate::shots::microphone_access() {
        crate::shots::recopts::MicAccess::NotAsked => Access::NotAsked,
        crate::shots::recopts::MicAccess::Granted => Access::Granted,
        crate::shots::recopts::MicAccess::Refused => Access::Denied,
    };
    #[cfg(windows)]
    return windows_access();
    #[cfg(not(any(target_os = "macos", windows)))]
    Access::Unknown
}

/// Ask where the platform asks (macOS: the system prompt, once; waits for the answer). Elsewhere the same as
/// [`current`]. Never called at launch: only when a dictation starts.
pub fn request() -> Access {
    #[cfg(target_os = "macos")]
    {
        if crate::shots::microphone_request() {
            Access::Granted
        } else {
            Access::Denied
        }
    }
    #[cfg(not(target_os = "macos"))]
    current()
}

/// Open the system's microphone privacy page (macOS and Windows; Linux has none).
pub fn open_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::shots::open_microphone_settings();
    #[cfg(windows)]
    return atlas_core::sys::shell_open("ms-settings:privacy-microphone")
        .map_err(|e| format!("could not open the microphone settings: {e}"));
    #[cfg(not(any(target_os = "macos", windows)))]
    Err("this system has no microphone privacy page".into())
}

/// The registry keys of the microphone consent store that can say `Deny`: the machine-wide switch, the user's
/// switch for all apps and the user's switch for desktop (non-Store) apps, which is the one that covers Atlas.
#[cfg(windows)]
const CONSENT_KEYS: [&str; 3] = [
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
    r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
    r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged",
];

/// `Allow` or `Deny` out of `reg query <key> /v Value`:
///
/// ```text
/// HKEY_CURRENT_USER\SOFTWARE\...\microphone
///     Value    REG_SZ    Allow
/// ```
#[cfg(any(windows, test))]
fn consent_value(out: &str) -> Option<&str> {
    out.lines().find_map(|l| {
        let mut t = l.split_whitespace();
        (t.next() == Some("Value") && t.next() == Some("REG_SZ"))
            .then(|| t.next())
            .flatten()
    })
}

/// Any of the switches says `Deny`.
#[cfg(any(windows, test))]
fn any_denied<'a>(outputs: impl IntoIterator<Item = &'a str>) -> bool {
    outputs
        .into_iter()
        .any(|o| consent_value(o).is_some_and(|v| v.eq_ignore_ascii_case("deny")))
}

#[cfg(windows)]
fn windows_access() -> Access {
    use atlas_core::sys::CommandExt;
    let outs: Vec<String> = CONSENT_KEYS
        .iter()
        .map(|k| {
            std::process::Command::new("reg")
                .args(["query", k, "/v", "Value"])
                .no_window()
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default()
        })
        .collect();
    if any_denied(outs.iter().map(String::as_str)) {
        Access::Denied
    } else {
        Access::Granted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(value: &str) -> String {
        format!(
            "\r\nHKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\r\n    Value    REG_SZ    {value}\r\n\r\n"
        )
    }

    #[test]
    fn reads_allow_and_deny_out_of_reg_query() {
        assert_eq!(consent_value(&reg("Allow")), Some("Allow"));
        assert_eq!(consent_value(&reg("Deny")), Some("Deny"));
        // A key that does not exist prints an error on stderr and nothing on stdout.
        assert_eq!(consent_value(""), None);
        // A subkey line is not the value.
        assert_eq!(
            consent_value("HKEY_CURRENT_USER\\...\\microphone\\NonPackaged\r\n"),
            None
        );
    }

    #[test]
    fn one_switch_saying_deny_is_enough_and_a_missing_key_is_not_a_denial() {
        let allow = reg("Allow");
        let deny = reg("Deny");
        assert!(!any_denied([
            allow.as_str(),
            allow.as_str(),
            allow.as_str()
        ]));
        assert!(any_denied([allow.as_str(), allow.as_str(), deny.as_str()]));
        assert!(any_denied([deny.as_str(), "", ""]));
        assert!(!any_denied(["", "", ""]), "no keys: nothing denies it");
    }
}
