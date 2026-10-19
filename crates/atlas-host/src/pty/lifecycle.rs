//! Sessions that outlive or predate this daemon: cold restore from scrollback,
//! adoption from a predecessor, and the freeze/thaw pair of a live handoff
//! (see [`crate::handoff`]).

use std::path::Path;
use std::sync::Arc;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use anyhow::{Context, Result};

#[cfg(unix)]
use super::master::{Master, Waiter};
use super::{Buf, Init, Meta, Session, bin_path, meta_path, ring_tail};
#[cfg(unix)]
use super::{Io, open_append};
#[cfg(unix)]
use crate::handoff::{ExitWatch, SessionState};
use crate::protocol::Scope;

impl Session {
    /// Restore an exited session from a scrollback file left by a previous daemon.
    pub fn cold(id: &str, scroll_dir: &Path) -> Option<Arc<Session>> {
        let data = std::fs::read(bin_path(scroll_dir, id)).ok()?;
        let meta = std::fs::read(meta_path(scroll_dir, id))
            .ok()
            .and_then(|b| serde_json::from_slice::<Meta>(&b).ok())
            .map(|m| Meta {
                argv: Vec::new(),
                ..m
            })
            .unwrap_or(Meta {
                cwd: String::new(),
                argv: Vec::new(),
                scope: Scope::default(),
                created_at: 0,
            });
        let (ring, file_len) = ring_tail(&data);
        Some(Session::new(
            Init {
                id: id.to_string(),
                meta,
                pid: None,
                title: String::new(),
                exited: Some(None),
                size: (80, 24),
                io: None,
                buf: Buf::new(ring, None, file_len),
            },
            scroll_dir,
        ))
    }

    /// Take over a session frozen by the previous daemon (see `handoff`). A live one
    /// arrives with its pty master `fd`; an exited one is restored like a cold session
    /// but keeps its title, size and exit code.
    #[cfg(unix)]
    pub fn adopt(
        st: SessionState,
        fd: Option<std::os::fd::OwnedFd>,
        scroll_dir: &Path,
    ) -> Result<Arc<Session>> {
        let data = std::fs::read(bin_path(scroll_dir, &st.id)).unwrap_or_default();
        let (ring, file_len) = ring_tail(&data);
        let meta = Meta {
            cwd: st.cwd,
            argv: st.argv,
            scope: st.scope,
            created_at: st.created_at,
        };
        let mut buf = Buf::new(ring, None, file_len);
        let mut io = None;
        let mut run = None;
        if let (Some(fd), false) = (fd, st.exited) {
            let reader = fd
                .try_clone()
                .context("dup pty master")
                .map(std::fs::File::from)?;
            let writer = fd
                .try_clone()
                .context("dup pty master")
                .map(std::fs::File::from)?;
            buf.file = open_append(&bin_path(scroll_dir, &st.id));
            io = Some(Io {
                master: Some(Master::Fd(fd)),
                writer: Box::new(writer),
            });
            run = Some(reader);
        }
        let s = Session::new(
            Init {
                id: st.id,
                meta,
                pid: st.pid,
                title: st.title,
                exited: st.exited.then_some(st.exit_code),
                size: (st.cols, st.rows),
                io,
                buf,
            },
            scroll_dir,
        );
        if let Some(reader) = run {
            // The child is not ours: no waitpid. Pty EOF, then the kqueue watch opened
            // now (while it is alive); the exit code is only known when the OS reports it.
            s.run(Box::new(reader), s.exit_waiter());
        }
        Ok(s)
    }

    /// Waiter for a child this daemon did not spawn (adopted, or resumed after a failed
    /// handoff). The watch is registered here, not when the waiter runs.
    #[cfg(unix)]
    fn exit_waiter(&self) -> Waiter {
        let watch = self.pid.map(ExitWatch::new);
        Box::new(move || watch.and_then(ExitWatch::wait))
    }

    /// Stop reading, flush what was read into the ring/file, and describe the session
    /// for the successor. Returns `None` for pipe-mode sessions (no pty master, so
    /// nothing to pass; they end with this daemon). The master fd stays owned by
    /// `self` until the caller has sent it.
    #[cfg(unix)]
    pub async fn freeze(&self) -> Option<(SessionState, Option<std::os::fd::RawFd>)> {
        let exited = self.exited();
        let fd = if exited.is_none() {
            let fd = self.io.lock().as_ref()?.master.as_ref()?.raw_fd()?;
            let (tx, rx) = tokio::sync::oneshot::channel();
            *self.frozen.lock() = Some(tx);
            self.stop.store(true, std::sync::atomic::Ordering::Release);
            // Reader exits within one poll tick (100 ms); a session that exits while
            // freezing finishes instead (then it is reported as exited).
            let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
            Some(fd)
        } else {
            None
        };
        let exited = self.exited();
        let (cols, rows) = *self.size.lock();
        let file_len = self.buf.lock().file_len;
        Some((
            SessionState {
                id: self.id.clone(),
                cwd: self.meta.cwd.clone(),
                argv: self.meta.argv.clone(),
                scope: self.meta.scope.clone(),
                created_at: self.meta.created_at,
                title: self.title.lock().clone(),
                cols,
                rows,
                pid: self.pid,
                file_len,
                exited: exited.is_some(),
                exit_code: exited.flatten(),
                fd: None,
            },
            if exited.is_none() { fd } else { None },
        ))
    }

    /// Resume reading after a failed handoff (the successor never took the session).
    #[cfg(unix)]
    pub fn thaw(self: &Arc<Self>) {
        if self.exited().is_some() {
            return;
        }
        self.stop.store(false, std::sync::atomic::Ordering::Release);
        let reader = self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref()?.dup_file().ok());
        if let Some(reader) = reader {
            self.run(Box::new(reader), self.exit_waiter());
        }
    }
}
