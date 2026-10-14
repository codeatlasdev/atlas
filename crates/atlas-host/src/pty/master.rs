//! The pty master end and the child-exit waiter type.

#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd, RawFd};

use anyhow::Result;
use portable_pty::{MasterPty, PtySize};

/// Blocks until the child exits; returns its code.
pub(super) type Waiter = Box<dyn FnOnce() -> Option<i32> + Send>;

/// The pty master: owned by `portable_pty` for sessions this daemon spawned, a bare fd
/// for sessions adopted from a previous daemon (see `handoff`, Unix only). On Windows it is
/// the ConPTY pseudo console; dropping it closes the console.
pub(super) enum Master {
    Pty(Box<dyn MasterPty + Send>),
    #[cfg(unix)]
    Fd(OwnedFd),
}

#[cfg(windows)]
impl Master {
    pub(super) fn resize(&self, size: PtySize) -> Result<()> {
        match self {
            Master::Pty(m) => Ok(m.resize(size)?),
        }
    }
}

#[cfg(unix)]
impl Master {
    pub(super) fn dup_file(&self) -> std::io::Result<std::fs::File> {
        let fd = self
            .raw_fd()
            .ok_or_else(|| std::io::Error::other("no pty fd"))?;
        // SAFETY: `fd` is owned by `self`, alive for this call; the clone is independent.
        let b = unsafe { BorrowedFd::borrow_raw(fd) };
        b.try_clone_to_owned().map(std::fs::File::from)
    }

    pub(super) fn raw_fd(&self) -> Option<RawFd> {
        match self {
            Master::Pty(m) => m.as_raw_fd(),
            Master::Fd(f) => Some(f.as_raw_fd()),
        }
    }

    pub(super) fn resize(&self, size: PtySize) -> Result<()> {
        match self {
            Master::Pty(m) => Ok(m.resize(size)?),
            Master::Fd(f) => {
                let ws = libc::winsize {
                    ws_row: size.rows,
                    ws_col: size.cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                // SAFETY: valid pty master fd and winsize pointer.
                if unsafe { libc::ioctl(f.as_raw_fd(), libc::TIOCSWINSZ, &ws) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(())
            }
        }
    }

    pub(super) fn process_group_leader(&self) -> Option<i32> {
        match self {
            Master::Pty(m) => m.process_group_leader(),
            // SAFETY: tcgetpgrp on a valid fd.
            Master::Fd(f) => match unsafe { libc::tcgetpgrp(f.as_raw_fd()) } {
                n if n > 0 => Some(n),
                _ => None,
            },
        }
    }
}
