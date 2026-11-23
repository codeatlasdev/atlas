//! Terminal output to the webview: raw bytes in frames over a Tauri channel, with flow control.
//!
//! Output used to travel hostd → JSON + base64 → this process → batch per 16 ms → base64 + JSON → `emit_to` (a script
//! evaluated in the webview per event) → `atob` in the page. A busy terminal cost 380 % of a core across hostd, this
//! process and WebKit's web process, and the web process grew from 205 to 1,153 MB, because nothing stopped the
//! sender: every event was queued in WebKit however slowly xterm.js parsed (docs/perf/linux-webkit-study.md, cause 3).
//!
//! Now each viewed pty has a dedicated `pty_attach_raw` connection (hostd writes the bytes as they are, no JSON, no
//! base64) read by [`feed`], which hands them to the page as binary frames (`InvokeResponseBody::Raw`, an `ArrayBuffer`
//! there; `apps/desktop/src/lib/ptyChannel.ts` decodes them). The page acknowledges the bytes xterm has parsed
//! ([`Credit::ack`]); at most [`WINDOW`] bytes are ever unacknowledged. Past that the feed stops reading the socket, so
//! hostd's bounded raw queue fills and the program waits, as in a native terminal; hostd drops a reader that does not
//! read for `RAW_STALL` and the page then asks for a fresh replay (a [`DROPPED`] frame). Nothing is ever skipped
//! silently: a byte is either delivered in order or the view is rebuilt from hostd's history.
//!
//! Frame: one kind byte, then payload ([`DATA`], [`REPLAY`]) or nothing ([`DROPPED`], [`ENDED`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::FutureExt;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Notify;
use tokio::time::Instant;

/// Output bytes: append to the terminal.
pub const DATA: u8 = 0;
/// Output bytes after a reset: the history hostd replays when a view attaches. Empty when there is none.
pub const REPLAY: u8 = 1;
/// The raw stream closed while the program still runs (hostd dropped a reader that stalled): attach again.
pub const DROPPED: u8 = 2;
/// The program ended (or its host is gone): the view keeps its last screen.
pub const ENDED: u8 = 3;

const HEADER: usize = 1;
/// Payload bytes of one frame at most: 4 frames fill the window, and xterm.js parses a frame in a few 12 ms slices.
pub const FRAME_BYTES: usize = 512 * 1024;
/// Bytes sent to the page and not yet parsed by xterm.js, at most. Memory held for a view is bounded by this plus one
/// frame here and hostd's raw queue behind it, however fast the program prints.
pub const WINDOW: usize = 4 * FRAME_BYTES;
/// Frames of one pty are at least this far apart (about one display frame): a program printing in many small writes
/// becomes one frame per frame instead of one script/fetch round trip per hostd chunk (which froze WebKitGTK with two
/// busy terminals, 2026-10-26). A frame after a quiet moment goes out at once.
const FRAME_EVERY: Duration = Duration::from_millis(16);

/// A full window with no acknowledgement for this long: nobody is reading (a reloaded page, a webview that died).
/// The stream ends with [`DROPPED`] instead of holding hostd's connection and its memory for ever; a page that is
/// alive redraws from hostd's history.
const ABANDONED: Duration = Duration::from_secs(60);

/// Where frames go: the page's channel, or a test's vector.
pub trait FrameSink {
    /// `false` when nobody receives frames any more (the webview is gone).
    fn send(&self, frame: Vec<u8>) -> bool;
}

/// The flow-control balance of one stream: what the page has been sent and has not yet parsed.
#[derive(Default)]
pub struct Credit {
    inflight: AtomicUsize,
    acked: Notify,
}

impl Credit {
    pub fn spend(&self, n: usize) {
        self.inflight.fetch_add(n, Ordering::Relaxed);
    }

