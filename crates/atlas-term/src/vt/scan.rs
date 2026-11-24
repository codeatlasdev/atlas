//! Streaming finder for the string sequences alacritty ignores: OSC 133/7 (prompt marks,
//! cwd), OSC 9/777 (desktop notifications), APC `G` (Kitty graphics) and DCS sixel.
//! Split-safe across `feed` chunks.

/// A string sequence alacritty ignores, picked out of the stream.
#[derive(Debug, PartialEq)]
pub(super) enum Seq {
    /// OSC 133 / 7 / 9 / 777 payload.
    Osc(Vec<u8>),
    /// APC `G…` (Kitty graphics), without the `G`.
    Kitty(Vec<u8>),
    /// DCS sixel (`P1;P2;P3 q …`).
    Sixel(Vec<u8>),
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Kind {
    #[default]
    Osc,
    Apc,
    Dcs,
}

/// Streaming finder for OSC (BEL/ST), APC and DCS (ST) strings, split-safe.
/// Whole APC/DCS bodies are consumed here, so an `ESC ]` inside one is not an OSC.
#[derive(Default)]
pub(super) struct SeqScan {
    /// 0 text, 1 after ESC, 2 in a string, 3 ESC inside a string.
    st: u8,
    kind: Kind,
    buf: Vec<u8>,
    /// The string is not one we want (or overflowed): skip to its end.
    skip: bool,
}

impl SeqScan {
    const MAX_OSC: usize = 4096;
    const MAX_BODY: usize = 64 << 20;

    fn start(&mut self, kind: Kind) -> u8 {
        self.kind = kind;
        self.buf.clear();
        self.skip = false;
        2
    }

    /// `(offset just past the terminator, sequence)` for each wanted string in `bytes`.
    pub(super) fn scan(&mut self, bytes: &[u8]) -> Vec<(usize, Seq)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            // Plain text: jump to the next ESC.
            if self.st == 0 {
                match memchr::memchr(0x1b, &bytes[i..]) {
                    Some(k) => i += k,
                    None => break,
                }
            }
            let b = bytes[i];
            self.st = match (self.st, b) {
                (0 | 1, 0x1b) => 1,
                (1, b']') => self.start(Kind::Osc),
                (1, b'_') => self.start(Kind::Apc),
                (1, b'P') => self.start(Kind::Dcs),
                (2, 0x07) | (3, b'\\') if self.st == 3 || self.kind == Kind::Osc => {
                    if !self.skip
                        && let Some(seq) = self.finish()
                    {
                        out.push((i + 1, seq));
                    }
                    0
                }
                (2 | 3, 0x1b) => 3,
                (2, _) => {
                    let max = match self.kind {
                        Kind::Osc => Self::MAX_OSC,
                        _ => Self::MAX_BODY,
                    };
                    if self.skip {
                    } else if self.buf.len() < max {
                        self.buf.push(b);
                    } else {
                        self.skip = true;
                        self.buf = Vec::new();
                    }
                    2
                }
                // CAN/SUB or an unrelated escape abort the string.
                _ => 0,
            };
            i += 1;
        }
        out
    }

    fn finish(&mut self) -> Option<Seq> {
        let buf = std::mem::take(&mut self.buf);
        match self.kind {
            Kind::Osc => {
                const WANTED: [&[u8]; 4] = [b"133;", b"7;", b"9;", b"777;"];
                WANTED
                    .iter()
                    .any(|p| buf.starts_with(p))
                    .then_some(Seq::Osc(buf))
            }
            Kind::Apc => buf.strip_prefix(b"G").map(|b| Seq::Kitty(b.to_vec())),
            // Sixel: numeric params, then `q` (no intermediates: `$q` is DECRQSS).
            Kind::Dcs => {
                let q = buf
                    .iter()
                    .position(|&c| !(c.is_ascii_digit() || c == b';'))?;
                (buf[q] == b'q').then_some(Seq::Sixel(buf))
            }
        }
    }
}

/// Finds `ESC[3J` + `ESC[H` + `ESC[2J` (macOS/ncurses `clear`: terminfo `E3` then `clear`), split-safe.
/// ED 3 arrives while the old screen is still on the grid, then alacritty's ED 2 on the primary
/// screen scrolls that screen into history instead of erasing it, so the old prompt stays one
/// scroll away (and the scrollbar shows). The match point is just past `ESC[2J`.
#[derive(Default)]
pub(super) struct ClearScan {
    /// Bytes of `PATTERN` matched so far.
    at: usize,
}

impl ClearScan {
    const PATTERN: &'static [u8] = b"\x1b[3J\x1b[H\x1b[2J";

