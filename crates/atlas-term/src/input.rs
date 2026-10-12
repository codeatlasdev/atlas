//! Keyboard → bytes: xterm legacy encoding, or the Kitty keyboard protocol
//! (flags 1 disambiguate, 8 all keys as escapes, 16 associated text) when the app
//! pushes it. Press events only: the webview path does not forward key releases.
//! Keys arrive as DOM `KeyboardEvent.key` names from the webview.

use alacritty_terminal::term::TermMode;

#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    /// ⌘ (Super). Only the macOS line-editing chords (`line_edit`) use it; other ⌘ chords
    /// are app shortcuts and never reach the encoder.
    pub meta: bool,
}

impl Mods {
    /// xterm modifier parameter (1 = none). Kitty also carries super (8).
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8 + 8 * self.meta as u8
    }
}

/// macOS line editing, as in Terminal.app / iTerm2 / Ghostty defaults. Legacy encoding
/// sends the readline bytes; the Kitty keyboard protocol sends the real key with its
/// modifier (super = 8) so a program that asked for it can bind ⌘⌫ itself.
///
/// | chord | legacy | meaning |
/// |---|---|---|
/// | ⌘⌫ | `^U` | delete to line start |
/// | ⌘← / ⌘→ | `^A` / `^E` | line start / end |
/// | ⌥⌫ | `ESC DEL` | delete word back |
/// | ⌥← / ⌥→ | `ESC b` / `ESC f` | word back / forward |
/// | ⌥Delete | `ESC d` | delete word forward |
fn line_edit(name: &str, m: Mods) -> Option<&'static [u8]> {
    if m.shift || m.ctrl {
        return None;
    }
    match (name, m.meta, m.alt) {
        ("Backspace", true, false) => Some(b"\x15"),
        ("ArrowLeft", true, false) => Some(b"\x01"),
        ("ArrowRight", true, false) => Some(b"\x05"),
        ("Backspace", false, true) => Some(b"\x1b\x7f"),
        ("ArrowLeft", false, true) => Some(b"\x1bb"),
        ("ArrowRight", false, true) => Some(b"\x1bf"),
        ("Delete", false, true) => Some(b"\x1bd"),
        _ => None,
    }
}

