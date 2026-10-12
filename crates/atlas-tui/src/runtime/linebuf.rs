//! Raw child output → log lines.
//!
//! A pipe delivers arbitrary byte chunks, not lines. `LineAssembler` turns them
//! into lines the way a terminal would show them:
//! - a partial line is held until its newline arrives (or `flush` is called
//!   after an idle gap — prompts and progress output without a trailing `\n`);
//! - `\r\n` ends a line; a lone `\r` rewinds to the start of the line, so a
//!   progress bar (`10%\r20%\r30%`) collapses to its last segment;
//! - invalid UTF-8 is replaced (never dropped, never a panic);
//! - one line is capped at `MAX_LINE_BYTES` so a minified blob can't blow the
//!   ring; the rest of that line is discarded up to its newline and the line
//!   carries `truncated`.

/// Longest single line kept, in bytes.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

/// Appended to a line cut at `MAX_LINE_BYTES`.
pub const TRUNCATION_MARK: &str = " … [truncated]";

/// One assembled line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    /// Cut at `MAX_LINE_BYTES` (the marker is already appended to `text`).
    pub truncated: bool,
}

#[derive(Debug, Default)]
pub struct LineAssembler {
    /// Bytes of the current line since the last `\r` rewind.
    cur: Vec<u8>,
    /// A `\r` was seen and the next byte decides: `\n` ends the line, anything
    /// else means the rewind happened and `cur` restarts.
    pending_cr: bool,
    /// The current line already hit the cap; drop bytes until its newline.
    overflow: bool,
}

impl LineAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one chunk; every completed line is handed to `emit`.
    pub fn push(&mut self, chunk: &[u8], mut emit: impl FnMut(Line)) {
        for &b in chunk {
            if self.pending_cr {
                self.pending_cr = false;
                if b == b'\n' {
                    self.finish(&mut emit);
                    continue;
                }
                // Lone `\r`: the cursor went back to column 0. What was typed
                // so far is overwritten; keep only what follows.
                self.cur.clear();
                self.overflow = false;
            }
            match b {
                b'\n' => self.finish(&mut emit),
                b'\r' => self.pending_cr = true,
                _ if self.overflow => {}
                _ => {
                    if self.cur.len() >= MAX_LINE_BYTES {
                        self.overflow = true;
                    } else {
                        self.cur.push(b);
                    }
                }
            }
        }
    }

    /// Idle flush: emit the pending partial line (if any) so a prompt or a
    /// progress bar becomes visible. A trailing lone `\r` is kept as a rewind.
    pub fn flush(&mut self, mut emit: impl FnMut(Line)) {
        if self.pending_cr {
            self.pending_cr = false;
        }
        if !self.cur.is_empty() {
            self.finish(&mut emit);
        }
    }

    /// True while a partial line is buffered.
    pub fn has_partial(&self) -> bool {
        !self.cur.is_empty()
    }

    fn finish(&mut self, emit: &mut impl FnMut(Line)) {
        let overflow = std::mem::take(&mut self.overflow);
        let bytes = std::mem::take(&mut self.cur);
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if overflow {
            text.push_str(TRUNCATION_MARK);
        }
        emit(Line {
            text,
            truncated: overflow,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&[u8]]) -> Vec<Line> {
        let mut a = LineAssembler::new();
        let mut out = Vec::new();
        for c in chunks {
            a.push(c, |l| out.push(l));
        }
        out
    }

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn partial_lines_wait_for_their_newline() {
        let mut a = LineAssembler::new();
        let mut out = Vec::new();
        a.push(b"hel", |l| out.push(l));
        assert!(out.is_empty() && a.has_partial());
        a.push(b"lo\nwor", |l| out.push(l));
        assert_eq!(texts(&out), ["hello"]);
        a.push(b"ld\n", |l| out.push(l));
        assert_eq!(texts(&out), ["hello", "world"]);
        assert!(!a.has_partial());
    }

    #[test]
    fn idle_flush_emits_the_partial_once() {
        let mut a = LineAssembler::new();
        let mut out = Vec::new();
        a.push(b"Password: ", |l| out.push(l));
        a.flush(|l| out.push(l));
        a.flush(|l| out.push(l));
        assert_eq!(texts(&out), ["Password: "]);
    }

    #[test]
    fn crlf_is_one_terminator_even_across_chunks() {
        let out = feed(&[b"a\r\nb\r", b"\nc\n"]);
        assert_eq!(texts(&out), ["a", "b", "c"]);
    }

    #[test]
    fn lone_cr_keeps_only_the_last_progress_segment() {
        let out = feed(&[b"10%\r20%\r30%\rdone\n"]);
        assert_eq!(texts(&out), ["done"]);
        // Across chunks, and with CRLF right after a rewind.
        let out = feed(&[b"a\r", b"b\r", b"c\r\nnext\n"]);
        assert_eq!(texts(&out), ["c", "next"]);
    }

    #[test]
    fn invalid_utf8_is_replaced_not_dropped() {
        let out = feed(&[b"ok \xff\xfe bad\n"]);
        assert_eq!(texts(&out), ["ok \u{fffd}\u{fffd} bad"]);
    }

    #[test]
    fn multibyte_split_across_chunks_survives() {
        let s = "héllo 日本\n".as_bytes();
        let (a, b) = s.split_at(2); // splits the é
        assert_eq!(texts(&feed(&[a, b])), ["héllo 日本"]);
    }

    #[test]
    fn huge_line_is_capped_and_the_rest_discarded() {
        let mut big = vec![b'x'; MAX_LINE_BYTES * 3];
        big.extend_from_slice(b"\nnext\n");
        let out = feed(&[&big]);
        assert_eq!(out.len(), 2);
        assert!(out[0].truncated && out[0].text.ends_with(TRUNCATION_MARK));
        assert_eq!(out[0].text.len(), MAX_LINE_BYTES + TRUNCATION_MARK.len());
        assert_eq!(out[1].text, "next");
        assert!(!out[1].truncated);
    }

    #[test]
    fn rewind_after_overflow_starts_a_fresh_line() {
        let mut big = vec![b'x'; MAX_LINE_BYTES + 10];
        big.extend_from_slice(b"\rok\n");
        let out = feed(&[&big]);
        assert_eq!(texts(&out), ["ok"]);
        assert!(!out[0].truncated);
    }

    #[test]
    fn empty_lines_are_kept() {
        assert_eq!(texts(&feed(&[b"a\n\nb\n"])), ["a", "", "b"]);
    }
}
