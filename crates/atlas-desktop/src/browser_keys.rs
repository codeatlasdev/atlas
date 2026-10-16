//! Keys that must reach the app while an embedded browser page has focus.
//!
//! The page is a native child webview: AppKit hands its key events to that view, never to the app's
//! own webview, so the keymap handler in the frontend is blind there (issue #42: ⌘1…⌘9, ⌃Tab, ⌃⌘1…6
//! and user remaps were dead). The native menu is the only thing that survives, and it cannot carry
//! ⌃Tab or a remap.
//!
//! A `keyDown` monitor (`browser_keys_mac.rs`) sees the event first. This module is its pure
//! decision: the frontend pushes the strokes the effective keymap binds ([`set`]); an event whose
//! stroke is in that list is taken from the page and reported as [`Forwarded`] (`browser://key`),
//! and the app runs it through its own keymap. Nothing is injected into the page and the page
//! cannot send or forge anything: unbound keys, plain typing included, are never touched.

use std::collections::HashSet;
use std::sync::LazyLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Event the app listens to; the payload is [`Forwarded`].
pub const EVENT: &str = "browser://key";

/// A key taken from a page, in the shape the frontend's `strokeOf` reads (`apps/desktop/src/lib/browserKeys.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Forwarded {
    /// DOM `KeyboardEvent.code` of the physical key (`Digit1`, `Tab`, `BracketLeft`).
    pub code: &'static str,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

static STROKES: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Replace the forwarded strokes (canonical spelling, `keys.ts` `normalize`: `ctrl-alt-shift-cmd-<key>`).
pub fn set(strokes: Vec<String>) {
    *STROKES.lock() = strokes.into_iter().collect();
}

/// The frontend pushes the strokes its effective keymap binds (`browserKeys.ts` `forwardedStrokes`).
#[tauri::command]
pub fn browser_set_keys(strokes: Vec<String>) {
    set(strokes);
}

/// The canonical stroke of a physical key, spelled exactly as the frontend's `strokeOf` does
/// (`keys.ts`): modifiers in `ctrl-alt-shift-cmd` order, then the unshifted key. None for a key it cannot name.
pub fn stroke(code: &str, m: Mods) -> Option<String> {
    let key = if let Some(c) = code.strip_prefix("Key").filter(|c| c.len() == 1) {
        c.to_ascii_lowercase()
    } else if let Some(d) = code.strip_prefix("Digit").filter(|d| d.len() == 1) {
        d.to_string()
    } else if code.len() > 1
        && code.starts_with('F')
        && code[1..].bytes().all(|b| b.is_ascii_digit())
    {
        code.to_ascii_lowercase()
    } else {
        match code {
            "Backquote" => "`",
            "Backslash" => "\\",
            "BracketLeft" => "[",
            "BracketRight" => "]",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "Minus" => "-",
            "Equal" => "=",
            "Semicolon" => ";",
            "Quote" => "'",
            "Escape" => "escape",
            "Enter" => "enter",
            "Tab" => "tab",
            "Backspace" => "backspace",
            "Delete" => "delete",
            "Space" => "space",
            "ArrowUp" => "up",
            "ArrowDown" => "down",
            "ArrowLeft" => "left",
            "ArrowRight" => "right",
            "Home" => "home",
            "End" => "end",
            "PageUp" => "pageup",
            "PageDown" => "pagedown",
            _ => return None,
        }
        .to_string()
    };
    let mods = [
        (m.ctrl, "ctrl"),
        (m.alt, "alt"),
        (m.shift, "shift"),
        (m.meta, "cmd"),
    ];
    let mut out = String::new();
    for (on, name) in mods {
        if on {
            out.push_str(name);
            out.push('-');
        }
    }
    out.push_str(&key);
    Some(out)
}

/// Decide a key event: `Some` when the page must not get it and the app must (its stroke is in the
/// pushed list). Everything else, including every plain or shift-only key, returns None.
pub fn forward(mac_key_code: u16, m: Mods) -> Option<Forwarded> {
    let code = dom_code(mac_key_code)?;
    let s = stroke(code, m)?;
    STROKES.lock().contains(&s).then_some(Forwarded {
        code,
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
        meta: m.meta,
    })
}