/// Bytes for a non-text key, or `None` when the key produces no input
/// (text arrives separately through `text`).
pub fn key(name: &str, m: Mods, mode: TermMode) -> Option<Vec<u8>> {
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
        && let Some(b) = kitty(name, m, mode)
    {
        return Some(b);
    }
    // Legacy terminals get the readline bytes; a program that pushed the Kitty protocol gets
    // the real key with its modifiers (`CSI 1;9D`, `CSI 127;9u`) and binds it itself.
    if !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
        && let Some(b) = line_edit(name, m)
    {
        return Some(b.to_vec());
    }
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let p = m.param();
    // CSI <final> keys: arrows, Home/End (SS3 in app-cursor mode without modifiers).
    let csi_final = |f: char| -> Vec<u8> {
        if p > 1 {
            format!("\x1b[1;{p}{f}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{f}").into_bytes()
        } else {
            format!("\x1b[{f}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if p > 1 {
            format!("\x1b[{n};{p}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let alt = |b: &[u8]| -> Vec<u8> {
        if m.alt {
            [b"\x1b".as_slice(), b].concat()
        } else {
            b.to_vec()
        }
    };
    Some(match name {
        "ArrowUp" => csi_final('A'),
        "ArrowDown" => csi_final('B'),
        "ArrowRight" => csi_final('C'),
        "ArrowLeft" => csi_final('D'),
        "Home" => csi_final('H'),
        "End" => csi_final('F'),
        "Insert" => tilde(2),
        "Delete" => tilde(3),
        "PageUp" => tilde(5),
        "PageDown" => tilde(6),
        "F1" | "F2" | "F3" | "F4" => {
            let f = (b'P' + name.as_bytes()[1] - b'1') as char;
            if p > 1 {
                format!("\x1b[1;{p}{f}").into_bytes()
            } else {
                format!("\x1bO{f}").into_bytes()
            }
        }
        "F5" => tilde(15),
        "F6" => tilde(17),
        "F7" => tilde(18),
        "F8" => tilde(19),
        "F9" => tilde(20),
        "F10" => tilde(21),
        "F11" => tilde(23),
        "F12" => tilde(24),
        "Enter" if m.shift => b"\x1b[13;2u".to_vec(),
        "Enter" => alt(b"\r"),
        "Tab" if m.shift => b"\x1b[Z".to_vec(),
        "Tab" => alt(b"\t"),
        "Backspace" if m.ctrl => alt(b"\x08"),
        "Backspace" => alt(b"\x7f"),
        "Escape" => alt(b"\x1b"),
        _ => return ctrl_or_alt(name, m),
    })
}

/// Ctrl/Alt chords on printable keys (the webview delivers no text for these).
fn ctrl_or_alt(name: &str, m: Mods) -> Option<Vec<u8>> {
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() || !(m.ctrl || m.alt) {
        return None;
    }
    let byte = if m.ctrl {
        match c.to_ascii_lowercase() {
            c @ 'a'..='z' => c as u8 - b'a' + 1,
            '@' | ' ' | '2' => 0,
            '[' | '3' => 0x1b,
            '\\' | '4' => 0x1c,
            ']' | '5' => 0x1d,
            '^' | '6' => 0x1e,
            '_' | '-' | '7' => 0x1f,
            '/' => 0x1f,
            '8' | '?' => 0x7f,
            _ => return None,
        }
    } else if c.is_ascii() {
        c as u8
    } else {
        // Alt+non-ASCII: macOS composes these (option layer); they arrive as text.
        return None;
    };
    Some(if m.alt { vec![0x1b, byte] } else { vec![byte] })
}

/// Kitty `CSI code ; mods [; text] u`; `None` = same bytes as legacy.
fn kitty(name: &str, m: Mods, mode: TermMode) -> Option<Vec<u8>> {
    let all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let p = m.param();
    let csi_u = |code: u32| {
        if p > 1 {
            format!("\x1b[{code};{p}u").into_bytes()
        } else {
            format!("\x1b[{code}u").into_bytes()
        }
    };
    match name {
        "Escape" => Some(csi_u(27)),
        "Enter" | "Tab" | "Backspace" if all || p > 1 => Some(csi_u(match name {
            "Enter" => 13,
            "Tab" => 9,
            _ => 127,
        })),
        _ => {
            // Ctrl/Alt chords on printable keys: base (lowercase) codepoint + mods.
            let mut chars = name.chars();
            let c = chars.next()?;
            (chars.next().is_none() && (all || m.ctrl || m.alt))
                .then(|| csi_u(c.to_ascii_lowercase() as u32))
        }
    }
}

/// Typed text. With Kitty "report all keys as escapes", a single character becomes
/// `CSI code [; shift] [; text] u`; IME/multi-char input stays plain text.
pub fn text(t: &str, mode: TermMode) -> Vec<u8> {
    let mut chars = t.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return t.as_bytes().to_vec();
    };
    if !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
        return t.as_bytes().to_vec();
    }
    let base = c.to_ascii_lowercase();
    let shift = if base != c { ";2" } else { "" };
    let mut s = format!("\x1b[{}{shift}", base as u32);
    if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) {
        if shift.is_empty() {
            s.push_str(";1");
        }
        s.push_str(&format!(";{}", c as u32));
    }
    s.push('u');
    s.into_bytes()
}

