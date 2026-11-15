//! Output pipeline: reader thread → bounded channel → coalescer task, plus the
//! sinks it drives (ring/file append, title, exit).

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::json;
use tokio::sync::mpsc;

use super::master::Waiter;
use super::title::{TitleParser, shown};
use super::{Buf, RING_MAX, RawOut, Session, bin_path, open_append};

const RING_DROP: usize = 64 * 1024;
pub(super) const FILE_MAX: u64 = 4 * 1024 * 1024;
const COALESCE: Duration = Duration::from_millis(8);
const COALESCE_BYTES: usize = 64 * 1024;
/// A raw reader that accepts nothing for this long is dropped (hung client): the
/// program must not freeze behind it.
const RAW_STALL: Duration = Duration::from_secs(2);
/// At most one `pty_activity` per interval while a program prints (#56: working/idle of agents without hooks).
pub(super) const ACTIVITY_EVERY: Duration = Duration::from_secs(1);

/// Throttle of `pty_activity`, one per coalescer.
#[derive(Default)]
pub(super) struct Activity {
    last: Option<std::time::Instant>,
}

impl Activity {
    /// True when output at `now` should be reported.
    pub(super) fn due(&mut self, now: std::time::Instant) -> bool {
        if self.last.is_some_and(|l| now < l + ACTIVITY_EVERY) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

enum Msg {
    Data(Vec<u8>),
    Exit(Option<i32>),
    /// `freeze` asked the reader to stop; everything before this is flushed.
    Frozen,
}

impl Session {
    /// Reader thread → bounded channel → coalescer task. `wait` blocks until the child
    /// is gone (after the reader hits EOF) and yields its code.
    pub(super) fn run(self: &Arc<Self>, reader: Box<dyn Read + Send>, wait: Waiter) {
        // Bounded (64 × 16 KiB): with a slow raw reader the thread blocks, the kernel
        // pty buffer fills, and the program waits.
        let (tx, rx) = mpsc::channel::<Msg>(64);
        // Pollable pty master (not pipe mode): lets `freeze` interrupt a blocked read.
        #[cfg(unix)]
        let poll_fd = self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref()?.raw_fd());
        #[cfg(windows)]
        let (poll_fd, wait) = (None, self.close_console_on_exit(wait));
        let stop = self.stop.clone();
        std::thread::spawn(move || read_loop(reader, wait, poll_fd, &stop, &tx));
        tokio::spawn(self.clone().coalesce(rx));
    }

    async fn coalesce(self: Arc<Self>, mut rx: mpsc::Receiver<Msg>) {
        let mut title = TitleParser::default();
        let mut activity = Activity::default();
        let mut startup_query = cfg!(windows)
            && self
                .io
                .lock()
                .as_ref()
                .is_some_and(|io| io.master.is_some());
        while let Some(msg) = rx.recv().await {
            let mut pending = match msg {
                Msg::Data(d) => d,
                Msg::Exit(code) => {
                    self.finish(code);
                    return;
                }
                Msg::Frozen => {
                    self.ack_frozen();
                    return;
                }
            };
            let deadline = tokio::time::Instant::now() + COALESCE;
            let mut exit = None;
            let mut frozen = false;
            while pending.len() < COALESCE_BYTES {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(Msg::Data(d))) => pending.extend_from_slice(&d),
                    Ok(Some(Msg::Exit(code))) => {
                        exit = Some(code);
                        break;
                    }
                    Ok(Some(Msg::Frozen)) => {
                        frozen = true;
                        break;
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            if startup_query {
                startup_query = false;
                self.answer_startup_query(&mut pending);
            }
            if let Some(t) = title.feed(&pending) {
                self.set_title(shown(t));
            }
            if !pending.is_empty() && activity.due(std::time::Instant::now()) {
                let line = json!({"ev": "pty_activity", "pty": self.id}).to_string();
                self.buf.lock().notify(&self.id, &line);
            }
            for (conn, out) in self.output(&pending) {
                if !send_raw(&out, &pending).await {
                    self.buf.lock().raw.remove(&conn);
                }
            }
            if let Some(code) = exit {
                self.finish(code);
                return;
            }
            if frozen {
                self.ack_frozen();
                return;
            }
        }
    }

    /// ConPTY opens by asking the terminal where the cursor is (`ESC [ 6 n`) and holds the
    /// program until it hears back. hostd answers for the fresh console (row 1, column 1) and
    /// keeps the query from the terminals: a session nobody watches yet still starts, and a
    /// watching terminal does not answer a second time (its reply would reach the shell as
    /// typed text). Only the first output is looked at; later queries belong to programs.
    fn answer_startup_query(&self, first: &mut Vec<u8>) {
        const DSR: &[u8] = b"[6n";
        if let Some(i) = first.windows(DSR.len()).position(|w| w == DSR) {
            first.drain(i..i + DSR.len());
            let _ = self.write(b"[1;1R");
        }
    }

    fn ack_frozen(&self) {
        if let Some(tx) = self.frozen.lock().take() {
            let _ = tx.send(());
        }
    }

    /// Ring, scrollback file and JSON attachers; returns the raw readers, which the
    /// caller feeds outside the lock (awaiting them is the backpressure).
    pub(super) fn output(self: &Arc<Self>, bytes: &[u8]) -> Vec<(u64, RawOut)> {
        let mut b = self.buf.lock();
        b.ring.extend(bytes);
        if b.ring.len() > RING_MAX {
            let over = b.ring.len() - RING_MAX;
            let n = (over.div_ceil(RING_DROP) * RING_DROP).min(b.ring.len());
            b.ring.drain(..n);
        }
        b.file_len += bytes.len() as u64;
        if let Some(since) = b.trim.as_mut() {
            since.extend_from_slice(bytes);
        } else if b.file_len > FILE_MAX {
            // The rewrite is a 2 MiB write and an fsync: never under the lock that every
            // attach and every chunk of output waits on (#301).
            let (snapshot, epoch) = (b.ring_bytes(), b.epoch);
            b.trim = Some(Vec::new());
            let s = self.clone();
            std::thread::spawn(move || s.rewrite_scrollback(&snapshot, epoch));
        } else if let Some(f) = b.file.as_mut() {
            let _ = f.write_all(bytes);
        }
        let raw = b.raw.iter().map(|(k, v)| (*k, v.clone())).collect();
        // Only raw readers (GPU terminal): skip the JSON + base64 encode.
        if !b.attachers.is_empty() {
            let line =
                json!({"ev": "pty_output", "pty": self.id, "data": B64.encode(bytes)}).to_string();
            Buf::broadcast(&self.id, &mut b.attachers, &line);
        }
        raw
    }

    /// Replace the scrollback file with `snapshot` (the ring when the file passed `FILE_MAX`), then
    /// append what arrived meanwhile. Runs on its own thread; the lock is taken only to finish. The
    /// write is atomic: a crash or a failure (full disk) leaves the previous `.bin`, which keeps
    /// growing until a later chunk tries again.
    pub(super) fn rewrite_scrollback(&self, snapshot: &[u8], epoch: u64) {
        let p = bin_path(&self.dir, &self.id);
        let written = atlas_core::fs::write_atomic(&p, snapshot, false);
        let mut b = self.buf.lock();
        let since = b.trim.take().unwrap_or_default();
        let cleared = b.epoch != epoch;
        if written.is_ok() {
            if cleared {
                // `clear` emptied the old file; the rename just brought its content back.
                let _ = std::fs::write(&p, b"");
            }
            b.file = open_append(&p);
        }
        if let Some(f) = b.file.as_mut() {
            let _ = f.write_all(&since);
        }
        let kept = if cleared { 0 } else { snapshot.len() as u64 };
        b.file_len = match written {
            Ok(()) => kept + since.len() as u64,
            // The old file is still the file: try again after another `FILE_MAX / 2` of output.
            Err(_) if cleared => since.len() as u64,
            Err(_) => FILE_MAX / 2 + since.len() as u64,
        };
    }

    /// Store the program's title and tell every attached client, unless it is the title already
    /// shown: omp redraws its spinner title on every frame, mostly with the text it had (#110).
    fn set_title(&self, t: String) {
        {
            let mut cur = self.title.lock();
            if *cur == t {
                return;
            }
            cur.clone_from(&t);
        }
        let line = json!({"ev": "pty_title", "pty": self.id, "title": t}).to_string();
        self.buf.lock().notify(&self.id, &line);
    }

    /// ConPTY keeps the output pipe open after the child exits until the pseudo console is
    /// closed, so the reader would never see EOF. Wait for the child on its own thread, then drop
    /// the master (closing the console flushes the last output and ends the pipe); the reader
    /// collects the code after its EOF.
    #[cfg(windows)]
    fn close_console_on_exit(self: &Arc<Self>, wait: Waiter) -> Waiter {
        if self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref())
            .is_none()
        {
            // Pipe mode: stdout ends with the child.
            return wait;
        }
        let (ctx, crx) = std::sync::mpsc::channel();
        let me = Arc::downgrade(self);
        std::thread::spawn(move || {
            let code = wait();
            let master = me
                .upgrade()
                .and_then(|s| s.io.lock().as_mut().and_then(|io| io.master.take()));
            drop(master);
            let _ = ctx.send(code);
        });
        Box::new(move || crx.recv().ok().flatten())
    }

    fn finish(&self, code: Option<i32>) {
        self.io.lock().take();
        // `exited` is set and announced under the buf lock, the lock `attach` registers under: a listener
        // either is in the map when the exit is sent, or sees `exited` and is told on the spot. Never neither.
        let mut b = self.buf.lock();
        *self.exited.lock() = Some(code);
        // Raw streams end with the pty: dropping the senders closes those connections.
        b.raw.clear();
        b.notify_force(&self.exit_line(code));
        drop(b);
        // The shell is gone: whatever it left behind under this terminal's tag (a backgrounded dev
        // server, a browser that daemonized) has no terminal to answer to.
        self.reap_leftovers();
    }
}

/// Blocking half of the pipeline: read the pty (or pipe) until EOF/`stop`, then reap.
#[cfg(unix)]
type PollFd = std::os::fd::RawFd;
/// No pollable handle on Windows: a read is only interrupted by the console closing.
#[cfg(windows)]
type PollFd = ();

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    wait: Waiter,
    poll_fd: Option<PollFd>,
    stop: &std::sync::atomic::AtomicBool,
    tx: &mpsc::Sender<Msg>,
) {
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        if stop.load(Ordering::Acquire) {
            let _ = tx.blocking_send(Msg::Frozen);
            return;
        }
        #[cfg(unix)]
        if let Some(fd) = poll_fd {
            let mut p = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one valid pollfd; 100 ms bound so `stop` is noticed.
            let r = unsafe { libc::poll(&mut p, 1, 100) };
            if r == 0
                || (r < 0
                    && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted)
            {
                continue;
            }
        }
        #[cfg(windows)]
        let _ = poll_fd;
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.blocking_send(Msg::Data(chunk[..n].to_vec())).is_err() {
                    break;
                }
            }
        }
    }
    let code = wait();
    let _ = tx.blocking_send(Msg::Exit(code));
}

/// Feed one raw reader; `false` when it is gone or stalled past [`RAW_STALL`].
async fn send_raw(out: &RawOut, bytes: &[u8]) -> bool {
    let sent = tokio::time::timeout(RAW_STALL, out.send(bytes.to_vec())).await;
    matches!(sent, Ok(Ok(())))
}