    /// The page parsed `n` bytes of what it was sent.
    pub fn ack(&self, n: usize) {
        let _ = self
            .inflight
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                Some(c.saturating_sub(n))
            });
        self.acked.notify_one();
    }

    pub fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Relaxed)
    }

    /// Resolves `true` once fewer than [`WINDOW`] bytes are unacknowledged; `false` when [`ABANDONED`] passed with
    /// no acknowledgement at all (the page is gone, or never read what it was sent).
    async fn room(&self) -> bool {
        let deadline = Instant::now() + ABANDONED;
        loop {
            // Register before looking, so an ack in between is not missed.
            let woken = self.acked.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            if self.inflight() < WINDOW {
                return true;
            }
            if tokio::time::timeout_at(deadline, woken).await.is_err() {
                return false;
            }
        }
    }
}

/// Why a raw stream ended.
pub enum Ending {
    /// The program exited, or the host (or the pty) is gone: [`ENDED`].
    Exited,
    /// The program runs on: hostd cut this reader, [`DROPPED`].
    Dropped,
}

/// Read `raw` until it ends and send what it yields to `sink` as frames. `replayed` is the number of bytes at the head
/// of `raw` that are hostd's history; with none, the reset frame goes out at once (an empty history still resets the
/// view). `ending` says why the stream closed.
pub async fn feed<R, S>(
    mut raw: R,
    replayed: usize,
    sink: &S,
    credit: &Credit,
    ending: impl Future<Output = Ending>,
) where
    R: AsyncRead + Unpin,
    S: FrameSink,
{
    let mut kind = REPLAY;
    if replayed == 0 {
        if !sink.send(vec![REPLAY]) {
            return;
        }
        kind = DATA;
    }
    // Read into one buffer per stream; a frame is a right-sized copy, so a quiet terminal's frames stay small.
    let mut buf = vec![0u8; FRAME_BYTES];
    let mut next = Instant::now();
    let mut eof = false;
    let mut drop_stream = false;
    while !eof {
        tokio::time::sleep_until(next).await;
        if !credit.room().await {
            drop_stream = true;
            break;
        }
        let mut n = match raw.read(&mut buf).await {
            Ok(n @ 1..) => n,
            _ => break,
        };
        // Whatever else is already waiting goes in the same frame.
        while n < buf.len() {
            match raw.read(&mut buf[n..]).now_or_never() {
                Some(Ok(m @ 1..)) => n += m,
                Some(_) => {
                    eof = true;
                    break;
                }
                None => break,
            }
        }
        let mut frame = Vec::with_capacity(HEADER + n);
        frame.push(kind);
        frame.extend_from_slice(&buf[..n]);
        credit.spend(frame.len() - HEADER);
        if !sink.send(frame) {
            return;
        }
        kind = DATA;
        next = Instant::now() + FRAME_EVERY;
    }
    let last = match if drop_stream {
        Ending::Dropped
    } else {
        ending.await
    } {
        Ending::Exited => ENDED,
        Ending::Dropped => DROPPED,
    };
    sink.send(vec![last]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::AsyncWriteExt;

    #[derive(Default)]
    struct Frames(Mutex<Vec<Vec<u8>>>);

    impl FrameSink for Arc<Frames> {
        fn send(&self, frame: Vec<u8>) -> bool {
            self.0.lock().push(frame);
            true
        }
    }

    impl Frames {
        fn take(&self) -> Vec<Vec<u8>> {
            std::mem::take(&mut self.0.lock())
        }
    }

    /// A program that never stops printing: every read is satisfied at once; `produced` counts what was read.
    struct Flood(Arc<AtomicUsize>);

    impl AsyncRead for Flood {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let n = buf.remaining();
            buf.put_slice(&vec![b'x'; n]);
            self.0.fetch_add(n, Ordering::Relaxed);
            std::task::Poll::Ready(Ok(()))
        }
    }

    fn payload(frames: &[Vec<u8>]) -> Vec<u8> {
        frames.iter().flat_map(|f| f[HEADER..].to_vec()).collect()
    }

    // Incident (linux-webkit-study, cause 3): a program printing flat out grew the web process from 205 to 1,153 MB
    // because every event was queued in WebKit however slowly xterm.js parsed. A page that acknowledges nothing must
    // stop being fed after one window, and the stream behind it must stop being read.
    #[tokio::test(start_paused = true)]
    async fn a_flood_with_a_page_that_does_not_ack_stops_at_one_window() {
        let produced = Arc::new(AtomicUsize::new(0));
        let frames = Arc::new(Frames::default());
        let credit = Arc::new(Credit::default());
        let task = {
            let (p, f, c) = (produced.clone(), frames.clone(), credit.clone());
            tokio::spawn(async move { feed(Flood(p), 0, &f, &c, async { Ending::Exited }).await })
        };
        tokio::time::sleep(Duration::from_secs(5)).await;
        let sent: usize = frames.0.lock().iter().map(|f| f.len() - HEADER).sum();
        assert!(
            (WINDOW..WINDOW + FRAME_BYTES).contains(&sent),
            "{sent} bytes sent to a page that acknowledged none (window {WINDOW})"
        );
        assert!(
            produced.load(Ordering::Relaxed) < WINDOW + 2 * FRAME_BYTES,
            "the stream was read on after the window was full: {} bytes",
            produced.load(Ordering::Relaxed)
        );

        // The page catches up: the flood resumes, and again stops one window later.
        credit.ack(credit.inflight());
        tokio::time::sleep(Duration::from_secs(5)).await;
        let sent_after: usize = frames.0.lock().iter().map(|f| f.len() - HEADER).sum();
        assert!(
            (2 * WINDOW..2 * WINDOW + FRAME_BYTES).contains(&sent_after),
            "{sent_after} bytes after one ack of the whole window"
        );
        task.abort();
    }

    // A page that parses as fast as it is fed never waits: ten windows of output arrive complete and in order.
    #[tokio::test(start_paused = true)]
    async fn a_page_that_keeps_up_gets_every_byte_in_order() {
        let (mut w, r) = tokio::io::duplex(64 * 1024);
        let frames = Arc::new(Frames::default());
        let credit = Arc::new(Credit::default());
        let (f, c) = (frames.clone(), credit.clone());
        let task = tokio::spawn(async move { feed(r, 0, &f, &c, async { Ending::Exited }).await });
        let total = 10 * WINDOW;
        let want: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let mut got = Vec::new();
        let writer = tokio::spawn({
            let want = want.clone();
            async move {
                w.write_all(&want).await.unwrap();
            }
        });
        while got.len() < total {
            tokio::time::sleep(Duration::from_millis(5)).await;
            for f in frames.take() {
                if f[0] == DATA {
                    credit.ack(f.len() - HEADER);
                    got.extend_from_slice(&f[HEADER..]);
                }
            }
        }
        writer.await.unwrap();
        assert!(got == want, "bytes differ or were reordered");
        task.abort();
    }

    // The history is flagged once, on the first frame, whether or not live output is already behind it.
    #[tokio::test(start_paused = true)]
    async fn the_first_frame_resets_the_view_and_later_ones_append() {
        let (mut w, r) = tokio::io::duplex(1024 * 1024);
        w.write_all(b"HISTORY").await.unwrap();
        let frames = Arc::new(Frames::default());
        let credit = Arc::new(Credit::default());
        let (f, c) = (frames.clone(), credit.clone());
        let task = tokio::spawn(async move { feed(r, 7, &f, &c, async { Ending::Exited }).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        w.write_all(b"+live").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let got = frames.take();
        assert_eq!(got[0][0], REPLAY);
        assert!(got[1..].iter().all(|f| f[0] == DATA));
        assert_eq!(payload(&got), b"HISTORY+live");
        task.abort();
    }

    // An empty history still resets the view (its terminal waits for the reset before it shows anything), and it
    // does so without waiting for a byte.
    #[tokio::test(start_paused = true)]
    async fn an_empty_history_resets_the_view_at_once() {
        let (_w, r) = tokio::io::duplex(1024);
        let frames = Arc::new(Frames::default());
        let credit = Arc::new(Credit::default());
        let (f, c) = (frames.clone(), credit.clone());
        let task = tokio::spawn(async move { feed(r, 0, &f, &c, async { Ending::Exited }).await });
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(frames.take(), vec![vec![REPLAY]]);
        task.abort();
    }

    // Many small writes within one display frame leave as one frame, and a write after a quiet moment is not held.
    #[tokio::test(start_paused = true)]
    async fn small_writes_in_one_display_frame_share_a_frame_and_a_quiet_one_goes_out_at_once() {
        let (mut w, r) = tokio::io::duplex(1024 * 1024);
        let frames = Arc::new(Frames::default());
        let credit = Arc::new(Credit::default());
        let (f, c) = (frames.clone(), credit.clone());
        let task = tokio::spawn(async move { feed(r, 0, &f, &c, async { Ending::Exited }).await });
        tokio::time::sleep(Duration::from_secs(1)).await;
        frames.take();
        w.write_all(b"a").await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(frames.take(), vec![vec![DATA, b'a']], "quiet: sent at once");
        for b in b"bcdefg" {
            w.write_all(&[*b]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(payload(&frames.take()), b"bcdefg");
        task.abort();
    }

    // The two ways a stream ends are told apart: a program that ended keeps its screen, a reader hostd cut reattaches.
    #[tokio::test(start_paused = true)]
    async fn the_end_of_the_stream_is_reported_as_the_caller_classifies_it() {
        for (ending, want) in [(Ending::Exited, ENDED), (Ending::Dropped, DROPPED)] {
            let (mut w, r) = tokio::io::duplex(1024);
            w.write_all(b"bye").await.unwrap();
            drop(w);
            let frames = Arc::new(Frames::default());
            let credit = Credit::default();
            feed(r, 0, &frames, &credit, async { ending }).await;
            let got = frames.take();
            assert_eq!(got.last().unwrap(), &vec![want]);
            assert_eq!(payload(&got[..got.len() - 1]), b"bye");
        }
    }

    // A page that is gone (a reloaded webview) never acknowledges: the stream must not hold hostd's connection and
    // its window of memory for ever.
    #[tokio::test(start_paused = true)]
    async fn a_page_that_never_acks_is_given_up_on_and_told_to_redraw() {
        let produced = Arc::new(AtomicUsize::new(0));
        let frames = Arc::new(Frames::default());
        let credit = Credit::default();
        let done = feed(Flood(produced), 0, &frames, &credit, async {
            Ending::Exited
        });
        tokio::time::timeout(2 * ABANDONED, done)
            .await
            .expect("the stream outlived an abandoned page");
        assert_eq!(frames.take().last().unwrap(), &vec![DROPPED]);
    }

    // Contract with `apps/desktop/src/lib/ptyChannel.ts`: the frames this module really produces, written to
    // `__fixtures__/pty-frames.json` (hex), are what `ptyChannel.test.ts` decodes with the page's own decoder.
    // `UPDATE_FIXTURES=1 cargo test -p atlas-desktop pty_frames_fixture` rewrites it.
    #[tokio::test(start_paused = true)]
    async fn pty_frames_fixture_is_what_rust_sends() {
        async fn run(history: &[u8], live: &[u8], ending: Ending) -> Vec<Vec<u8>> {
            let (mut w, r) = tokio::io::duplex(64 * 1024);
            w.write_all(history).await.unwrap();
            w.write_all(live).await.unwrap();
            drop(w);
            let frames = Arc::new(Frames::default());
            feed(r, history.len(), &frames, &Credit::default(), async {
                ending
            })
            .await;
            frames.take()
        }
        let hex = |frames: Vec<Vec<u8>>| -> Vec<String> {
            frames
                .iter()
                .map(|f| f.iter().map(|b| format!("{b:02x}")).collect())
                .collect()
        };
        // UTF-8 and a bare escape sequence: bytes, not text, must survive.
        let history = "PS> é\u{1b}[31m".as_bytes();
        let fixture = serde_json::json!({
            "history_then_live_then_exit": hex(run(history, b"dir\r\n", Ending::Exited).await),
            "empty_history_then_dropped": hex(run(b"", b"x", Ending::Dropped).await),
            "history_utf8": String::from_utf8(history.to_vec()).unwrap(),
        });
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&fixture, &mut ser).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/pty-frames.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("pty-frames.json (UPDATE_FIXTURES=1 writes it)"),
        )
        .unwrap();
        assert_eq!(
            committed, fixture,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