/// DOM `code` of a macOS virtual key code (`kVK_*` in HIToolbox/Events.h). These are physical key
/// positions, so the result is layout-independent, like `strokeOf`.
pub fn dom_code(mac_key_code: u16) -> Option<&'static str> {
    Some(match mac_key_code {
        0 => "KeyA",
        1 => "KeyS",
        2 => "KeyD",
        3 => "KeyF",
        4 => "KeyH",
        5 => "KeyG",
        6 => "KeyZ",
        7 => "KeyX",
        8 => "KeyC",
        9 => "KeyV",
        11 => "KeyB",
        12 => "KeyQ",
        13 => "KeyW",
        14 => "KeyE",
        15 => "KeyR",
        16 => "KeyY",
        17 => "KeyT",
        18 => "Digit1",
        19 => "Digit2",
        20 => "Digit3",
        21 => "Digit4",
        22 => "Digit6",
        23 => "Digit5",
        24 => "Equal",
        25 => "Digit9",
        26 => "Digit7",
        27 => "Minus",
        28 => "Digit8",
        29 => "Digit0",
        30 => "BracketRight",
        31 => "KeyO",
        32 => "KeyU",
        33 => "BracketLeft",
        34 => "KeyI",
        35 => "KeyP",
        36 => "Enter",
        37 => "KeyL",
        38 => "KeyJ",
        39 => "Quote",
        40 => "KeyK",
        41 => "Semicolon",
        42 => "Backslash",
        43 => "Comma",
        44 => "Slash",
        45 => "KeyN",
        46 => "KeyM",
        47 => "Period",
        48 => "Tab",
        49 => "Space",
        50 => "Backquote",
        51 => "Backspace",
        53 => "Escape",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        99 => "F3",
        100 => "F8",
        101 => "F9",
        103 => "F11",
        105 => "F13",
        107 => "F14",
        109 => "F10",
        111 => "F12",
        113 => "F15",
        115 => "Home",
        116 => "PageUp",
        117 => "Delete",
        118 => "F4",
        119 => "End",
        120 => "F2",
        121 => "PageDown",
        122 => "F1",
        123 => "ArrowLeft",
        124 => "ArrowRight",
        125 => "ArrowDown",
        126 => "ArrowUp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        meta: true,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        meta: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: true,
        meta: false,
    };
    const CMD_SHIFT: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: true,
        meta: true,
    };
    const CTRL_CMD: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        meta: true,
    };
    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };

    // kVK_ANSI_1.. in macOS virtual key codes (HIToolbox/Events.h).
    const DIGITS: [u16; 9] = [18, 19, 20, 21, 23, 22, 26, 28, 25];
    const TAB: u16 = 48;
    const BRACKET_LEFT: u16 = 33;
    const BRACKET_RIGHT: u16 = 30;
    const KEY_A: u16 = 0;

    /// The strokes `apps/desktop` pushes for the default keymap (what matters here: tab and worktree switching).
    fn defaults() -> Vec<String> {
        let mut v: Vec<String> = (1..=9).map(|n| format!("cmd-{n}")).collect();
        v.extend((1..=6).map(|n| format!("ctrl-cmd-{n}")));
        v.extend(["ctrl-tab", "ctrl-shift-tab", "shift-cmd-[", "shift-cmd-]"].map(String::from));
        v
    }

    // The tests share one global list: serialize them so one cannot read another's strokes.
    static GUARD: Mutex<()> = Mutex::new(());

    fn with<R>(strokes: Vec<String>, f: impl FnOnce() -> R) -> R {
        let _g = GUARD.lock();
        set(strokes);
        let r = f();
        set(vec![]);
        r
    }

    // Regression: #42 — ⌘1…⌘9 did nothing while a browser page had focus.
    #[test]
    fn cmd_digits_are_forwarded_with_the_physical_digit_code() {
        with(defaults(), || {
            for (i, k) in DIGITS.iter().enumerate() {
                let f = forward(*k, CMD).unwrap_or_else(|| panic!("⌘{} not forwarded", i + 1));
                assert_eq!(f.code, format!("Digit{}", i + 1));
                assert!(f.meta && !f.ctrl && !f.alt && !f.shift);
            }
        });
    }

    // Regression: #42 — ⌃Tab / ⌃⇧Tab cannot be menu accelerators, so only forwarding can reach them.
    #[test]
    fn ctrl_tab_and_ctrl_shift_tab_are_forwarded() {
        with(defaults(), || {
            assert_eq!(forward(TAB, CTRL).unwrap().code, "Tab");
            let f = forward(TAB, CTRL_SHIFT).unwrap();
            assert_eq!((f.code, f.ctrl, f.shift), ("Tab", true, true));
        });
    }

    #[test]
    fn shift_cmd_brackets_and_ctrl_cmd_digits_are_forwarded() {
        with(defaults(), || {
            assert_eq!(
                forward(BRACKET_LEFT, CMD_SHIFT).unwrap().code,
                "BracketLeft"
            );
            assert_eq!(
                forward(BRACKET_RIGHT, CMD_SHIFT).unwrap().code,
                "BracketRight"
            );
            for (i, k) in DIGITS[..6].iter().enumerate() {
                let f =
                    forward(*k, CTRL_CMD).unwrap_or_else(|| panic!("⌃⌘{} not forwarded", i + 1));
                assert_eq!(f.code, format!("Digit{}", i + 1));
                assert!(f.ctrl && f.meta);
            }
        });
    }

    // The list is the effective keymap, so a user remap is forwarded and what it freed is not.
    #[test]
    fn a_remapped_stroke_is_forwarded_and_the_freed_one_reaches_the_page() {
        with(vec!["alt-cmd-1".into()], || {
            let alt_cmd = Mods {
                ctrl: false,
                alt: true,
                shift: false,
                meta: true,
            };
            assert_eq!(forward(DIGITS[0], alt_cmd).unwrap().code, "Digit1");
            assert_eq!(
                forward(DIGITS[0], CMD),
                None,
                "⌘1 was unbound: the page keeps it"
            );
        });
    }

    #[test]
    fn unbound_keys_and_plain_typing_stay_with_the_page() {
        with(defaults(), || {
            assert_eq!(forward(KEY_A, NONE), None, "typing a");
            assert_eq!(
                forward(
                    KEY_A,
                    Mods {
                        shift: true,
                        ..NONE
                    }
                ),
                None,
                "typing A"
            );
            assert_eq!(forward(DIGITS[0], NONE), None, "typing 1");
            assert_eq!(forward(TAB, NONE), None, "Tab moves focus in the page");
            assert_eq!(forward(KEY_A, CMD), None, "⌘A is the page's select-all");
            assert_eq!(
                forward(DIGITS[9 - 1], CTRL),
                None,
                "⌃9 is not bound in this list"
            );
        });
    }

    // A wrong row in the virtual-key table would swallow the wrong key (the Mac digit row is not
    // monotonic: 5 is 23, 6 is 22). Every code the table yields must be one `stroke` can name, and
    // no two physical keys may share a DOM code.
    #[test]
    fn virtual_key_table_is_injective_and_every_code_is_nameable() {
        let mut seen = std::collections::HashMap::new();
        for k in 0..=0x7Fu16 {
            let Some(code) = dom_code(k) else { continue };
            assert!(
                stroke(code, NONE).is_some(),
                "{code} (vk {k}) has no stroke spelling"
            );
            if let Some(prev) = seen.insert(code, k) {
                panic!("{code} is both vk {prev} and vk {k}");
            }
        }
        // The keys the default keymap binds with ⌘/⌃: all present.
        for code in [
            "Digit0",
            "Digit9",
            "Tab",
            "BracketLeft",
            "BracketRight",
            "Backslash",
            "Backquote",
            "Equal",
            "Minus",
            "Comma",
            "Period",
            "Slash",
            "F12",
            "ArrowLeft",
            "ArrowRight",
        ] {
            assert!(seen.contains_key(code), "{code} missing from the vk table");
        }
    }

    #[test]
    fn nothing_is_forwarded_before_the_frontend_pushes_a_list() {
        with(vec![], || assert_eq!(forward(DIGITS[0], CMD), None));
    }

    #[test]
    fn unknown_virtual_keys_never_match() {
        with(defaults(), || {
            assert_eq!(dom_code(0xFFFF), None);
            assert_eq!(forward(0xFFFF, CMD), None);
        });
    }

    // Same spelling as keys.ts `normalize`/`strokeOf`: modifiers ctrl-alt-shift-cmd, then the unshifted key.
    #[test]
    fn stroke_spelling_matches_the_frontend() {
        assert_eq!(stroke("Digit1", CMD).as_deref(), Some("cmd-1"));
        assert_eq!(stroke("Tab", CTRL_SHIFT).as_deref(), Some("ctrl-shift-tab"));
        assert_eq!(
            stroke("BracketLeft", CMD_SHIFT).as_deref(),
            Some("shift-cmd-[")
        );
        assert_eq!(stroke("Digit1", CTRL_CMD).as_deref(), Some("ctrl-cmd-1"));
        assert_eq!(stroke("F12", NONE).as_deref(), Some("f12"));
        assert_eq!(stroke("Minus", CMD).as_deref(), Some("cmd--"));
    }

    // Contract: `apps/desktop/src/lib/__fixtures__/browser-key.json` is the REAL serialization of a
    // forwarded key; `browserKeys.test.ts` feeds it to the frontend's own `strokeOf` path.
    // `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn forwarded_key_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/browser-key.json");
        let real = with(defaults(), || forward(TAB, CTRL_SHIFT).unwrap());
        let fresh = serde_json::to_value(&real).unwrap();
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
