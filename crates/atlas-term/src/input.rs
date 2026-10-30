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
    // Ghostty's macOS bindings are text, not keys: the same bytes whatever protocol the program pushed
    // (`super+backspace=text:\x15`, `super+left/right=text:\x01/\x05`, `alt+left/right=esc:b/f`). Under Kitty,
    // `CSI 127;9u` reached fish 4 unbound, so ⌘⌫ did nothing. ⌥⌫ / ⌥⌦ stay real keys (shells bind alt).
    if (m.meta || matches!(name, "ArrowLeft" | "ArrowRight"))
        && let Some(b) = line_edit(name, m)
    {
        return Some(b.to_vec());
    }
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
        && let Some(b) = kitty(name, m, mode)
    {
        return Some(b);
    }
    // ⌥ chords: legacy terminals get the readline bytes; a program that pushed the Kitty protocol gets the real key
    // with its modifier (`CSI 127;3u`) and binds it itself.
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
        // A pasted end marker would let the clipboard escape the bracket; strip until none
        // is left, since removing one can join two halves into a new one.
        let mut body = body;
        for end in ["\x1b[201~", "\u{9b}201~"] {
            while let Some(i) = body.find(end) {
                body.replace_range(i..i + end.len(), "");
            }
        }
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

/// What the macOS pasteboard holds, as far as a terminal paste cares (read natively: the
/// webview's paste event only exposes text, so an image or a Finder file copy arrives as
/// nothing or as a bare file name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clipboard {
    Empty,
    Text(String),
    /// Absolute paths of copied files (`public.file-url`), Finder ⌘C.
    Files(Vec<String>),
    /// Image data and no text (screenshot tools, "Copy Image").
    Image,
}

impl Clipboard {
    /// Pick the paste kind from what the pasteboard offers. Precedence (Ghostty, and omp
    /// #8769): copied files first (Finder also adds the file's name as text and an icon
    /// bitmap), then text, then a bitmap with no text.
    pub fn classify(files: Vec<String>, text: Option<String>, has_image: bool) -> Self {
        if !files.is_empty() {
            return Self::Files(files);
        }
        match text {
            Some(t) if !t.is_empty() => Self::Text(t),
            _ if has_image => Self::Image,
            _ => Self::Empty,
        }
    }
}

/// Bytes for ⌘V of `clip`, or `None` when there is nothing to send.
/// - text: a normal paste;
/// - files: their shell-quoted paths, space separated (Ghostty);
/// - image only: no terminal can carry the bitmap, so the agent is asked to read the
///   clipboard itself, the way the one that accepts it expects: an empty bracketed paste
///   (iTerm2 / Terminal.app; omp's `onPasteImage`) or, without bracketed paste, Ctrl+V
///   (omp `app.clipboard.pasteImage`, Claude Code `chat:imagePaste`, Codex `paste_image`).
pub fn paste_clipboard(clip: &Clipboard, mode: TermMode) -> Option<Vec<u8>> {
    match clip {
        Clipboard::Text(t) if !t.is_empty() => Some(paste(t, mode)),
        Clipboard::Files(f) if !f.is_empty() => {
            let paths: Vec<String> = f.iter().map(|p| shell_quote(p)).collect();
            Some(paste(&paths.join(" "), mode))
        }
        Clipboard::Image if mode.contains(TermMode::BRACKETED_PASTE) => Some(paste("", mode)),
        Clipboard::Image => key(
            "v",
            Mods {
                ctrl: true,
                ..Mods::default()
            },
            mode,
        ),
        _ => None,
    }
}

