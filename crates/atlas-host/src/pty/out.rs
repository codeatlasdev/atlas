//! A connection's outgoing line queue, bounded in bytes.
//!
//! hostd is the process that must not die (it keeps every agent alive), and a client that
//! stops reading (a phone on a bad network, a paused webview, a suspended process) while a
//! program prints would otherwise make an unbounded queue grow until the OS kills hostd.
//! Producers call [`Out::send`], which never blocks (the pty reader and the other
//! attachers are unaffected): past [`OUT_CAP`] pending bytes it refuses, and the caller
//! drops that connection from what it feeds. The pty ring (2 MiB) already holds what the
//! client missed, so it reattaches with `replay: true` and continues.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::mpsc;

/// Bytes a connection may have queued and unwritten before it is dropped. Four times
/// the ring: a healthy client on a slow link rides out a burst, a stuck one is cut.
pub const OUT_CAP: usize = 8 * 1024 * 1024;

/// Why [`Out::send`] refused a line.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The reader is gone (connection closed).
    Closed,
    /// Over the cap: the client does not read fast enough.
    Full,
}

/// Producer half; cheap to clone (every clone charges the same counter).
#[derive(Clone)]
pub struct Out {
    tx: mpsc::UnboundedSender<String>,
    pending: Arc<AtomicUsize>,
    cap: usize,
}

/// Consumer half, drained by the connection's writer task.
pub struct OutRx {
    rx: mpsc::UnboundedReceiver<String>,
    pending: Arc<AtomicUsize>,
}

/// A queue for one connection holding at most [`OUT_CAP`] pending bytes.
pub fn channel() -> (Out, OutRx) {
    with_cap(OUT_CAP)
}

pub fn with_cap(cap: usize) -> (Out, OutRx) {
    let (tx, rx) = mpsc::unbounded_channel();
    let pending = Arc::new(AtomicUsize::new(0));
    (
        Out {
            tx,
            pending: pending.clone(),
            cap,
        },
        OutRx { rx, pending },
    )
}

impl Out {
    /// Queue `line` unless the connection is over its cap (or gone). Never blocks.
    pub fn send(&self, line: String) -> Result<(), Refused> {
        if self.pending.load(Ordering::Relaxed) >= self.cap {
            return Err(Refused::Full);
        }
        self.force(line)
    }

    /// Queue `line` past the cap: replies to the client's own requests and the notice that
    /// it was dropped. These are small and bounded by the client's request rate.
    pub fn force(&self, line: String) -> Result<(), Refused> {
        let n = line.len();
        self.pending.fetch_add(n, Ordering::Relaxed);
        self.tx.send(line).map_err(|_| {
            self.pending.fetch_sub(n, Ordering::Relaxed);
            Refused::Closed
        })
    }

    /// Bytes queued and not yet taken by the writer.
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

impl OutRx {
    pub async fn recv(&mut self) -> Option<String> {
        let line = self.rx.recv().await?;
        self.pending.fetch_sub(line.len(), Ordering::Relaxed);
        Some(line)
    }

    /// Without waiting (tests, drains).
    pub fn try_recv(&mut self) -> Option<String> {
        let line = self.rx.try_recv().ok()?;
        self.pending.fetch_sub(line.len(), Ordering::Relaxed);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_past_the_cap_and_frees_room_as_the_reader_drains() {
        let (out, mut rx) = with_cap(10);
        assert_eq!(out.send("12345".into()), Ok(()));
        // Under the cap when the line is accepted, so the last line may cross it.
        assert_eq!(out.send("12345".into()), Ok(()));
        assert_eq!(out.pending(), 10);
        assert_eq!(out.send("x".into()), Err(Refused::Full));
        assert_eq!(out.pending(), 10, "a refused line is not charged");
        assert_eq!(rx.try_recv().as_deref(), Some("12345"));
        assert_eq!(out.pending(), 5);
        assert_eq!(out.send("x".into()), Ok(()));
    }

    #[test]
    fn force_bypasses_the_cap_but_counts() {
        let (out, mut rx) = with_cap(4);
        out.send("abcd".into()).unwrap();
        assert_eq!(out.send("e".into()), Err(Refused::Full));
        assert_eq!(out.force("notice".into()), Ok(()));
        assert_eq!(out.pending(), 10);
        assert_eq!(rx.try_recv().as_deref(), Some("abcd"));
        assert_eq!(rx.try_recv().as_deref(), Some("notice"));
        assert_eq!(out.pending(), 0);
    }

    #[test]
    fn a_closed_reader_is_reported_and_not_charged() {
        let (out, rx) = with_cap(100);
        drop(rx);
        assert!(out.is_closed());
        assert_eq!(out.send("abc".into()), Err(Refused::Closed));
        assert_eq!(out.force("abc".into()), Err(Refused::Closed));
        assert_eq!(out.pending(), 0);
    }

    #[test]
    fn clones_share_one_budget() {
        let (a, _rx) = with_cap(6);
        let b = a.clone();
        a.send("1234".into()).unwrap();
        b.send("56".into()).unwrap();
        assert_eq!(a.send("7".into()), Err(Refused::Full));
        assert_eq!(b.pending(), 6);
    }
}
