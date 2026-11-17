//! A keyboard shortcut as text, for the places Rust writes one out (CLI hints, notifications).
//!
//! A shortcut is never typed inside a catalog message: the message takes a `{ $key }` placeholder
//! and the caller passes `shortcut("cmd-shift-p")`. That keeps `Ctrl+Shift+P` and `⇧⌘P` out of
//! every translation, and follows the OS. Same canonical strokes and same output as
//! `label()` in `apps/desktop/src/lib/keys.ts` (`cmd-shift-p`, `ctrl-space`, `alt-enter`).
//! Key names off macOS come from `keys.ftl`, so a language can spell `Space` its own way;
//! modifiers stay as printed on keyboards.

/// The shortcut for `stroke` (`-`-separated lowercase, last part the key; a trailing `--` is the
/// minus key) in the display language on this OS.
pub fn shortcut(stroke: &str) -> String {
    shortcut_for(stroke, crate::os(), crate::lang())
}

/// [`shortcut`] for an explicit OS and language (`mac`, `windows`, `linux`; a catalog tag).
pub fn shortcut_for(stroke: &str, os: &'static str, lang: &str) -> String {
    let mut parts: Vec<&str> = stroke.split('-').collect();
    if stroke.ends_with("--") {
        parts.truncate(parts.len() - 2);
        parts.push("-");
    }
    if os == "mac" {
        return parts.iter().map(|p| glyph(p)).collect();
    }
    let key = parts.pop().unwrap_or_default();
    let has = |m: &str| parts.contains(&m);
    let word = |id: &str| crate::lookup_on(lang, id, None, os).unwrap_or_else(|| id.to_owned());
    let mut out = Vec::new();
    if has("ctrl") || has("cmd") {
        out.push(word("key-ctrl"));
    }
    if has("alt") {
        out.push(word("key-alt"));
    }
    if has("shift") {
        out.push(word("key-shift"));
    }
    out.push(match key {
        "escape" => word("key-escape"),
        "enter" => word("key-enter"),
        "tab" => word("key-tab"),
        "backspace" => word("key-backspace"),
        "delete" => word("key-delete"),
        "space" => word("key-space"),
        "home" => word("key-home"),
        "end" => word("key-end"),
        "pageup" => word("key-pageup"),
        "pagedown" => word("key-pagedown"),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        other => other.to_uppercase(),
    });
    out.join("+")
}

fn glyph(part: &str) -> String {
    match part {
        "ctrl" => "⌃",
        "alt" => "⌥",
        "shift" => "⇧",
        "cmd" => "⌘",
        "escape" => "⎋",
        "enter" => "↩",
        "tab" => "⇥",
        "backspace" => "⌫",
        "delete" => "⌦",
        "home" => "↖",
        "end" => "↘",
        "pageup" => "⇞",
        "pagedown" => "⇟",
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        "space" => "␣",
        other => return other.to_uppercase(),
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // One table, every OS and language: the shortcut differs by OS and by the language's key
    // names, never by hand-written text in a message.
    #[test]
    fn shortcut_table_by_os_and_language() {
        let cases: &[(&str, &str, &str, &str)] = &[
            // stroke, os, lang, want
            ("cmd-shift-p", "mac", "en", "⌘⇧P"),
            ("cmd-shift-p", "windows", "en", "Ctrl+Shift+P"),
            ("cmd-shift-p", "linux", "pt-BR", "Ctrl+Shift+P"),
            ("ctrl-alt-left", "linux", "en", "Ctrl+Alt+←"),
            ("ctrl-alt-left", "mac", "pt-BR", "⌃⌥←"),
            ("cmd-enter", "mac", "en", "⌘↩"),
            ("cmd-enter", "windows", "pt-BR", "Ctrl+Enter"),
            ("ctrl-space", "windows", "en", "Ctrl+Space"),
            // pt-BR spells the space bar out; macOS uses the glyph in every language.
            ("ctrl-space", "windows", "pt-BR", "Ctrl+Espaço"),
            ("ctrl-space", "mac", "pt-BR", "⌃␣"),
            ("escape", "linux", "en", "Esc"),
            ("cmd--", "windows", "en", "Ctrl+-"),
            ("cmd--", "mac", "en", "⌘-"),
            ("pagedown", "windows", "pt-BR", "PgDn"),
        ];
        for (stroke, os, lang, want) in cases {
            assert_eq!(
                shortcut_for(stroke, os, lang),
                *want,
                "{stroke} on {os} in {lang}"
            );
        }
    }
}