/// `shlex.quote`: safe paths stay bare, anything else is single-quoted.
pub fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
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

    const B: TermMode = TermMode::BRACKETED_PASTE;

    // A single-pass strip let `\x1b[2\x1b[201~01~` reassemble into an end marker.
    #[test]
    fn nested_end_marker_cannot_escape_the_bracket() {
        let out = String::from_utf8(paste("\x1b[2\x1b[201~01~rm", B)).unwrap();
        assert_eq!(out.matches("\x1b[201~").count(), 1, "{out:?}");
        assert!(out.ends_with("\x1b[201~"));
    }

    #[test]
    fn c1_end_marker_is_removed() {
        assert_eq!(paste("a\u{9b}201~b", B), "\x1b[200~ab\x1b[201~".as_bytes());
    }

    // #27: omp and other TUIs read ⇧⏎ as CSI u even without the kitty protocol.
    #[test]
    fn shift_enter_sends_csi_u_in_legacy_mode_too_issue_27() {
        let m = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(key("Enter", m, TermMode::empty()).unwrap(), b"\x1b[13;2u");
    }

    /// Incident: ⌘V of a screenshot did nothing in omp / Claude Code / Codex inside Atlas,
    /// because the webview paste event carries no text for an image-only clipboard and the
    /// paste was dropped. omp treats an empty bracketed paste as "read the clipboard image
    /// yourself" (its `onPasteImage`); that is what iTerm2 / Terminal.app send.
    #[test]
    fn image_only_clipboard_sends_an_empty_bracketed_paste() {
        assert_eq!(
            paste_clipboard(&Clipboard::Image, B).unwrap(),
            b"\x1b[200~\x1b[201~"
        );
    }

    /// Without bracketed paste there is no empty-paste signal: forward Ctrl+V, the key every
    /// agent binds to "paste image" (omp `app.clipboard.pasteImage`, Claude Code
    /// `chat:imagePaste`, Codex `paste_image`).
    #[test]
    fn image_only_clipboard_without_bracketed_paste_sends_ctrl_v() {
        assert_eq!(
            paste_clipboard(&Clipboard::Image, TermMode::empty()).unwrap(),
            [0x16]
        );
        // A program that turned on the Kitty keyboard protocol wants the chord as CSI u.
        assert_eq!(
            paste_clipboard(&Clipboard::Image, TermMode::DISAMBIGUATE_ESC_CODES).unwrap(),
            b"\x1b[118;5u"
        );
    }

    #[test]
    fn text_clipboard_is_the_text_paste() {
        assert_eq!(
            paste_clipboard(&Clipboard::Text("a\nb".into()), B).unwrap(),
            b"\x1b[200~a\rb\x1b[201~"
        );
        assert_eq!(
            paste_clipboard(&Clipboard::Text("a".into()), TermMode::empty()).unwrap(),
            b"a"
        );
    }

    /// Finder ⌘C of files: Ghostty pastes their paths, shell-quoted, space separated.
    #[test]
    fn file_clipboard_pastes_quoted_paths() {
        let files = Clipboard::Files(vec![
            "/tmp/shot.png".into(),
            "/tmp/My Shots/it's.png".into(),
        ]);
        assert_eq!(
            paste_clipboard(&files, B).unwrap(),
            b"\x1b[200~/tmp/shot.png '/tmp/My Shots/it'\\''s.png'\x1b[201~"
        );
        assert_eq!(
            paste_clipboard(&files, TermMode::empty()).unwrap(),
            b"/tmp/shot.png '/tmp/My Shots/it'\\''s.png'"
        );
    }

    /// A path can carry the bracket end marker or a newline: it must not escape the bracket
    /// nor split the command line.
    #[test]
    fn file_paths_cannot_escape_the_bracket() {
        let files = Clipboard::Files(vec!["/tmp/a\x1b[201~b\nc".into()]);
        let out = paste_clipboard(&files, B).unwrap();
        assert_eq!(out, b"\x1b[200~'/tmp/ab\rc'\x1b[201~");
    }

    #[test]
    fn empty_clipboards_send_nothing() {
        assert_eq!(paste_clipboard(&Clipboard::Empty, B), None);
        assert_eq!(paste_clipboard(&Clipboard::Text(String::new()), B), None);
        assert_eq!(paste_clipboard(&Clipboard::Files(vec![]), B), None);
    }

    #[test]
    fn shell_quote_leaves_safe_paths_and_quotes_the_rest() {
        assert_eq!(shell_quote("/a/b-c_d.png"), "/a/b-c_d.png");
        assert_eq!(shell_quote("/a b/$x"), "'/a b/$x'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    /// Finder ⌘C of a PNG puts its file URL, its name as text and a generated icon bitmap on
    /// the pasteboard (omp #8769 hit exactly this): the files win over the image, and over
    /// the bare file name.
    #[test]
    fn classify_prefers_files_over_icon_bitmap_and_name_text() {
        let c = Clipboard::classify(vec!["/tmp/shot.png".into()], Some("shot.png".into()), true);
        assert_eq!(c, Clipboard::Files(vec!["/tmp/shot.png".into()]));
    }

    /// A browser "Copy Image" carries the bitmap plus text: text keeps working for those,
    /// while a screenshot tool (bitmap only) is the image case.
    #[test]
    fn classify_image_only_is_image_and_text_beats_a_bitmap() {
        assert_eq!(Clipboard::classify(vec![], None, true), Clipboard::Image);
        assert_eq!(
            Clipboard::classify(vec![], Some(String::new()), true),
            Clipboard::Image
        );
        assert_eq!(
            Clipboard::classify(vec![], Some("hello".into()), true),
            Clipboard::Text("hello".into())
        );
        assert_eq!(Clipboard::classify(vec![], None, false), Clipboard::Empty);
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

    /// Incident (2026-10-26, fish 4 in the GPU terminal): fish enables the Kitty protocol, the ⌘ chords went out as
    /// super+key (`CSI 127;9u`, `CSI 1;9D`), which fish does not bind, so ⌘⌫ deleted nothing and ⌘← moved a word.
    /// Ghostty binds them as text (`super+backspace=text:\x15`, `super+left=text:\x01`, `super+right=text:\x05`),
    /// sent whatever protocol the program pushed. ⌥ chords stay real keys there: shells bind alt.
    #[test]
    fn macos_line_editing_map_kitty() {
        let k = TermMode::DISAMBIGUATE_ESC_CODES;
        assert_eq!(key("Backspace", CMD, k).unwrap(), b"\x15");
        assert_eq!(key("ArrowLeft", CMD, k).unwrap(), b"\x01");
        assert_eq!(key("ArrowRight", CMD, k).unwrap(), b"\x05");
        let all = k | TermMode::REPORT_ALL_KEYS_AS_ESC;
        assert_eq!(key("Backspace", CMD, all).unwrap(), b"\x15");
        // The real key with its modifier (alt 3): the program binds it.
        assert_eq!(key("Backspace", OPT, k).unwrap(), b"\x1b[127;3u");
        // Ghostty binds ⌥← ⌥→ as text too (`alt+left=esc:b`, `alt+right=esc:f`): word jumps in every shell.
        assert_eq!(key("ArrowLeft", OPT, k).unwrap(), b"\x1bb");
        assert_eq!(key("ArrowRight", OPT, k).unwrap(), b"\x1bf");
        assert_eq!(key("Delete", OPT, k).unwrap(), b"\x1b[3;3~");
        // Plain Backspace keeps its legacy byte.
        assert_eq!(key("Backspace", NONE, k).unwrap(), b"\x7f");
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
