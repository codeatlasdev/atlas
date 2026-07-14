#![allow(unused)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};

use atlas_core::Result;
use atlas_core::AtlasError;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tracing::info;

use crate::fanout::SessionBroadcast;
use crate::reaper::spawn_reaper;
use crate::scrollback::RingBuffer;
use crate::session::PtySession;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub shell: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub rows: u16,
    pub cols: u16,
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub shell: String,
    pub rows: u16,
    pub cols: u16,
    pub cwd: PathBuf,
    pub created_at_ms: u64,
    pub alive: bool,
}

pub(crate) struct PtyManagerInner {
    pub(crate) sessions: HashMap<String, PtySession>,
}

pub struct PtyManager {
    inner: Arc<Mutex<PtyManagerInner>>,
}

impl PtyManager {
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(PtyManagerInner {
            sessions: HashMap::new(),
        }));
        spawn_reaper(Arc::clone(&inner));
        Self { inner }
    }

    pub async fn create_session(&self, config: SessionConfig) -> Result<String> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| AtlasError::Io(std::io::Error::other(e.to_string())))?;

        let mut cmd = CommandBuilder::new(&config.shell);
        for arg in &config.args {
            cmd.arg(arg);
        }
        cmd.cwd(&config.cwd);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AtlasError::Io(std::io::Error::other(e.to_string())))?;

        drop(pair.slave);

        let session_id = uuid::Uuid::new_v4().to_string();
        let broadcast = Arc::new(SessionBroadcast::new());
        let scrollback = Arc::new(StdMutex::new(RingBuffer::new()));
        let alive = Arc::new(AtomicBool::new(true));

        // Spawn read loop
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| AtlasError::Io(std::io::Error::other(e.to_string())))?;

        let bc = Arc::clone(&broadcast);
        let sb = Arc::clone(&scrollback);
        let al = Arc::clone(&alive);
        tokio::task::spawn_blocking(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        al.store(false, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        sb.lock().unwrap().write(&data);
                        bc.send(data);
                    }
                    Err(_) => {
                        al.store(false, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                }
            }
        });

        let session = PtySession {
            id: session_id.clone(),
            master: pair.master,
            child,
            broadcast,
            scrollback,
            size,
            shell: config.shell,
            cwd: config.cwd,
            created_at: std::time::Instant::now(),
            alive,
        };

        let mut inner = self.inner.lock().await;
        inner.sessions.insert(session_id.clone(), session);

        info!(session_id = %session_id, "PTY session created");
        Ok(session_id)
    }

    pub async fn attach(
        &self,
        session_id: &str,
    ) -> Result<(Vec<u8>, broadcast::Receiver<Vec<u8>>)> {
        let inner = self.inner.lock().await;
        let session = inner
            .sessions
            .get(session_id)
            .ok_or_else(|| AtlasError::NotFound(format!("session {session_id}")))?;

        let snapshot = session.scrollback_snapshot();
        let rx = session.subscribe();
        Ok((snapshot, rx))
    }

    pub async fn write_input(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AtlasError::NotFound(format!("session {session_id}")))?;

        let mut writer = session
            .master
            .take_writer()
            .map_err(|e| AtlasError::Io(std::io::Error::other(e.to_string())))?;

        writer
            .write_all(data)
            .map_err(AtlasError::Io)?;

        Ok(())
    }

    pub async fn resize(&self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AtlasError::NotFound(format!("session {session_id}")))?;

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        session
            .master
            .resize(size)
            .map_err(|e| AtlasError::Io(std::io::Error::other(e.to_string())))?;

        session.size = size;
        Ok(())
    }

    pub async fn kill_session(&self, session_id: &str) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AtlasError::NotFound(format!("session {session_id}")))?;

        session.child.kill().map_err(|e| {
            AtlasError::Io(std::io::Error::other(e.to_string()))
        })?;
        session
            .alive
            .store(false, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let inner = self.inner.lock().await;
        inner
            .sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id.clone(),
                shell: s.shell.clone(),
                rows: s.size.rows,
                cols: s.size.cols,
                cwd: s.cwd.clone(),
                created_at_ms: s.created_at.elapsed().as_millis() as u64,
                alive: s.is_alive(),
            })
            .collect()
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}
