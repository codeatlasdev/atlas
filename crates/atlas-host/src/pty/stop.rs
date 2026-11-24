//! `freeze`'s request to a session's reader thread: a flag, and on Unix a socket pair the reader polls next to the pty
//! master. The reader used to poll the master with a 100 ms bound only to look at the flag: ten wakeups a second per
//! open terminal, forever (Linux idle, #289). Now it sleeps until the program prints or `set` writes a byte.

use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Stop {
    flag: AtomicBool,
    /// (the end the reader polls, the end `set` writes), both non-blocking.
    #[cfg(unix)]
    wake: (
        std::os::unix::net::UnixStream,
        std::os::unix::net::UnixStream,
    ),
}

impl Default for Stop {
    fn default() -> Self {
        Self {
            flag: AtomicBool::new(false),
            #[cfg(unix)]
            wake: {
                let (r, w) = std::os::unix::net::UnixStream::pair()
                    .expect("a socket pair for the pty reader's wake-up");
                r.set_nonblocking(true).expect("non-blocking wake end");
                w.set_nonblocking(true).expect("non-blocking wake end");
                (r, w)
            },
        }
    }
}

impl Stop {
    pub(super) fn is_set(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Ask the reader to stop and wake it if it sleeps in `poll`. A full socket buffer means a wake is already queued.
    pub(super) fn set(&self) {
        self.flag.store(true, Ordering::Release);
        #[cfg(unix)]
        {
            use std::io::Write;
            let _ = (&self.wake.1).write(&[1]);
        }
    }

    /// Withdraw the request (a failed handoff resumes the session) and drop any wake still queued.
    pub(super) fn clear(&self) {
        self.flag.store(false, Ordering::Release);
        #[cfg(unix)]
        self.drain();
    }

    /// The fd the reader polls for a stop request.
    #[cfg(unix)]
    pub(super) fn wake_fd(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.wake.0)
    }

    /// Empty the wake socket: a byte that outlived its request (`set` then `clear` racing the write) must not keep the
    /// reader's poll returning.
    #[cfg(unix)]
    pub(super) fn drain(&self) {
        use std::io::Read;
        let mut buf = [0u8; 16];
        while matches!((&self.wake.0).read(&mut buf), Ok(n) if n > 0) {}
    }
}