    /// Offsets just past each full pattern in `bytes`.
    pub(super) fn scan(&mut self, bytes: &[u8]) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            // Outside a match only an ESC can start one: jump to it instead of testing every byte.
            if self.at == 0 {
                match memchr::memchr(0x1b, &bytes[i..]) {
                    Some(k) => i += k,
                    None => break,
                }
            }
            let b = bytes[i];
            if b == Self::PATTERN[self.at] {
                self.at += 1;
            } else {
                // The pattern has no repeated prefix except its leading ESC.
                self.at = usize::from(b == 0x1b);
            }
            if self.at == Self::PATTERN.len() {
                out.push(i + 1);
                self.at = 0;
            }
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::ClearScan;

    #[test]
    fn clear_scan_is_split_safe_and_exact() {
        let whole = b"x\x1b[3J\x1b[H\x1b[2Jy";
        assert_eq!(ClearScan::default().scan(whole), vec![whole.len() - 1]);
        // Every possible split point finds exactly one match.
        for k in 0..=whole.len() {
            let mut s = ClearScan::default();
            let n = s.scan(&whole[..k]).len() + s.scan(&whole[k..]).len();
            assert_eq!(n, 1, "split at {k}");
        }
        // Other erases never match: a shell's ^L / ED 2 alone, ED 3 alone, a restarted ESC.
        for not in [
            &b"\x1b[H\x1b[2J"[..],
            b"\x1b[3J",
            b"\x1b[3J\x1b[2J",
            b"\x1b[3J\x1b[H\x1b[J",
        ] {
            assert!(ClearScan::default().scan(not).is_empty());
        }
        // An aborted attempt right before the real one still matches.
        assert_eq!(
            ClearScan::default()
                .scan(b"\x1b[3\x1b[3J\x1b[H\x1b[2J")
                .len(),
            1
        );
        // Two clears back to back.
        let two = [&whole[1..whole.len() - 1], &whole[1..whole.len() - 1]].concat();
        assert_eq!(ClearScan::default().scan(&two).len(), 2);
    }

    // #252: the scan jumps over plain text to the next ESC; the cut offsets must stay exact (they split
    // the chunk fed to the parser) with text before, between and after the clears, at every split.
    #[test]
    fn clear_offsets_stay_exact_across_skipped_text() {
        let clear = b"\x1b[3J\x1b[H\x1b[2J";
        let text = "ab\u{e9} line\r\n".repeat(40);
        let mut whole = text.clone().into_bytes();
        whole.extend_from_slice(clear);
        let first = whole.len();
        whole.extend_from_slice(text.as_bytes());
        whole.extend_from_slice(b"\x1b[3J\x1b[H\x1b[");
        whole.extend_from_slice(text.as_bytes()); // aborted clear in the middle
        whole.extend_from_slice(clear);
        let second = whole.len();
        whole.extend_from_slice(b"tail");
        assert_eq!(ClearScan::default().scan(&whole), vec![first, second]);
        for k in 0..=whole.len() {
            let mut s = ClearScan::default();
            let mut got = s.scan(&whole[..k]);
            got.extend(s.scan(&whole[k..]).into_iter().map(|o| o + k));
            assert_eq!(got, vec![first, second], "split at {k}");
        }
    }

    // #252: the skipping scan must match the old byte-at-a-time loop, incl. a partial match broken by a
    // non-ESC byte (`ESC[3` then `x`), the one path where no skip happens.
    #[test]
    fn skipping_text_matches_the_byte_wise_scan() {
        fn reference(at: &mut usize, bytes: &[u8]) -> Vec<usize> {
            let p = ClearScan::PATTERN;
            let mut out = Vec::new();
            for (i, &b) in bytes.iter().enumerate() {
                *at = if b == p[*at] {
                    *at + 1
                } else {
                    usize::from(b == 0x1b)
                };
                if *at == p.len() {
                    out.push(i + 1);
                    *at = 0;
                }
            }
            out
        }
        let corpus: &[u8] =
            b"text\x1b[3x\x1b[3J\x1b[H\x1b[2Jmid\x1b\x1b[3J\x1b[H\x1b[2J\x1b[3J\x1b[H\x1b[2J\
            \x1b[3J\x1b[H\x1b[2x\x1b[31mred\x1b[3J\x1b[H\x1b[2Jend\x1b[3J";
        let want = reference(&mut 0, corpus);
        assert_eq!(want.len(), 4);
        for k in 0..=corpus.len() {
            let (mut s, mut at) = (ClearScan::default(), 0);
            for (a, b) in [(0, k), (k, corpus.len())] {
                let got = s.scan(&corpus[a..b]);
                assert_eq!(got, reference(&mut at, &corpus[a..b]), "split at {k}");
                assert_eq!(s.at, at, "split at {k}");
            }
        }
    }
}
