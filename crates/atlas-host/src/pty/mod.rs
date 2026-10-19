//! One PTY session: child process, ring buffer, scrollback file, OSC title
//! tracking and the set of attached connections receiving `pty_output`.
//!
//! Output path: reader thread → channel → coalescer task (8 ms / 64 KiB) →
//! [`Session::output`] which appends to the ring + file and broadcasts under one
//! lock, so an attach-with-replay never loses or duplicates bytes.
//!
//! This file owns the [`Session`] state; behaviour is split by concern:
//! - [`spawn`]: starting a child (PTY or pipe) and the login shell lookup
//! - [`stream`]: reader thread + coalescer feeding [`Session::output`]
//! - [`attach`]: ring/file/broadcast and the attach/detach subscriptions
//! - [`lifecycle`]: cold restore, handoff adoption, freeze/thaw
//! - [`control`]: write/resize/signal/kill, scrollback maintenance, inspection
//! - [`master`]: the pty master (spawned or adopted fd)
//! - [`title`]: OSC 0/2 window-title parser

mod attach;
mod control;
mod lifecycle;
mod master;
mod spawn;
mod stream;
mod title;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::protocol::Scope;
use master::Master;

const RING_MAX: usize = 2 * 1024 * 1024;

pub type Out = mpsc::UnboundedSender<String>;
/// Bounded: a raw reader that falls behind blocks the pty reader (backpressure to
/// the program, as in a native terminal) instead of hostd buffering without limit.
pub type RawOut = mpsc::Sender<Vec<u8>>;
pub const RAW_QUEUE: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub cwd: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    pub created_at: u64,
}

struct Io {
    /// `None` in pipe mode (no terminal to resize).
    master: Option<Master>,
    writer: Box<dyn Write + Send>,
}

struct Buf {
    ring: VecDeque<u8>,
    attachers: HashMap<u64, Out>,
    /// Attached without output: title/exit events only.
    listeners: HashMap<u64, Out>,
    /// `pty_attach_raw` connections: raw output chunks.
    raw: HashMap<u64, RawOut>,
    file: Option<std::fs::File>,
    file_len: u64,
}

impl Buf {
    fn new(ring: VecDeque<u8>, file: Option<std::fs::File>, file_len: u64) -> Self {
        Self {
            ring,
            attachers: HashMap::new(),
            listeners: HashMap::new(),
            raw: HashMap::new(),
            file,
            file_len,
        }
    }

    /// The ring as one contiguous buffer.
    fn ring_bytes(&self) -> Vec<u8> {
        let (a, c) = self.ring.as_slices();
        [a, c].concat()
    }

    /// Title/exit: every attached connection.
    fn notify(&mut self, line: &str) {
        Self::broadcast(&mut self.attachers, line);
        Self::broadcast(&mut self.listeners, line);
    }

    fn broadcast(conns: &mut HashMap<u64, Out>, line: &str) {
        conns.retain(|_, out| out.send(line.to_string()).is_ok());
    }
}

pub struct Session {
    pub id: String,
    pub meta: Meta,
    pub pid: Option<u32>,
    title: Mutex<String>,
    /// `None` while alive; `Some(code)` once exited (`Some(None)` = unknown/cold).
    exited: Mutex<Option<Option<i32>>>,
    size: Mutex<(u16, u16)>,
    io: Mutex<Option<Io>>,
    buf: Mutex<Buf>,
    dir: PathBuf,
    /// Set by `freeze`: the reader thread stops after its current read and the
    /// coalescer flushes and signals `frozen` instead of finishing the session.
    stop: Arc<AtomicBool>,
    frozen: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

/// Everything that differs between a fresh, a cold and an adopted session.
struct Init {
    id: String,
    meta: Meta,
    pid: Option<u32>,
    title: String,
    exited: Option<Option<i32>>,
    size: (u16, u16),
    io: Option<Io>,
    buf: Buf,
}

impl Session {
    fn new(init: Init, dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            id: init.id,
            meta: init.meta,
            pid: init.pid,
            title: Mutex::new(init.title),
            exited: Mutex::new(init.exited),
            size: Mutex::new(init.size),
            io: Mutex::new(init.io),
            buf: Mutex::new(init.buf),
            dir: dir.to_path_buf(),
            stop: Arc::default(),
            frozen: Mutex::new(None),
        })
    }

    pub fn exited(&self) -> Option<Option<i32>> {
        *self.exited.lock()
    }
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn bin_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.bin"))
}
fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}
/// Windows: the batch file a shell-form terminal runs (see `spawn`).
fn batch_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.cmd"))
}

fn open_append(p: &Path) -> Option<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .ok()
}

/// Last `RING_MAX` bytes of scrollback as a ring, plus the file's full length.
fn ring_tail(data: &[u8]) -> (VecDeque<u8>, u64) {
    let start = data.len().saturating_sub(RING_MAX);
    (data[start..].iter().copied().collect(), data.len() as u64)
}