/// Pasted text, bracketed when the app asked for it (newlines as CR).
pub fn paste(text: &str, mode: TermMode) -> Vec<u8> {
    let body = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode.contains(TermMode::BRACKETED_PASTE) {
        // A pasted end marker would let the clipboard escape the bracket.
        let body = body.replace("\x1b[201~", "");
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

/// Wheel on the alternate screen (xterm "alternate scroll", DECSET 1007, on by default;
/// Ghostty `mouse-scroll` on the alt screen): `lines` > 0 scrolls up, sent as that many
/// Up arrows (Down for negative). `None` when the program has the real scrollback (primary
/// screen), asked for mouse reports (`MOUSE_MODE`, not implemented here: the program
/// would get nothing), or turned the mode off.
pub fn alt_scroll(lines: i32, mode: TermMode) -> Option<Vec<u8>> {
    if lines == 0
        || !mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        || mode.intersects(TermMode::MOUSE_MODE)
    {
        return None;
    }
    let arrow: &[u8] = match (lines > 0, mode.contains(TermMode::APP_CURSOR)) {
        (true, false) => b"\x1b[A",
        (true, true) => b"\x1bOA",
        (false, false) => b"\x1b[B",
        (false, true) => b"\x1bOB",
    };
    Some(arrow.repeat(lines.unsigned_abs().min(50) as usize))
}

/// ⇧PgUp / ⇧PgDn: scroll the terminal's own scrollback by a page instead of sending
/// `CSI 5;2~`. `Some(true)` = up. `None` on the alternate screen (the program owns the
/// keys) and for any other key.
pub fn shift_page(name: &str, mode: TermMode) -> Option<bool> {
    if mode.contains(TermMode::ALT_SCREEN) {
        return None;
    }
    match name {
        "PageUp" => Some(true),
        "PageDown" => Some(false),
        _ => None,
    }
}

/// Arrow keys that move the line editor's cursor by (`dx` columns, `dy` rows): the
/// vertical moves first, as a click on an earlier line of a wrapped prompt needs.
pub fn cursor_moves(dx: i32, dy: i32, mode: TermMode) -> Vec<u8> {
    let app = mode.contains(TermMode::APP_CURSOR);
    let arrow = |f: char| -> Vec<u8> {
        if app {
            format!("\x1bO{f}").into_bytes()
        } else {
            format!("\x1b[{f}").into_bytes()
        }
    };
    let mut out = Vec::new();
    let v = arrow(if dy < 0 { 'A' } else { 'B' });
    let h = arrow(if dx < 0 { 'D' } else { 'C' });
    for _ in 0..dy.unsigned_abs().min(200) {
        out.extend_from_slice(&v);
    }
    for _ in 0..dx.unsigned_abs().min(500) {
        out.extend_from_slice(&h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    };
    const CMD: Mods = Mods { meta: true, ..NONE };
    const OPT: Mods = Mods { alt: true, ..NONE };

    #[test]
    fn arrows_follow_cursor_mode_and_modifiers() {
        assert_eq!(key("ArrowUp", NONE, TermMode::empty()).unwrap(), b"\x1b[A");
        assert_eq!(
            key("ArrowUp", NONE, TermMode::APP_CURSOR).unwrap(),
            b"\x1bOA"
        );
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(
            key("ArrowLeft", ctrl, TermMode::APP_CURSOR).unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(
            key(
                "Delete",
                Mods {
                    shift: true,
                    ..NONE
                },
                TermMode::empty()
            )
            .unwrap(),
            b"\x1b[3;2~"
        );
        assert_eq!(key("F1", NONE, TermMode::empty()).unwrap(), b"\x1bOP");
    }

    #[test]
    fn control_chords() {
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(key("c", ctrl, TermMode::empty()).unwrap(), [3]);
        assert_eq!(key("C", ctrl, TermMode::empty()).unwrap(), [3]);
        assert_eq!(key("[", ctrl, TermMode::empty()).unwrap(), [0x1b]);
        assert_eq!(
            key("b", Mods { alt: true, ..NONE }, TermMode::empty()).unwrap(),
            b"\x1bb"
        );
        // Plain text is not a key: it comes through the text path.
        assert_eq!(key("a", NONE, TermMode::empty()), None);
        assert_eq!(key("Shift", NONE, TermMode::empty()), None);
    }

    #[test]
    fn kitty_disambiguates_escape_and_chords_only() {
        let k = TermMode::DISAMBIGUATE_ESC_CODES;
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(key("Escape", NONE, k).unwrap(), b"\x1b[27u");
        assert_eq!(key("c", ctrl, k).unwrap(), b"\x1b[99;5u");
        assert_eq!(key("C", ctrl, k).unwrap(), b"\x1b[99;5u");
        assert_eq!(key("Enter", ctrl, k).unwrap(), b"\x1b[13;5u");
        // Unmodified Enter, arrows and text keep their legacy bytes.
        assert_eq!(key("Enter", NONE, k).unwrap(), b"\r");
        assert_eq!(key("ArrowUp", NONE, k).unwrap(), b"\x1b[A");
        assert_eq!(text("a", k), b"a");
    }

    #[test]
    fn kitty_report_all_encodes_text_with_shift_and_associated_text() {
        let all = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_ALL_KEYS_AS_ESC;
        assert_eq!(key("Enter", NONE, all).unwrap(), b"\x1b[13u");
        assert_eq!(text("a", all), b"\x1b[97u");
        assert_eq!(text("A", all), b"\x1b[97;2u");
        let with_text = all | TermMode::REPORT_ASSOCIATED_TEXT;
        assert_eq!(text("a", with_text), b"\x1b[97;1;97u");
        assert_eq!(text("A", with_text), b"\x1b[97;2;65u");
        // IME output is not a key.
        assert_eq!(text("日本", all), "日本".as_bytes());
    }

    #[test]
    fn bracketed_paste_strips_embedded_end_marker() {
        assert_eq!(paste("a\nb", TermMode::empty()), b"a\rb");
        assert_eq!(
            paste("x\x1b[201~rm", TermMode::BRACKETED_PASTE),
            b"\x1b[200~xrm\x1b[201~"
        );
    }

    /// The macOS line-editing map (Terminal.app / iTerm2 / Ghostty defaults), legacy encoding.
    #[test]
    fn macos_line_editing_map_legacy() {
        let legacy = TermMode::empty();
        let table: [(&str, Mods, &[u8]); 7] = [
            ("Backspace", CMD, b"\x15"),
            ("ArrowLeft", CMD, b"\x01"),
            ("ArrowRight", CMD, b"\x05"),
            ("Backspace", OPT, b"\x1b\x7f"),
            ("ArrowLeft", OPT, b"\x1bb"),
            ("ArrowRight", OPT, b"\x1bf"),
            ("Delete", OPT, b"\x1bd"),
        ];
        for (name, m, want) in table {
            assert_eq!(key(name, m, legacy).unwrap(), want, "{name} {m:?}");
            // Application cursor mode (readline in `less`, vim) does not change them.
            assert_eq!(
                key(name, m, TermMode::APP_CURSOR).unwrap(),
                want,
                "{name} {m:?} app-cursor"
            );
        }
        // Unmodified keys are untouched.
        assert_eq!(key("Backspace", NONE, legacy).unwrap(), b"\x7f");
        assert_eq!(key("ArrowLeft", NONE, legacy).unwrap(), b"\x1b[D");
        // ⌘ plus another modifier is not a line-editing chord: standard xterm parameters.
        let cmd_shift = Mods { shift: true, ..CMD };
        assert_eq!(key("ArrowLeft", cmd_shift, legacy).unwrap(), b"\x1b[1;10D");
    }

    #[test]
    fn macos_line_editing_map_kitty() {
        let k = TermMode::DISAMBIGUATE_ESC_CODES;
        // The real key with its modifier (alt 3, super 9): the program binds it.
        assert_eq!(key("Backspace", CMD, k).unwrap(), b"\x1b[127;9u");
        assert_eq!(key("Backspace", OPT, k).unwrap(), b"\x1b[127;3u");
        assert_eq!(key("ArrowLeft", CMD, k).unwrap(), b"\x1b[1;9D");
        assert_eq!(key("ArrowRight", CMD, k).unwrap(), b"\x1b[1;9C");
        assert_eq!(key("ArrowLeft", OPT, k).unwrap(), b"\x1b[1;3D");
        assert_eq!(key("ArrowRight", OPT, k).unwrap(), b"\x1b[1;3C");
        assert_eq!(key("Delete", OPT, k).unwrap(), b"\x1b[3;3~");
        // Plain Backspace keeps its legacy byte.
        assert_eq!(key("Backspace", NONE, k).unwrap(), b"\x7f");
    }

    #[test]
    fn wheel_on_the_alternate_screen_sends_arrows() {
        let alt = TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL;
        assert_eq!(alt_scroll(3, alt).unwrap(), b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(alt_scroll(-2, alt).unwrap(), b"\x1b[B\x1b[B");
        assert_eq!(
            alt_scroll(1, alt | TermMode::APP_CURSOR).unwrap(),
            b"\x1bOA"
        );
        // Primary screen keeps its scrollback; mode 1007 off, or a mouse-reporting program, opts out.
        assert_eq!(alt_scroll(3, TermMode::ALTERNATE_SCROLL), None);
        assert_eq!(alt_scroll(3, TermMode::ALT_SCREEN), None);
        assert_eq!(alt_scroll(3, alt | TermMode::MOUSE_REPORT_CLICK), None);
        assert_eq!(alt_scroll(0, alt), None);
        // A runaway flick is capped.
        assert_eq!(alt_scroll(500, alt).unwrap().len(), 50 * 3);
    }

    #[test]
    fn click_to_move_becomes_arrow_keys() {
        let m = TermMode::empty();
        assert_eq!(cursor_moves(-3, 0, m), b"\x1b[D\x1b[D\x1b[D");
        assert_eq!(cursor_moves(2, 0, TermMode::APP_CURSOR), b"\x1bOC\x1bOC");
        // Up first, then sideways.
        assert_eq!(cursor_moves(1, -1, m), b"\x1b[A\x1b[C");
        assert!(cursor_moves(0, 0, m).is_empty());
    }

    #[test]
    fn shift_page_scrolls_only_on_the_primary_screen() {
        assert_eq!(shift_page("PageUp", TermMode::empty()), Some(true));
        assert_eq!(shift_page("PageDown", TermMode::empty()), Some(false));
        assert_eq!(shift_page("PageUp", TermMode::ALT_SCREEN), None);
        assert_eq!(shift_page("Home", TermMode::empty()), None);
    }
}
