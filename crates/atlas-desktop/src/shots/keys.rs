//! The keystroke pill of a recording: how a key press is written (`⌘⇧4`) and which presses the pill still shows.
//! Pure (times are parameters, no AppKit), so the symbols, their order and the fade are tested everywhere.
//!
//! Privacy: nothing here logs or stores a key. The model holds the labels of the last few presses only while the
//! recording runs, each for [`SHOW_MS`], and is dropped with it.

/// A pressed key stays in the pill this long after its press.
pub const SHOW_MS: u64 = 1500;
/// The pill shows at most this many of the latest presses.
pub const MAX_SHOWN: usize = 4;

/// The modifier keys held with a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
}

impl Mods {
    /// The symbols in macOS menu order: ⌃ ⌥ ⇧ ⌘.
    fn symbols(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push('⌃');
        }
        if self.alt {
            s.push('⌥');
        }
        if self.shift {
            s.push('⇧');
        }
        if self.cmd {
            s.push('⌘');
        }
        s
    }
}

/// The name of a key by its virtual key code (the code is the physical key, so it is the same on every layout),
/// `None` for a key that has no fixed name: its printable character is used instead.
fn named(code: u16) -> Option<&'static str> {
    Some(match code {
        36 => "↩",
        76 => "⌤",
        48 => "⇥",
        49 => "Space",
        51 => "⌫",
        117 => "⌦",
        53 => "⎋",
        123 => "←",
        124 => "→",
        125 => "↓",
        126 => "↑",
        115 => "↖",
        119 => "↘",
        116 => "⇞",
        121 => "⇟",
        122 => "F1",
        120 => "F2",
        99 => "F3",
        118 => "F4",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        100 => "F8",
        101 => "F9",
        109 => "F10",
        103 => "F11",
        111 => "F12",
        105 => "F13",
        107 => "F14",
        113 => "F15",
        106 => "F16",
        64 => "F17",
        79 => "F18",
        80 => "F19",
        90 => "F20",
        _ => return None,
    })
}

/// How a press is written: the held modifiers as symbols (⌃⌥⇧⌘, in that order) then the key, a named key by its
/// symbol or word (arrows, Space, ↩, ⎋, F-keys…), any other by its character, upper-cased (`⌘⇧4`, `⌃C`).
///
/// `chars` is the key's character without modifiers (`charactersIgnoringModifiers`). A press with neither a name nor
/// a printable character (a dead key, a bare modifier) is `None`: the pill shows nothing for it.
pub fn format_key(mods: Mods, code: u16, chars: &str) -> Option<String> {
    let key = match named(code) {
        Some(n) => n.to_string(),
        None => {
            let c = chars
                .chars()
                .next()
                .filter(|c| !c.is_control() && !c.is_whitespace())?;
            c.to_uppercase().collect()
        }
    };
    Some(format!("{}{key}", mods.symbols()))
}

/// The presses the pill currently shows.
#[derive(Debug, Default)]
pub struct Pill {
    shown: Vec<(String, u64)>,
}

impl Pill {
    /// A press at `now_ms` (any monotonic millisecond clock).
    pub fn push(&mut self, label: String, now_ms: u64) {
        self.prune(now_ms);
        self.shown.push((label, now_ms));
        let extra = self.shown.len().saturating_sub(MAX_SHOWN);
        self.shown.drain(..extra);
    }

    fn prune(&mut self, now_ms: u64) {
        self.shown
            .retain(|(_, at)| now_ms.saturating_sub(*at) < SHOW_MS);
    }

    /// The labels to draw at `now_ms`, oldest first; empty once every press has faded.
    pub fn visible(&mut self, now_ms: u64) -> Vec<String> {
        self.prune(now_ms);
        self.shown.iter().map(|(l, _)| l.clone()).collect()
    }
}

