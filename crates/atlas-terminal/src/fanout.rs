#![allow(unused)]

use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 256;

pub struct SessionBroadcast {
    tx: broadcast::Sender<Vec<u8>>,
}

impl SessionBroadcast {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx }
    }

    pub fn send(&self, data: Vec<u8>) {
        // Ignore error if no receivers
        let _ = self.tx.send(data);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for SessionBroadcast {
    fn default() -> Self {
        Self::new()
    }
}
