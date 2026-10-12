//! The one keymap table. Help (`?`) and the footer both read it, so a key can
//! never be documented in one place and missing from the other. Behavior lives
//! in `app/input.rs`; a test pins every single-char key here to `BUILTIN_KEYS`.

use crossterm::event::KeyCode;

use crate::tui::app::Message;

/// Help section a binding is listed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Services,
    Navigation,
    Logs,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::General,
        Section::Services,
        Section::Navigation,
        Section::Logs,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "GENERAL",
            Section::Services => "SERVICES",
            Section::Navigation => "NAVIGATION",
            Section::Logs => "LOGS",
        }
    }
}

/// Where a binding is available (footer filtering + help context label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ctx {
    /// All and service tabs.
    Any,
    /// Only on a service tab.
    Service,
    /// Only on the All tab.
    All,
    /// Only on a session (`session: true`) tab.
    Session,
    /// Only when a `tunnel:` block is configured.
    Tunnel,
    /// Only in an attached client (`atlas dev` against a runner).
    Attached,
}

/// What the running session offers, beyond the tab: which optional rows apply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyCtx {
    pub has_tunnel: bool,
    pub attached: bool,
}

/// One row of the keymap.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    /// Display text of the key (`PgUp/PgDn`, `Ctrl+L`, `r`).
    pub keys: &'static str,
    /// Label shown in help.
    pub help: &'static str,
    /// Short label used by the footer; `None` keeps it out of the footer.
    pub footer: Option<&'static str>,
    pub section: Section,
    pub ctx: Ctx,
    /// Single-character keys this row binds (checked against `BUILTIN_KEYS`).
    pub chars: &'static str,
}

const fn b(
    keys: &'static str,
    help: &'static str,
    footer: Option<&'static str>,
    section: Section,
    ctx: Ctx,
    chars: &'static str,
) -> Binding {
    Binding {
        keys,
        help,
        footer,
        section,
        ctx,
        chars,
    }
}

use Ctx::*;
use Section::*;

/// Every built-in key, in display order.
pub const KEYMAP: &[Binding] = &[
    // GENERAL
    b(
        "q",
        "Quit (stops all services)",
        Some("Quit"),
        General,
        Any,
        "q",
    ),
    b(
        "D",
        "Detach: leave, services keep running",
        Some("Detach"),
        General,
        Attached,
        "D",
    ),
    b("?", "Help", Some("Help"), General, Any, "?"),
    b(":", "Command palette", Some("Commands"), General, Any, ":"),
    b("Ctrl+C", "Quit immediately", None, General, Any, ""),
    b("Ctrl+Z", "Suspend (fg to resume)", None, General, Any, ""),
    b("z", "Zoom: hide sidebar", Some("Zoom"), General, Any, "z"),
    b(
        "M",
        "Mouse capture on/off (⌥-drag selects)",
        None,
        General,
        Any,
        "M",
    ),
    b(
        "Ctrl+U/W",
        "In inputs: clear / delete word",
        None,
        General,
        Any,
        "",
    ),
    // SERVICES
    b(
        "r",
        "Restart all, asks first",
        Some("Restart all"),
        Services,
        All,
        "r",
    ),
    b(
        "r",
        "Restart this service",
        Some("Restart"),
        Services,
        Service,
        "r",
    ),
    b(
        "s",
        "Stop this service",
        Some("Stop"),
        Services,
        Service,
        "s",
    ),
    b("S", "Restart this service", None, Services, Service, "S"),
    b("m", "Service menu", Some("Menu"), Services, Service, "m"),
    b("t", "Keep tunnel open on exit", None, Services, Tunnel, "t"),
    b(
        "i",
        "Attach to session (Ctrl+B detaches)",
        Some("Attach"),
        Services,
        Session,
        "i",
    ),
    // NAVIGATION
    b("Tab", "Next tab", Some("Next tab"), Navigation, Any, ""),
    b("Shift+Tab", "Previous tab", None, Navigation, Any, ""),
    b("0-9", "Jump to tab", None, Navigation, Any, "0123456789"),
    b("j/k ↑/↓", "Scroll", Some("Scroll"), Navigation, Any, "jk"),
    b("PgUp/PgDn", "Scroll a page", None, Navigation, Any, ""),
    b("g/Home", "Oldest line", None, Navigation, Any, "g"),
    b("G/End", "Newest line", None, Navigation, Any, "G"),
    // LOGS
    b("f", "Follow / pause", Some("Follow"), Logs, Any, "f"),
    b("/", "Search", Some("Search"), Logs, Any, "/"),
    b("n/N", "Next / previous match", None, Logs, Any, "nN"),
    b(
        "v",
        "Level filter: all → warn+ → error",
        Some("Level"),
        Logs,
        Any,
        "v",
    ),
    b(
        "V",
        "Time range: 1m → 5m → 15m → 1h → since restart → since start → custom",
        Some("Time"),
        Logs,
        Any,
        "V",
    ),
    b(
        "]e / [e",
        "Next / previous error line",
        None,
        Logs,
        Any,
        "[]",
    ),
    b(
        "]w / [w",
        "Next / previous warning or error",
        None,
        Logs,
        Any,
        "",
    ),
    b(
        "}/{",
        "Next / previous link (URL, path:line)",
        None,
        Logs,
        Any,
        "{}",
    ),
    b(
        "o / y",
        "Open / copy the selected link",
        None,
        Logs,
        Any,
        "",
    ),
    b("W", "Wrap long lines (off: ←/→ pan)", None, Logs, Any, "W"),
    b("←/→", "Pan sideways (wrap off)", None, Logs, Any, ""),
    b("T", "Timestamps", None, Logs, Any, "T"),
    b("J", "Pretty-print JSON", None, Logs, Any, "J"),
    b("-", "Insert a separator line", None, Logs, Any, "-"),
    b("Tab", "In search: find ⇄ filter", None, Logs, Any, ""),
    b("Ctrl+R", "In search: regex on/off", None, Logs, Any, ""),
    b("L", "Copy logs", None, Logs, Any, "L"),
    b("P", "Copy logs as AI prompt", None, Logs, Any, "P"),
    b("E", "Copy errors", None, Logs, Any, "E"),
    b("Ctrl+L", "Clear logs", None, Logs, Any, ""),
];

