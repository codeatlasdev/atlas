//! Every global hotkey Atlas owns (the dock's summon and hide keys, the capture keys), as one table.
//!
//! A hotkey is a stroke in the keymap's grammar (`ctrl-alt-space`, `cmd-shift-s`). Registration goes through
//! `tauri-plugin-global-shortcut` (Carbon `RegisterEventHotKey` on macOS: no Accessibility permission). Keeping
//! the table in one place is what makes the checks possible:
//!
//! * [`accelerator`] turns a stroke into the plugin's accelerator, or says in a sentence why it cannot work;
//! * [`clash`] finds two actions sharing one stroke (the plugin would refuse the second registration, and the
//!   user would lose a hotkey with no explanation);
//! * [`reserved`] names the strokes macOS already owns (⇧⌘3/4/5 screenshots, ⌘Space, ⌘Tab …) and the ones apps
//!   own and a global hotkey would steal from them (⇧⌘S "Save As", ⌘C …): a global hotkey wins over the focused
//!   app, so those are refused or flagged before they are registered.

use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// The stroke grammar of `keys.ts` (`ctrl-alt-space`, `cmd-shift-k`) as the plugin's accelerator
/// (`Control+Alt+Space`). `None` for the empty stroke (no hotkey). A global hotkey needs a modifier, or it would
/// swallow a plain key in every app.
pub fn accelerator(stroke: &str) -> Result<Option<String>, String> {
    let stroke = stroke.trim().to_lowercase();
    if stroke.is_empty() {
        return Ok(None);
    }
    let mut parts: Vec<&str> = stroke.split('-').collect();
    // `ctrl--` is the minus key.
    let key = if stroke.ends_with("--") {
        parts.truncate(parts.len() - 2);
        "-".to_string()
    } else {
        parts.pop().unwrap_or_default().to_string()
    };
    if key.is_empty() {
        return Err(format!("\"{stroke}\" has no key"));
    }
    let mut out = Vec::new();
    for m in parts {
        out.push(match m {
            "ctrl" => "Control",
            "alt" => "Alt",
            "shift" => "Shift",
            "cmd" => "Command",
            other => return Err(format!("unknown modifier \"{other}\" in \"{stroke}\"")),
        });
    }
    if out.is_empty() {
        return Err("A global hotkey needs a modifier (⌃, ⌥, ⇧ or ⌘)".into());
    }
    let key = match key.as_str() {
        "escape" => "Escape".into(),
        "up" => "ArrowUp".into(),
        "down" => "ArrowDown".into(),
        "left" => "ArrowLeft".into(),
        "right" => "ArrowRight".into(),
        "space" => "Space".into(),
        "enter" => "Enter".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        k => k.to_string(),
    };
    let accel = format!("{}+{key}", out.join("+"));
    accel
        .parse::<Shortcut>()
        .map_err(|e| format!("\"{stroke}\" is not a key the system can listen for ({e})"))?;
    Ok(Some(accel))
}

/// The modifiers and the key of a stroke, order-independent: `shift-cmd-s` and `cmd-shift-s` are the same hotkey.
/// `None` for an empty or malformed stroke.
fn canonical(stroke: &str) -> Option<String> {
    let stroke = stroke.trim().to_lowercase();
    if stroke.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = if stroke.ends_with("--") {
        parts.truncate(parts.len() - 2);
        "-".to_string()
    } else {
        parts.pop()?.to_string()
    };
    let mut mods: Vec<&str> = parts;
    mods.sort_unstable();
    mods.dedup();
    mods.push(&key);
    Some(mods.join("-"))
}

/// Two strokes that press the same keys.
pub fn same(a: &str, b: &str) -> bool {
    matches!((canonical(a), canonical(b)), (Some(x), Some(y)) if x == y)
}

/// The first two entries of `(name, stroke)` that share a stroke. Empty strokes (no hotkey) never clash.
pub fn clash<'a>(entries: &[(&'a str, &str)]) -> Option<(&'a str, &'a str)> {
    for (i, (na, a)) in entries.iter().enumerate() {
        for (nb, b) in &entries[i + 1..] {
            if same(a, b) {
                return Some((na, nb));
            }
        }
    }
    None
}

