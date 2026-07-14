#![allow(unused)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::debug;

use crate::manager::PtyManagerInner;

pub(crate) fn spawn_reaper(inner: Arc<Mutex<PtyManagerInner>>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            let mut guard = inner.lock().await;
            let dead_ids: Vec<String> = guard
                .sessions
                .iter_mut()
                .filter_map(|(id, session)| {
                    match session.child.try_wait() {
                        Ok(Some(_status)) => {
                            session.alive.store(false, std::sync::atomic::Ordering::Relaxed);
                            debug!(session_id = %id, "session exited");
                            None // keep in map, just mark dead
                        }
                        Ok(None) => None, // still running
                        Err(_) => {
                            session.alive.store(false, std::sync::atomic::Ordering::Relaxed);
                            None
                        }
                    }
                })
                .collect();
            drop(guard);
        }
    });
}