/// Top-left of the pill (`size` points) at the bottom centre of `region` (global points), `margin` above its bottom
/// edge. A region narrower than the pill centres on it and the pill is pulled back inside the display `bounds` so
/// it is never cut off.
pub fn pill_origin(
    region: (f64, f64, f64, f64),
    bounds: (f64, f64, f64, f64),
    size: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    let x = region.0 + (region.2 - size.0) / 2.0;
    let y = region.1 + region.3 - size.1 - margin;
    let max_x = (bounds.0 + bounds.2 - size.0).max(bounds.0);
    let max_y = (bounds.1 + bounds.3 - size.1).max(bounds.1);
    (x.clamp(bounds.0, max_x), y.clamp(bounds.1, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        cmd: false,
    };

    #[test]
    fn modifiers_come_in_menu_order_then_the_key() {
        let all = Mods {
            ctrl: true,
            alt: true,
            shift: true,
            cmd: true,
        };
        assert_eq!(format_key(all, 21, "4").unwrap(), "⌃⌥⇧⌘4");
        let m = Mods {
            shift: true,
            cmd: true,
            ..NONE
        };
        assert_eq!(format_key(m, 21, "$").unwrap(), "⇧⌘$");
        let c = Mods { ctrl: true, ..NONE };
        assert_eq!(format_key(c, 8, "c").unwrap(), "⌃C");
        // order does not depend on which flag is set first
        let a = Mods {
            alt: true,
            cmd: true,
            ..NONE
        };
        assert_eq!(format_key(a, 49, " ").unwrap(), "⌥⌘Space");
    }

    #[test]
    fn named_keys_use_their_symbol_whatever_the_character_says() {
        for (code, want) in [
            (123, "←"),
            (124, "→"),
            (125, "↓"),
            (126, "↑"),
            (49, "Space"),
            (36, "↩"),
            (53, "⎋"),
            (48, "⇥"),
            (51, "⌫"),
            (117, "⌦"),
        ] {
            // the character of an arrow key is a private-use code point: it must never be printed
            assert_eq!(
                format_key(NONE, code, "\u{f702}").unwrap(),
                want,
                "code {code}"
            );
        }
    }

    #[test]
    fn function_keys_are_named_f1_to_f20() {
        let codes = [
            (122, 1),
            (120, 2),
            (99, 3),
            (118, 4),
            (96, 5),
            (97, 6),
            (98, 7),
            (100, 8),
            (101, 9),
            (109, 10),
            (103, 11),
            (111, 12),
            (105, 13),
            (107, 14),
            (113, 15),
            (106, 16),
            (64, 17),
            (79, 18),
            (80, 19),
            (90, 20),
        ];
        for (code, n) in codes {
            assert_eq!(format_key(NONE, code, "\u{f704}").unwrap(), format!("F{n}"));
        }
        let m = Mods { cmd: true, ..NONE };
        assert_eq!(format_key(m, 122, "").unwrap(), "⌘F1");
    }

    #[test]
    fn a_letter_is_shown_upper_case_and_a_key_without_a_character_is_skipped() {
        assert_eq!(format_key(NONE, 0, "a").unwrap(), "A");
        assert_eq!(format_key(NONE, 0, "ß").unwrap(), "SS");
        // dead key / bare modifier: no character, no name
        assert_eq!(format_key(NONE, 0, ""), None);
        assert_eq!(
            format_key(NONE, 0, "\u{7f}"),
            None,
            "control characters are never printed"
        );
        assert_eq!(
            format_key(NONE, 0, " "),
            None,
            "a space is the Space key (code 49), not a character"
        );
    }

    #[test]
    fn a_press_fades_after_a_second_and_a_half() {
        let mut p = Pill::default();
        p.push("A".into(), 1000);
        assert_eq!(p.visible(1000), ["A"]);
        assert_eq!(
            p.visible(2499),
            ["A"],
            "1499 ms after the press it is still there"
        );
        assert!(
            p.visible(2500).is_empty(),
            "1500 ms after the press it is gone"
        );
    }

    #[test]
    fn each_press_has_its_own_clock_and_the_pill_keeps_the_latest_few() {
        let mut p = Pill::default();
        p.push("A".into(), 0);
        p.push("B".into(), 1000);
        assert_eq!(
            p.visible(1600),
            ["B"],
            "A faded at 1500, B lives until 2500"
        );
        for (i, k) in ["C", "D", "E", "F", "G"].iter().enumerate() {
            p.push((*k).into(), 1700 + i as u64);
        }
        assert_eq!(p.visible(1800), ["D", "E", "F", "G"]);
    }

    /// The model never grows without bound while someone types for ten minutes.
    #[test]
    fn nothing_is_kept_after_the_presses_expire() {
        let mut p = Pill::default();
        for i in 0..10_000u64 {
            p.push("x".into(), i);
        }
        assert!(p.shown.len() <= MAX_SHOWN);
        assert!(p.visible(1_000_000).is_empty());
        assert!(p.shown.is_empty());
    }

    #[test]
    fn the_pill_sits_at_the_bottom_centre_of_the_region_and_stays_on_the_display() {
        let display = (0.0, 0.0, 1440.0, 900.0);
        let size = (420.0, 64.0);
        // whole display: centred, 40 above the bottom
        assert_eq!(pill_origin(display, display, size, 40.0), (510.0, 796.0));
        // a region in the middle
        assert_eq!(
            pill_origin((200.0, 100.0, 800.0, 500.0), display, size, 24.0),
            (200.0 + 190.0, 100.0 + 500.0 - 64.0 - 24.0)
        );
        // a region narrower than the pill, at the left edge: pulled back inside the display
        let (x, _) = pill_origin((0.0, 100.0, 100.0, 300.0), display, size, 24.0);
        assert_eq!(x, 0.0);
        // a display that is not at the origin
        let d2 = (1440.0, -200.0, 1920.0, 1080.0);
        let (x, y) = pill_origin(d2, d2, size, 40.0);
        assert_eq!((x, y), (1440.0 + 750.0, -200.0 + 1080.0 - 64.0 - 40.0));
    }
}
