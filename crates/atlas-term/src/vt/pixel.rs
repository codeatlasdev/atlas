//! DECSET 1016 (SGR-Pixels mouse). alacritty's parser reports the mode as unknown and ignores it, so this
//! scanner follows `CSI ? Pm h|l` (also in lists: `?1000;1016;1006h`), answers DECRQM (`CSI ? 1016 $ p`)
//! and drops the mode on a reset (`CSI ! p` DECSTR, `ESC c` RIS), split-safe across `feed` chunks.
//!
//! Like xterm and Ghostty, 1016 is an *encoding*: it only matters while a mouse mode (1000/1002/1003) is on
//! and it takes precedence over 1006 (`mouse::report`). Setting 1006 or 1005 afterwards does not clear it.

/// The mode number.
const PIXEL: u16 = 1016;

/// What the stream did to mode 1016 (`PixelScan::scan`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum Change {
    /// `?1016h` / `?1016l` (the new state).
    Set(bool),
    /// `?1016$p`: the program asks whether it is set.
    Query,
    /// DECSTR or RIS: every mode is back to its default.
    Reset,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum St {
    #[default]
    Text,
    Esc,
    /// `ESC [`
    Csi,
    /// `ESC [ ?`: params being read.
    Private,
    /// `ESC [ !`: waiting for `p`.
    Bang,
}

/// Split-safe scanner for the sequences that change or query mode 1016.
#[derive(Default)]
pub(super) struct PixelScan {
    st: St,
    /// Parameter being read, and whether the list holds 1016.
    cur: Option<u32>,
    has: bool,
    /// Parameters seen in the list.
    n: u8,
    /// An intermediate `$` was seen (DECRQM `$p`).
    dollar: bool,
}

impl PixelScan {
    fn restart(&mut self, st: St) {
        self.st = st;
        self.cur = None;
        self.has = false;
        self.n = 0;
        self.dollar = false;
    }

    fn end_param(&mut self) {
        self.n = self.n.saturating_add(1);
        if self.cur.take() == Some(u32::from(PIXEL)) {
            self.has = true;
        }
    }

    /// The changes `bytes` make, in order.
    pub(super) fn scan(&mut self, bytes: &[u8]) -> Vec<Change> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == 0x1b {
                self.restart(St::Esc);
                continue;
            }
            match (self.st, b) {
                (St::Text, _) => {}
                (St::Esc, b'[') => self.restart(St::Csi),
                (St::Esc, b'c') => {
                    out.push(Change::Reset);
                    self.restart(St::Text);
                }
                (St::Csi, b'?') => self.restart(St::Private),
                (St::Csi, b'!') => self.restart(St::Bang),
                (St::Bang, b'p') => {
                    out.push(Change::Reset);
                    self.restart(St::Text);
                }
                (St::Private, b'0'..=b'9') => {
                    let d = u32::from(b - b'0');
                    // A huge parameter is not a mode; saturate rather than wrap onto 1016.
                    self.cur = Some(self.cur.unwrap_or(0).saturating_mul(10).saturating_add(d));
                }
                (St::Private, b';') => self.end_param(),
                (St::Private, b'$') => {
                    self.end_param();
                    self.dollar = true;
                }
                (St::Private, b'h' | b'l') if !self.dollar => {
                    self.end_param();
                    if self.has {
                        out.push(Change::Set(b == b'h'));
                    }
                    self.restart(St::Text);
                }
                (St::Private, b'p') if self.dollar => {
                    if self.has && self.n == 1 {
                        out.push(Change::Query);
                    }
                    self.restart(St::Text);
                }
                // Anything else ends the sequence (a different final byte, a control character).
                _ => self.restart(St::Text),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all(chunks: &[&[u8]]) -> Vec<Change> {
        let mut s = PixelScan::default();
        chunks.iter().flat_map(|c| s.scan(c)).collect()
    }

    #[test]
    fn set_reset_and_query_are_found() {
        assert_eq!(scan_all(&[b"\x1b[?1016h"]), [Change::Set(true)]);
        assert_eq!(scan_all(&[b"\x1b[?1016l"]), [Change::Set(false)]);
        assert_eq!(scan_all(&[b"\x1b[?1016$p"]), [Change::Query]);
    }

    // Programs combine modes in one sequence: `?1000;1016;1006h`.
    #[test]
    fn a_mode_list_with_1016_in_any_position_counts() {
        for seq in [
            &b"\x1b[?1016;1006h"[..],
            b"\x1b[?1000;1016;1006h",
            b"\x1b[?1000;1006;1016h",
        ] {
            assert_eq!(scan_all(&[seq]), [Change::Set(true)], "{seq:?}");
        }
    }

    // `?10160h`, `?101h`, `?1016` inside another number must never match; neither do other sequences.
    #[test]
    fn other_modes_and_sequences_do_not_match() {
        for seq in [
            &b"\x1b[?10160h"[..],
            b"\x1b[?101h",
            b"\x1b[?6h",
            b"\x1b[?1006h",
            b"\x1b[?1017l",
            // Not private (no `?`): `CSI 1016 h` is a public mode.
            b"\x1b[1016h",
            // A final byte that is neither h nor l.
            b"\x1b[?1016m",
            b"\x1b[?1016;1006$p\x1b[?25$p",
            // A query of another mode.
            b"\x1b[?2026$p",
            // Huge parameter that would wrap to 1016 in 32-bit arithmetic must not.
            b"\x1b[?4294968312h",
            b"plain 1016 text",
        ] {
            assert!(
                scan_all(&[seq]).is_empty(),
                "{:?}",
                String::from_utf8_lossy(seq)
            );
        }
    }

    // DECRQM asks about one mode per request: a list in a query is not one, and is not answered as 1016.
    #[test]
    fn a_decrqm_asks_for_one_mode() {
        assert_eq!(scan_all(&[b"\x1b[?1016$p"]), [Change::Query]);
        assert!(scan_all(&[b"\x1b[?1000;1016$p"]).is_empty());
    }

    #[test]
    fn a_reset_clears() {
        assert_eq!(scan_all(&[b"\x1b[!p"]), [Change::Reset]);
        assert_eq!(scan_all(&[b"\x1bc"]), [Change::Reset]);
        assert_eq!(
            scan_all(&[b"\x1b[?1016h", b"\x1b[!p", b"\x1b[?1016$p"]),
            [Change::Set(true), Change::Reset, Change::Query]
        );
    }

    // The sequence may be cut anywhere by a pty read boundary.
    #[test]
    fn every_split_point_finds_the_same_changes() {
        let whole = b"abc\x1b[?1000;1016;1006hxyz\x1b[?1016$p\x1b[?1016l\x1b[!p";
        let want = scan_all(&[whole]);
        assert_eq!(
            want,
            [
                Change::Set(true),
                Change::Query,
                Change::Set(false),
                Change::Reset
            ]
        );
        for k in 0..=whole.len() {
            assert_eq!(scan_all(&[&whole[..k], &whole[k..]]), want, "split at {k}");
        }
        // And one byte at a time.
        let bytes: Vec<&[u8]> = whole.chunks(1).collect();
        assert_eq!(scan_all(&bytes), want);
    }

    // A sequence interrupted by a new ESC starts over rather than gluing two sequences together.
    #[test]
    fn an_aborted_sequence_does_not_leak_into_the_next() {
        assert_eq!(scan_all(&[b"\x1b[?10\x1b[?1016h"]), [Change::Set(true)]);
        assert!(scan_all(&[b"\x1b[?10\x1b[6h"]).is_empty());
        assert!(scan_all(&[b"\x1b[?1016\n16h"]).is_empty());
    }
}
