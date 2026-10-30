//! Terminal output to the webview, batched per pty.
//!
//! hostd sends a pty's output in many small chunks (an agent TUI redraws ~30 times a second, each redraw several
//! chunks). Each chunk used to be one `emit`, and on Linux (WebKitGTK) every emit is a separate script evaluation in
//! the webview: one busy terminal made the app lag and two froze it. Output is now gathered per pty and flushed at
//! most once per frame, as one event per pty, in arrival order. A replay (the history after a reset) supersedes
//! whatever of that pty was still pending.

use std::collections::HashMap;

/// One event to send: a pty's bytes, `replay` when they are the history after a reset.
#[derive(Debug, PartialEq)]
pub struct Batch {
    pub pty: String,
    pub bytes: Vec<u8>,
    pub replay: bool,
}

/// Pending output of every pty, flushed by `drain`.
#[derive(Default)]
pub struct PtyBatcher {
    /// Ptys in the order their first pending chunk arrived (stable event order across ptys).
    order: Vec<String>,
    pending: HashMap<String, (Vec<u8>, bool)>,
}

/// Above this many pending bytes for one pty, flush without waiting for the frame (a `cat` of a big file).
pub const FLUSH_BYTES: usize = 256 * 1024;

impl PtyBatcher {
    /// Add output. Returns whether this pty is over `FLUSH_BYTES` and should be flushed now.
    pub fn push(&mut self, pty: &str, bytes: &[u8], replay: bool) -> bool {
        if replay {
            // The replay redraws from scratch: output still pending from before it is already in it, or stale.
            self.pending.remove(pty);
            self.order.retain(|p| p != pty);
        }
        let e = self.pending.entry(pty.to_string()).or_insert_with(|| {
            self.order.push(pty.to_string());
            (Vec::new(), false)
        });
        e.0.extend_from_slice(bytes);
        e.1 |= replay;
        e.0.len() >= FLUSH_BYTES
    }

    /// Everything pending, one batch per pty, in arrival order; the batcher is empty afterwards.
    pub fn drain(&mut self) -> Vec<Batch> {
        let order = std::mem::take(&mut self.order);
        order
            .into_iter()
            .filter_map(|pty| {
                let (bytes, replay) = self.pending.remove(&pty)?;
                Some(Batch { pty, bytes, replay })
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident (2026-10-26, Linux/Arch: "one terminal tab lags, two freeze the app"): one emit per hostd chunk. A
    // frame's worth of chunks must become one event per pty.
    #[test]
    fn many_chunks_of_one_pty_become_one_batch_in_order() {
        let mut b = PtyBatcher::default();
        for i in 0..100 {
            b.push("p1", format!("{i};").as_bytes(), false);
        }
        let out = b.drain();
        assert_eq!(out.len(), 1);
        assert!(out[0].bytes.starts_with(b"0;1;2;") && out[0].bytes.ends_with(b"98;99;"));
        assert!(b.is_empty());
        assert_eq!(b.drain(), vec![]);
    }

    #[test]
    fn two_ptys_keep_their_own_bytes_and_the_order_they_first_spoke() {
        let mut b = PtyBatcher::default();
        b.push("b", b"x", false);
        b.push("a", b"1", false);
        b.push("b", b"y", false);
        let out = b.drain();
        let got: Vec<_> = out
            .iter()
            .map(|x| (x.pty.as_str(), x.bytes.as_slice()))
            .collect();
        assert_eq!(got, vec![("b", &b"xy"[..]), ("a", &b"1"[..])]);
    }

    #[test]
    fn a_replay_supersedes_what_was_pending_and_live_output_after_it_follows_it() {
        let mut b = PtyBatcher::default();
        b.push("p", b"stale", false);
        b.push("p", b"HISTORY", true);
        b.push("p", b"+live", false);
        assert_eq!(
            b.drain(),
            vec![Batch {
                pty: "p".into(),
                bytes: b"HISTORY+live".to_vec(),
                replay: true
            }]
        );
    }

    #[test]
    fn a_big_burst_asks_for_an_early_flush() {
        let mut b = PtyBatcher::default();
        assert!(!b.push("p", &[b'x'; FLUSH_BYTES - 1], false));
        assert!(b.push("p", b"x", false));
    }
}
