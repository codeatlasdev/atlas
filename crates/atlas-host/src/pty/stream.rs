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
use super::title::TitleParser;
use super::{Buf, RING_MAX, RawOut, Session, bin_path, open_append};

const RING_DROP: usize = 64 * 1024;
const FILE_MAX: u64 = 4 * 1024 * 1024;
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
        let poll_fd = self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref()?.raw_fd());
        let stop = self.stop.clone();
        std::thread::spawn(move || read_loop(reader, wait, poll_fd, &stop, &tx));
        tokio::spawn(self.clone().coalesce(rx));
    }

    async fn coalesce(self: Arc<Self>, mut rx: mpsc::Receiver<Msg>) {
        let mut title = TitleParser::default();
        let mut activity = Activity::default();
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
            if let Some(t) = title.feed(&pending) {
                self.set_title(t);
            }
            if !pending.is_empty() && activity.due(std::time::Instant::now()) {
                let line = json!({"ev": "pty_activity", "pty": self.id}).to_string();
                self.buf.lock().notify(&line);
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

    fn ack_frozen(&self) {
        if let Some(tx) = self.frozen.lock().take() {
            let _ = tx.send(());
        }
    }

    /// Ring, scrollback file and JSON attachers; returns the raw readers, which the
    /// caller feeds outside the lock (awaiting them is the backpressure).
    fn output(&self, bytes: &[u8]) -> Vec<(u64, RawOut)> {
        let mut b = self.buf.lock();
        b.ring.extend(bytes);
        if b.ring.len() > RING_MAX {
            let over = b.ring.len() - RING_MAX;
            let n = (over.div_ceil(RING_DROP) * RING_DROP).min(b.ring.len());
            b.ring.drain(..n);
        }
        b.file_len += bytes.len() as u64;
        if b.file_len > FILE_MAX {
            let ring: Vec<u8> = b.ring.iter().copied().collect();
            let p = bin_path(&self.dir, &self.id);
            if std::fs::write(&p, &ring).is_ok() {
                b.file = open_append(&p);
                b.file_len = ring.len() as u64;
            }
        } else if let Some(f) = b.file.as_mut() {
            let _ = f.write_all(bytes);
        }
        let raw = b.raw.iter().map(|(k, v)| (*k, v.clone())).collect();
        // Only raw readers (GPU terminal): skip the JSON + base64 encode.
        if !b.attachers.is_empty() {
            let line =
                json!({"ev": "pty_output", "pty": self.id, "data": B64.encode(bytes)}).to_string();
            Buf::broadcast(&mut b.attachers, &line);
        }
        raw
    }

    fn set_title(&self, t: String) {
        *self.title.lock() = t.clone();
        let line = json!({"ev": "pty_title", "pty": self.id, "title": t}).to_string();
        self.buf.lock().notify(&line);
    }

    fn finish(&self, code: Option<i32>) {
        *self.exited.lock() = Some(code);
        self.io.lock().take();
        // Raw streams end with the pty: dropping the senders closes those connections.
        self.buf.lock().raw.clear();
        let line = json!({"ev": "pty_exit", "pty": self.id, "code": code}).to_string();
        self.buf.lock().notify(&line);
    }
}

/// Blocking half of the pipeline: read the pty (or pipe) until EOF/`stop`, then reap.
fn read_loop(
    mut reader: Box<dyn Read + Send>,
    wait: Waiter,
    poll_fd: Option<std::os::fd::RawFd>,
    stop: &std::sync::atomic::AtomicBool,
    tx: &mpsc::Sender<Msg>,
) {
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        if stop.load(Ordering::Acquire) {
            let _ = tx.blocking_send(Msg::Frozen);
            return;
        }
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
