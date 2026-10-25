//! Connection subscriptions: JSON attach (with replay), listener-only attach, raw
//! byte attach, detach.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use super::{Out, RawOut, Session};

/// Raw bytes per replay line: base64 makes 4/3 of it, well under
/// `MAX_LINE_BYTES` (every client drops longer lines, which lost whole replays).
const REPLAY_CHUNK: usize = 512 * 1024;

/// `data` in pieces of at most `REPLAY_CHUNK`, never cutting a UTF-8 character
/// (a client may decode each piece on its own). Always at least one piece.
fn replay_chunks(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = data;
    let mut first = true;
    std::iter::from_fn(move || {
        if rest.is_empty() && !std::mem::take(&mut first) {
            return None;
        }
        first = false;
        let mut n = rest.len().min(REPLAY_CHUNK);
        // Back off continuation bytes (10xxxxxx) so the cut lands on a char start.
        while n < rest.len() && n > 0 && rest[n] & 0xC0 == 0x80 {
            n -= 1;
        }
        if n == 0 {
            n = rest.len().min(REPLAY_CHUNK);
        }
        let (head, tail) = rest.split_at(n);
        rest = tail;
        Some(head)
    })
}

impl Session {
    fn info(&self) -> Value {
        let (cols, rows) = *self.size.lock();
        json!({
            "cols": cols,
            "rows": rows,
            "title": *self.title.lock(),
            "exited": self.exited().map(|c| json!({"code": c})),
        })
    }

    pub fn attach(&self, conn: u64, out: Out, replay: bool, output: bool) -> Value {
        let mut b = self.buf.lock();
        if !output {
            b.listeners.insert(conn, out);
            drop(b);
            return self.info();
        }
        if replay {
            let all = b.ring_bytes();
            for (i, part) in replay_chunks(&all).enumerate() {
                // The first piece is the reset (clients clear on pty_replay); the
                // rest is plain output, so the screen is rebuilt once, not per piece.
                let ev = if i == 0 { "pty_replay" } else { "pty_output" };
                let _ = out.force(
                    json!({"ev": ev, "pty": self.id, "data": B64.encode(part)}).to_string(),
                );
            }
        }
        b.attachers.insert(conn, out);
        drop(b);
        self.info()
    }

    /// Raw byte subscription. An exited pty gets its replay and then a closed stream.
    pub fn attach_raw(&self, conn: u64, out: RawOut, replay: bool) -> Value {
        let mut b = self.buf.lock();
        if replay {
            let _ = out.try_send(b.ring_bytes());
        }
        // Under the buf lock: `finish` clears `raw` under the same lock, so no sender outlives the pty.
        if self.exited().is_none() {
            b.raw.insert(conn, out);
        }
        drop(b);
        self.info()
    }

    pub fn detach(&self, conn: u64) {
        let mut b = self.buf.lock();
        b.attachers.remove(&conn);
        b.listeners.remove(&conn);
        b.raw.remove(&conn);
    }
}

#[cfg(test)]
mod tests {
    use super::{REPLAY_CHUNK, replay_chunks};

    #[test]
    fn replay_pieces_never_cut_a_character_and_rejoin_exactly() {
        // "é" (C3 A9) straddles the chunk boundary.
        let mut data = vec![b'a'; REPLAY_CHUNK - 1];
        data.extend_from_slice("é".as_bytes());
        data.extend(vec![b'b'; REPLAY_CHUNK]);
        let parts: Vec<&[u8]> = replay_chunks(&data).collect();
        assert!(parts.iter().all(|p| p.len() <= REPLAY_CHUNK));
        assert!(parts.iter().all(|p| std::str::from_utf8(p).is_ok()));
        assert_eq!(parts.concat(), data);
    }

    #[test]
    fn an_empty_history_still_sends_one_reset() {
        assert_eq!(replay_chunks(&[]).collect::<Vec<_>>(), vec![&[] as &[u8]]);
    }
}
