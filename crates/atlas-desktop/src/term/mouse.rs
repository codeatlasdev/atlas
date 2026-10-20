//! Mouse reporting: a program that turned on mouse modes (vim, tmux, htop, agent TUIs) gets the
//! pointer as escape sequences (`atlas_term::mouse`) instead of the terminal selecting text.

use atlas_term::mouse::{self, Action, Event};

use super::with_term;

/// `action` as the webview sends it (`pointer.ts`): lowercase words.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WireAction {
    Press,
    Release,
    Motion,
    WheelUp,
    WheelDown,
}

impl From<WireAction> for Action {
    fn from(a: WireAction) -> Self {
        match a {
            WireAction::Press => Action::Press,
            WireAction::Release => Action::Release,
            WireAction::Motion => Action::Motion,
            WireAction::WheelUp => Action::WheelUp,
            WireAction::WheelDown => Action::WheelDown,
        }
    }
}

/// Does the program own the mouse? `shift` held gives clicks back to the terminal (selection).
#[tauri::command]
pub fn term_mouse_captured(tab: String, shift: bool) -> Result<bool, String> {
    Ok(mouse::captures(with_term(&tab)?.vt.mode(), shift))
}

/// Report a pointer event to the program. `false` = the active mode does not report it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn term_mouse(
    tab: String,
    action: WireAction,
    button: Option<u8>,
    col: u16,
    row: u16,
    shift: bool,
    alt: bool,
    ctrl: bool,
) -> Result<bool, String> {
    let sh = with_term(&tab)?;
    let ev = Event {
        action: action.into(),
        button,
        shift,
        alt,
        ctrl,
        col,
        row,
    };
    let Some(bytes) = mouse::report(&ev, sh.vt.mode()) else {
        return Ok(false);
    };
    sh.write(&bytes);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The action names `pointer.ts` sends: a renamed variant would make every report fail
    // to deserialize and silently leave vim/tmux without a mouse.
    #[test]
    fn wire_actions_match_what_the_webview_sends() {
        let parse = |s: &str| serde_json::from_str::<WireAction>(&format!("\"{s}\"")).unwrap();
        assert_eq!(parse("press"), WireAction::Press);
        assert_eq!(parse("release"), WireAction::Release);
        assert_eq!(parse("motion"), WireAction::Motion);
        assert_eq!(parse("wheelUp"), WireAction::WheelUp);
        assert_eq!(parse("wheelDown"), WireAction::WheelDown);
        assert!(serde_json::from_str::<WireAction>("\"scroll\"").is_err());
    }
}
