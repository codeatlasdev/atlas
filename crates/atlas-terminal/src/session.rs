#![allow(unused)]

use std::sync::{Arc, Mutex};

use portable_pty::{Child, MasterPty, PtySize};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::fanout::SessionBroadcast;
use crate::scrollback::RingBuffer;

pub struct PtySession {
    pub(crate) id: String,
    pub(crate) master: Box<dyn MasterPty + Send>,
    pub(crate) child: Box<dyn Child + Send + Sync>,
    pub(crate) broadcast: Arc<SessionBroadcast>,
    pub(crate) scrollback: Arc<Mutex<RingBuffer>>,
    pub(crate) size: PtySize,
    pub(crate) shell: String,
    pub(crate) cwd: std::path::PathBuf,
    pub(crate) created_at: std::time::Instant,
    pub(crate) alive: Arc<std::sync::atomic::AtomicBool>,
}

impl PtySession {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn scrollback_snapshot(&self) -> Vec<u8> {
        self.scrollback.lock().unwrap().to_vec()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.broadcast.subscribe()
    }
}