/// Whether `b` applies in the given tab context.
pub fn applies(b: &Binding, on_service: bool, on_session: bool, k: KeyCtx) -> bool {
    match b.ctx {
        Any => true,
        Service => on_service && !on_session,
        All => !on_service,
        Session => on_session,
        Tunnel => k.has_tunnel,
        Attached => k.attached,
    }
}

/// Plain-text row for help: key column + label.
pub fn help_rows(section: Section, k: KeyCtx) -> Vec<&'static Binding> {
    KEYMAP
        .iter()
        .filter(|b| {
            b.section == section
                && (b.ctx != Tunnel || k.has_tunnel)
                && (b.ctx != Attached || k.attached)
        })
        .collect()
}

/// Bytes forwarded to an attached session's stdin for non-character keys.
pub const SESSION_KEYS: &[(KeyCode, &[u8])] = &[
    (KeyCode::Enter, b"\n"),
    (KeyCode::Backspace, b"\x7f"),
    (KeyCode::Esc, b"\x1b"),
    (KeyCode::Tab, b"\t"),
    (KeyCode::Up, b"\x1b[A"),
    (KeyCode::Down, b"\x1b[B"),
    (KeyCode::Left, b"\x1b[D"),
    (KeyCode::Right, b"\x1b[C"),
];

/// The message a footer click on `b` sends, if the key is safe to trigger by click.
pub fn click_message(b: &Binding, active: Option<&str>) -> Option<Message> {
    let svc = active.map(str::to_string);
    match (b.keys, b.ctx) {
        ("q", _) => Some(Message::ShowQuit),
        ("D", _) => Some(Message::Detach),
        ("?", _) => Some(Message::ShowHelp),
        (":", _) => Some(Message::ShowPalette),
        ("z", _) => Some(Message::ToggleZoom),
        ("r", All) => Some(Message::RestartAll),
        ("r", Service) => svc.map(Message::RestartService),
        ("s", Service) => svc.map(Message::StopService),
        ("m", Service) => svc.map(Message::ShowServiceMenu),
        ("i", Session) => Some(Message::AttachSession),
        ("Tab", Any) => Some(Message::NextTab),
        ("f", _) => Some(Message::ToggleFollow),
        ("/", _) => Some(Message::StartLogSearch),
        ("v", _) => Some(Message::CycleLevelFilter),
        ("V", _) => Some(Message::CycleTimeRange),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BUILTIN_KEYS;

    #[test]
    fn every_char_key_in_the_table_is_reserved() {
        for b in KEYMAP {
            for c in b.chars.chars() {
                assert!(
                    BUILTIN_KEYS.contains(&c),
                    "'{c}' ({}) is in the keymap but not BUILTIN_KEYS",
                    b.help
                );
            }
        }
    }

    #[test]
    fn every_reserved_key_is_documented() {
        let documented: String = KEYMAP.iter().map(|b| b.chars).collect();
        for c in BUILTIN_KEYS {
            assert!(
                documented.contains(*c),
                "'{c}' is in BUILTIN_KEYS but missing from the keymap table"
            );
        }
    }

    #[test]
    fn required_keys_are_present() {
        let all: String = KEYMAP.iter().map(|b| b.keys).collect::<Vec<_>>().join(" ");
        for k in [
            "f",
            ":",
            "Ctrl+C",
            "Ctrl+Z",
            "Shift+Tab",
            "g/Home",
            "G/End",
            "i",
            "n/N",
            "W",
            "T",
            "J",
            "z",
            "-",
            "v",
            "M",
            "PgUp/PgDn",
            "Ctrl+L",
        ] {
            assert!(all.contains(k), "keymap missing {k}");
        }
    }

    #[test]
    fn r_label_depends_on_context() {
        let r: Vec<_> = KEYMAP.iter().filter(|b| b.keys == "r").collect();
        assert_eq!(r.len(), 2);
        assert!(r.iter().any(|b| b.ctx == All && b.help.contains("all")));
        assert!(
            r.iter()
                .any(|b| b.ctx == Service && b.help.contains("this service"))
        );
    }
}