/// What owns a stroke already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// macOS handles it before any app: a global hotkey cannot take it, or must not.
    System(&'static str),
    /// A standard command in the focused app: registering it globally would steal it from every app.
    Apps(&'static str),
}

/// Strokes macOS or ordinary apps already own, with what they do. `⇧⌘S` is deliberately `Apps`, not `System`:
/// it is a preset the maintainer asked for, and the Settings note says what it costs.
pub fn reserved(stroke: &str) -> Option<Owner> {
    let c = canonical(stroke)?;
    Some(match c.as_str() {
        "cmd-shift-3" => Owner::System("macOS full-screen screenshot (⇧⌘3)"),
        "cmd-shift-4" => Owner::System("macOS area screenshot (⇧⌘4)"),
        "cmd-shift-5" => Owner::System("macOS screenshot and recording toolbar (⇧⌘5)"),
        "cmd-shift-6" => Owner::System("macOS Touch Bar screenshot (⇧⌘6)"),
        "cmd-space" => Owner::System("Spotlight (⌘Space)"),
        "alt-cmd-space" => Owner::System("Finder search (⌥⌘Space)"),
        "cmd-tab" | "cmd-shift-tab" => Owner::System("the app switcher (⌘Tab)"),
        "cmd-q" => Owner::System("Quit (⌘Q)"),
        "cmd-h" | "alt-cmd-h" => Owner::System("Hide (⌘H)"),
        "cmd-m" => Owner::System("Minimize (⌘M)"),
        "cmd-shift-s" => Owner::Apps("Save As in most apps (⇧⌘S)"),
        "cmd-c" | "cmd-v" | "cmd-x" | "cmd-z" | "cmd-shift-z" | "cmd-a" => {
            Owner::Apps("a standard editing command")
        }
        "cmd-s" | "cmd-o" | "cmd-n" | "cmd-w" | "cmd-p" | "cmd-f" | "cmd-t" => {
            Owner::Apps("a standard app command")
        }
        _ => return None,
    })
}

/// The accelerators one owner (the dock, the capture module) has registered with the system. Applying a new set
/// releases the old one first; a failure rolls back to nothing registered by this owner and says which key.
#[derive(Default)]
pub struct Registered(Mutex<Vec<String>>);

impl Registered {
    /// Make exactly `keys` (`(stroke, handler)`) the hotkeys of this owner. Empty strokes are skipped. A handler
    /// runs on key press only (never on release). On error nothing of the new set stays registered; the caller
    /// re-applies its previous set to restore what worked.
    #[allow(clippy::type_complexity)]
    pub fn apply(
        &self,
        app: &AppHandle,
        keys: Vec<(String, Box<dyn Fn(&AppHandle) + Send + Sync + 'static>)>,
    ) -> Result<(), String> {
        let gs = app.global_shortcut();
        let mut mine = self.0.lock();
        for old in mine.drain(..) {
            let _ = gs.unregister(old.as_str());
        }
        for (stroke, handler) in keys {
            let Some(accel) = accelerator(&stroke)? else {
                continue;
            };
            let result = gs.on_shortcut(accel.as_str(), move |app, _, ev| {
                if ev.state == ShortcutState::Pressed {
                    handler(app);
                }
            });
            if let Err(e) = result {
                for done in mine.drain(..) {
                    let _ = gs.unregister(done.as_str());
                }
                return Err(format!(
                    "{stroke} is taken by another app or the system ({e})"
                ));
            }
            mine.push(accel);
        }
        Ok(())
    }
}

/// What Settings shows under a stroke before it is saved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "reason", rename_all = "camelCase")]
pub enum Verdict {
    /// Free as far as we can tell (another app may still hold it: registering finds out).
    Ok,
    /// Works, but takes a command from the apps you use. The page says what it costs.
    Warn(String),
    /// Cannot work: the system owns it, or it is not a hotkey.
    Refused(String),
}

/// macOS's own screenshot strokes (⇧⌘3, ⇧⌘4, ⇧⌘5, ⇧⌘6): the only ones a person can free in System Settings >
/// Keyboard > Keyboard Shortcuts > Screenshots, and so the only ones "I turned them off" lets through.
pub fn is_screenshot_key(stroke: &str) -> bool {
    matches!(
        canonical(stroke).as_deref(),
        Some("cmd-shift-3" | "cmd-shift-4" | "cmd-shift-5" | "cmd-shift-6")
    )
}

/// The verdict on `stroke`: the accelerator's own sentence when it is not a hotkey, a refusal for the keys macOS
/// owns, a warning for the app commands a global hotkey would steal.
#[cfg(test)]
fn check(stroke: &str) -> Verdict {
    check_with(stroke, false)
}

/// [`check`], with the macOS screenshot keys let through when `screenshot_keys_free` (the person turned them off
/// in System Settings and said so; Atlas never changes that setting).
pub fn check_with(stroke: &str, screenshot_keys_free: bool) -> Verdict {
    if let Err(e) = accelerator(stroke) {
        return Verdict::Refused(e);
    }
    if screenshot_keys_free && is_screenshot_key(stroke) {
        return Verdict::Ok;
    }
    match reserved(stroke) {
        Some(Owner::System(r)) => Verdict::Refused(format!(
            "{r} already uses this key: macOS takes it before Atlas can"
        )),
        Some(Owner::Apps(r)) => Verdict::Warn(format!(
            "{r}. A global hotkey wins over the app in front, so that command stops working everywhere while Atlas runs"
        )),
        None => Verdict::Ok,
    }
}

/// Settings: the verdict on a stroke the user just recorded. `screenshot_keys_free` is the "I turned the macOS
/// screenshot shortcuts off" box.
#[tauri::command]
pub fn hotkey_check(stroke: String, screenshot_keys_free: Option<bool>) -> Verdict {
    check_with(&stroke, screenshot_keys_free.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings asks Rust about each stroke before saving: the verdict is what the page shows under the key.
    #[test]
    fn the_verdict_refuses_system_keys_warns_on_app_keys_and_passes_the_rest() {
        assert_eq!(check("ctrl-alt-4"), Verdict::Ok);
        assert_eq!(check(""), Verdict::Ok);
        match check("cmd-shift-4") {
            Verdict::Refused(r) => assert!(r.contains("⇧⌘4"), "{r}"),
            v => panic!("{v:?}"),
        }
        match check("shift-cmd-s") {
            Verdict::Warn(r) => assert!(r.contains("Save As"), "{r}"),
            v => panic!("{v:?}"),
        }
        // a stroke the system cannot listen for is refused with the accelerator's own sentence
        assert!(matches!(check("k"), Verdict::Refused(r) if r.contains("modifier")));
    }

    #[test]
    fn strokes_from_the_keymap_become_accelerators() {
        for (stroke, want) in [
            ("ctrl-alt-space", "Control+Alt+Space"),
            ("cmd-shift-k", "Command+Shift+k"),
            ("alt-1", "Alt+1"),
            ("ctrl-alt-f5", "Control+Alt+f5"),
            ("ctrl-shift-escape", "Control+Shift+Escape"),
            ("ctrl-alt-left", "Control+Alt+ArrowLeft"),
            ("ctrl--", "Control+-"),
            ("ctrl-alt-`", "Control+Alt+`"),
        ] {
            assert_eq!(
                accelerator(stroke).unwrap().as_deref(),
                Some(want),
                "{stroke}"
            );
        }
    }

    #[test]
    fn an_empty_stroke_means_no_hotkey() {
        assert_eq!(accelerator("").unwrap(), None);
        assert_eq!(accelerator("   ").unwrap(), None);
    }

    /// A hotkey without a modifier would swallow that key in every app (a bare `space`, a bare `k`).
    #[test]
    fn a_hotkey_needs_a_modifier_and_a_key_the_system_knows() {
        assert!(accelerator("space").unwrap_err().contains("modifier"));
        assert!(accelerator("k").unwrap_err().contains("modifier"));
        assert!(
            accelerator("hyper-k")
                .unwrap_err()
                .contains("unknown modifier")
        );
        assert!(accelerator("ctrl-alt-").unwrap_err().contains("no key"));
        assert!(
            accelerator("ctrl-alt-banana")
                .unwrap_err()
                .contains("not a key")
        );
    }

    /// `⇧⌘S` typed as shift-cmd-s or cmd-shift-s is one hotkey: the clash check must not be fooled by the order
    /// the Settings recorder happened to emit the modifiers in.
    #[test]
    fn the_same_keys_in_another_modifier_order_are_the_same_hotkey() {
        assert!(same("cmd-shift-s", "shift-cmd-s"));
        assert!(same("Ctrl-Alt-D", "alt-ctrl-d"));
        assert!(!same("cmd-shift-s", "cmd-s"));
        assert!(!same("ctrl-alt-d", "ctrl-alt-e"));
        // two empty strokes are two unset hotkeys, not a clash
        assert!(!same("", ""));
        assert!(!same("  ", "ctrl-alt-d"));
        // the minus key keeps its identity
        assert!(same("ctrl--", "ctrl--"));
        assert!(!same("ctrl--", "ctrl-="));
    }

    #[test]
    fn a_clash_names_both_actions() {
        let e = [
            ("summon", "ctrl-alt-space"),
            ("toggle", "ctrl-alt-d"),
            ("region", "alt-ctrl-d"),
        ];
        assert_eq!(clash(&e), Some(("toggle", "region")));
        assert_eq!(clash(&e[..2]), None);
        assert_eq!(clash(&[("a", ""), ("b", "")]), None);
        assert_eq!(clash(&[]), None);
    }

    /// The incident this guards is the request itself: ⇧⌘3/4/5 are macOS's own screenshot keys, and ⇧⌘S is "Save
    /// As" in most apps. A capture hotkey on one of them either never fires (system) or breaks Save As everywhere.
    #[test]
    fn macos_screenshot_keys_are_system_reserved_and_shift_cmd_s_is_an_apps_conflict() {
        for s in [
            "cmd-shift-3",
            "shift-cmd-4",
            "cmd-shift-5",
            "cmd-space",
            "cmd-tab",
        ] {
            assert!(matches!(reserved(s), Some(Owner::System(_))), "{s}");
        }
        assert!(matches!(reserved("shift-cmd-s"), Some(Owner::Apps(_))));
        assert!(matches!(reserved("cmd-c"), Some(Owner::Apps(_))));
        for s in [
            "ctrl-alt-d",
            "ctrl-alt-space",
            "ctrl-shift-4",
            "alt-cmd-4",
            "",
        ] {
            assert_eq!(reserved(s), None, "{s}");
        }
    }

    /// "I turned the macOS screenshot keys off" frees exactly those four strokes, never Spotlight or the app switcher.
    #[test]
    fn freeing_the_screenshot_keys_lets_them_through_and_nothing_else() {
        for s in ["cmd-shift-3", "shift-cmd-4", "cmd-shift-5", "cmd-shift-6"] {
            assert!(matches!(check_with(s, false), Verdict::Refused(_)), "{s}");
            assert_eq!(check_with(s, true), Verdict::Ok, "{s}");
        }
        for s in ["cmd-space", "cmd-tab", "cmd-q"] {
            assert!(matches!(check_with(s, true), Verdict::Refused(_)), "{s}");
        }
        // ⇧⌘S stays a warning either way
        assert!(matches!(check_with("shift-cmd-s", true), Verdict::Warn(_)));
        // the plain check is the not-freed one
        assert_eq!(check("cmd-shift-4"), check_with("cmd-shift-4", false));
    }

    #[test]
    fn only_the_screenshot_strokes_are_system_screenshot_keys() {
        assert!(is_screenshot_key("cmd-shift-3"));
        assert!(is_screenshot_key("shift-cmd-6"));
        assert!(!is_screenshot_key("cmd-shift-7"));
        assert!(!is_screenshot_key("ctrl-shift-3"));
        assert!(!is_screenshot_key(""));
    }
}
