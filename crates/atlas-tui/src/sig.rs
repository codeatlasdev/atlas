//! The OS asking this process to stop, as one type on every platform: `SIGTERM` and `SIGHUP`
//! on Unix; on Windows a shutdown or logoff, and the console window closing.

use std::io;

#[cfg(unix)]
pub(crate) struct Signal(tokio::signal::unix::Signal);

#[cfg(unix)]
impl Signal {
    pub(crate) async fn recv(&mut self) -> Option<()> {
        self.0.recv().await
    }
}

/// `SIGTERM` (`atlas down` falls back to it, so do service managers).
#[cfg(unix)]
pub(crate) fn terminate() -> io::Result<Signal> {
    use tokio::signal::unix::{SignalKind, signal};
    signal(SignalKind::terminate()).map(Signal)
}

/// `SIGHUP`: the terminal window was closed.
#[cfg(unix)]
pub(crate) fn hangup() -> io::Result<Signal> {
    use tokio::signal::unix::{SignalKind, signal};
    signal(SignalKind::hangup()).map(Signal)
}

#[cfg(windows)]
pub(crate) enum Signal {
    Shutdown(tokio::signal::windows::CtrlShutdown),
    Close(tokio::signal::windows::CtrlClose),
    /// No console to listen on: never fires.
    Never,
}

#[cfg(windows)]
impl Signal {
    pub(crate) async fn recv(&mut self) -> Option<()> {
        match self {
            Signal::Shutdown(s) => s.recv().await,
            Signal::Close(s) => s.recv().await,
            Signal::Never => std::future::pending().await,
        }
    }
}

/// Windows shuts down or logs off.
#[cfg(windows)]
pub(crate) fn terminate() -> io::Result<Signal> {
    Ok(tokio::signal::windows::ctrl_shutdown().map_or(Signal::Never, Signal::Shutdown))
}

/// The console window was closed (the `SIGHUP` of Windows; the process then has ~5 s).
#[cfg(windows)]
pub(crate) fn hangup() -> io::Result<Signal> {
    Ok(tokio::signal::windows::ctrl_close().map_or(Signal::Never, Signal::Close))
}
