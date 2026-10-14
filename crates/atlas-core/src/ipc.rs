//! Local IPC endpoints named by a path (`~/.atlas/hostd.sock`, `desktop.sock`, …).
//!
//! Unix: a Unix domain socket at that path. Windows: a named pipe whose name is derived from the
//! path ([`pipe_name`]), so every caller keeps addressing endpoints by path and two directories
//! (tests, agent instances) never collide. Both carry the same byte stream; auth stays the
//! callers' token handshake.

use std::io;
use std::path::Path;

pub use imp::{Listener, ReadHalf, Stream, SyncStream, WriteHalf, connect, connect_sync, split};

/// The named pipe that stands for the endpoint `path` on Windows: `\\.\pipe\atlas-<file>-<hash>`,
/// the hash taken over the path made absolute (its folder resolved on disk when it exists, so a
/// relative, `\\?\` or differently cased spelling of the same socket meets the same pipe) with
/// case and separators folded, as Windows paths compare.
pub fn pipe_name(path: &Path) -> String {
    let resolved = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .and_then(|d| crate::paths::canonical(d).ok())
        .zip(path.file_name())
        .map(|(d, f)| d.join(f))
        .or_else(|| std::path::absolute(path).ok())
        .map(|p| crate::paths::normalize(&p))
        .unwrap_or_else(|| path.to_path_buf());
    let norm = resolved.to_string_lossy().replace('/', "\\").to_lowercase();
    // FNV-1a 64: stable across builds and processes, no dependency.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in norm.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let file: String = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(40)
        .collect();
    format!(r"\\.\pipe\atlas-{file}-{h:016x}")
}

/// Remove a leftover socket file at `path` before binding (a no-op for named pipes, which vanish
/// with their last handle).
pub fn remove_stale(path: &Path) -> io::Result<()> {
    if cfg!(unix) {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
mod imp {
    use super::*;

    pub type Stream = tokio::net::UnixStream;
    pub type ReadHalf = tokio::net::unix::OwnedReadHalf;
    pub type WriteHalf = tokio::net::unix::OwnedWriteHalf;
    pub type SyncStream = std::os::unix::net::UnixStream;

    pub async fn connect(path: &Path) -> io::Result<Stream> {
        tokio::net::UnixStream::connect(path).await
    }

    pub fn connect_sync(path: &Path) -> io::Result<SyncStream> {
        std::os::unix::net::UnixStream::connect(path)
    }

    pub fn split(s: Stream) -> (ReadHalf, WriteHalf) {
        s.into_split()
    }

    pub struct Listener(tokio::net::UnixListener);

    impl Listener {
        pub fn bind(path: &Path) -> io::Result<Self> {
            tokio::net::UnixListener::bind(path).map(Self)
        }

        pub async fn accept(&self) -> io::Result<Stream> {
            self.0.accept().await.map(|(s, _)| s)
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_PIPE_BUSY: i32 = 231;

    /// One end of a connection: the client that dialled or the server instance that accepted.
    pub enum Stream {
        Client(NamedPipeClient),
        Server(NamedPipeServer),
    }

    pub type ReadHalf = tokio::io::ReadHalf<Stream>;
    pub type WriteHalf = tokio::io::WriteHalf<Stream>;

    /// A blocking client end, with the read timeout a `UnixStream` has: a read waits (polling the
    /// pipe) at most that long for data, then fails with `TimedOut`.
    pub struct SyncStream {
        file: std::fs::File,
        read_timeout: std::cell::Cell<Option<Duration>>,
    }

    impl SyncStream {
        pub fn set_read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
            self.read_timeout.set(t);
            Ok(())
        }

        /// Writes to a pipe complete once the server's buffer takes them; no timeout needed.
        pub fn set_write_timeout(&self, _t: Option<Duration>) -> io::Result<()> {
            Ok(())
        }

        /// Wait until the pipe has bytes, is closed, or the read timeout passes.
        fn wait_readable(&self) -> io::Result<()> {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Pipes::PeekNamedPipe;
            let Some(limit) = self.read_timeout.get() else {
                return Ok(());
            };
            let t0 = std::time::Instant::now();
            loop {
                let mut avail = 0u32;
                // SAFETY: valid pipe handle; null buffers only ask for the byte count.
                let ok = unsafe {
                    PeekNamedPipe(
                        self.file.as_raw_handle(),
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut avail,
                        std::ptr::null_mut(),
                    )
                };
                // Closed (the read reports EOF) or data waiting.
                if ok == 0 || avail > 0 {
                    return Ok(());
                }
                if t0.elapsed() >= limit {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "pipe read timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl io::Read for &SyncStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.wait_readable()?;
            (&self.file).read(buf)
        }
    }

    impl io::Read for SyncStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            (&*self).read(buf)
        }
    }

    impl io::Write for &SyncStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            (&self.file).write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            (&self.file).flush()
        }
    }

    impl io::Write for SyncStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            (&*self).write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            (&*self).flush()
        }
    }

    impl AsyncRead for Stream {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Client(c) => Pin::new(c).poll_read(cx, buf),
                Stream::Server(s) => Pin::new(s).poll_read(cx, buf),
            }
        }
    }

    impl AsyncWrite for Stream {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.get_mut() {
                Stream::Client(c) => Pin::new(c).poll_write(cx, buf),
                Stream::Server(s) => Pin::new(s).poll_write(cx, buf),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Client(c) => Pin::new(c).poll_flush(cx),
                Stream::Server(s) => Pin::new(s).poll_flush(cx),
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Stream::Client(c) => Pin::new(c).poll_shutdown(cx),
                // A server end has no half-close: disconnecting tells the client EOF.
                Stream::Server(s) => {
                    let _ = s.disconnect();
                    Pin::new(s).poll_shutdown(cx)
                }
            }
        }
    }

    /// All instances busy: the server is between `accept`s. Retry for up to ~2 s.
    fn busy(e: &io::Error) -> bool {
        e.raw_os_error() == Some(ERROR_PIPE_BUSY)
    }

    pub async fn connect(path: &Path) -> io::Result<Stream> {
        let name = pipe_name(path);
        for _ in 0..200 {
            match ClientOptions::new().open(&name) {
                Err(e) if busy(&e) => tokio::time::sleep(Duration::from_millis(10)).await,
                r => return r.map(Stream::Client),
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "pipe busy"))
    }

    pub fn connect_sync(path: &Path) -> io::Result<SyncStream> {
        let name = pipe_name(path);
        for _ in 0..200 {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&name)
            {
                Err(e) if busy(&e) => std::thread::sleep(Duration::from_millis(10)),
                r => {
                    return r.map(|file| SyncStream {
                        file,
                        read_timeout: Default::default(),
                    });
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "pipe busy"))
    }

    pub fn split(s: Stream) -> (ReadHalf, WriteHalf) {
        tokio::io::split(s)
    }

    /// Accepts connections on the pipe for one path. An instance is always listening, so clients
    /// never see "not found" between two `accept`s.
    pub struct Listener {
        name: String,
        next: tokio::sync::Mutex<NamedPipeServer>,
    }

    impl Listener {
        /// Fails with `AddrInUse` when another process already serves this path.
        pub fn bind(path: &Path) -> io::Result<Self> {
            let name = pipe_name(path);
            let first = ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(&name)
                .map_err(|e| {
                    if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
                        io::Error::new(io::ErrorKind::AddrInUse, format!("{name} is in use"))
                    } else {
                        e
                    }
                })?;
            Ok(Self {
                name,
                next: tokio::sync::Mutex::new(first),
            })
        }

        pub async fn accept(&self) -> io::Result<Stream> {
            let mut next = self.next.lock().await;
            next.connect().await?;
            let fresh = ServerOptions::new()
                .reject_remote_clients(true)
                .create(&self.name)?;
            Ok(Stream::Server(std::mem::replace(&mut *next, fresh)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn pipe_names_are_stable_distinct_per_path_and_fold_case_and_separators() {
        let a = pipe_name(Path::new(r"C:\Users\u\.atlas\hostd.sock"));
        assert_eq!(a, pipe_name(Path::new("c:/users/U/.atlas/hostd.sock")));
        assert_ne!(a, pipe_name(Path::new(r"C:\Users\v\.atlas\hostd.sock")));
        assert!(a.starts_with(r"\\.\pipe\atlas-hostd.sock-"), "{a}");
    }

    #[test]
    fn a_relative_and_an_absolute_spelling_of_one_socket_name_the_same_pipe() {
        let d = tempfile::tempdir().unwrap();
        let abs = d.path().join("x.sock");
        // The same file, reached through `..` from the temp dir's own folder.
        let name = d.path().file_name().unwrap();
        let roundabout = d.path().join("..").join(name).join("x.sock");
        assert_eq!(pipe_name(&abs), pipe_name(&roundabout));
        assert_ne!(pipe_name(&abs), pipe_name(&d.path().join("y.sock")));
    }

    #[tokio::test]
    async fn a_listener_serves_several_clients_in_turn_and_a_second_bind_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.sock");
        let l = Listener::bind(&p).unwrap();
        assert!(Listener::bind(&p).is_err());
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let s = l.accept().await.unwrap();
                let (r, mut w) = split(s);
                let mut line = String::new();
                BufReader::new(r).read_line(&mut line).await.unwrap();
                w.write_all(format!("echo {line}").as_bytes())
                    .await
                    .unwrap();
            }
        });
        for i in 0..3 {
            let (r, mut w) = split(connect(&p).await.unwrap());
            w.write_all(format!("{i}\n").as_bytes()).await.unwrap();
            let mut line = String::new();
            BufReader::new(r).read_line(&mut line).await.unwrap();
            assert_eq!(line, format!("echo {i}\n"));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_blocking_client_talks_to_an_async_listener() {
        use std::io::{BufRead, Write};
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.sock");
        let l = Listener::bind(&p).unwrap();
        let server = tokio::spawn(async move {
            let (r, mut w) = split(l.accept().await.unwrap());
            let mut line = String::new();
            BufReader::new(r).read_line(&mut line).await.unwrap();
            w.write_all(line.to_uppercase().as_bytes()).await.unwrap();
        });
        let p2 = p.clone();
        let got = tokio::task::spawn_blocking(move || {
            let mut s = connect_sync(&p2).unwrap();
            s.write_all(b"hi\n").unwrap();
            let mut line = String::new();
            std::io::BufReader::new(s).read_line(&mut line).unwrap();
            line
        })
        .await
        .unwrap();
        assert_eq!(got, "HI\n");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_blocking_read_gives_up_after_its_timeout_when_the_server_stays_silent() {
        use std::io::Read;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("q.sock");
        let l = Listener::bind(&p).unwrap();
        let server = tokio::spawn(async move {
            let s = l.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
            drop(s);
        });
        let p2 = p.clone();
        let (err, waited) = tokio::task::spawn_blocking(move || {
            let s = connect_sync(&p2).unwrap();
            s.set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let t0 = std::time::Instant::now();
            let e = (&s).read(&mut [0u8; 8]).unwrap_err();
            (e.kind(), t0.elapsed())
        })
        .await
        .unwrap();
        assert!(
            matches!(err, io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock),
            "{err:?}"
        );
        assert!(waited < Duration::from_millis(1500), "{waited:?}");
        server.abort();
    }

    #[tokio::test]
    async fn connecting_to_a_path_nobody_serves_fails() {
        let d = tempfile::tempdir().unwrap();
        assert!(connect(&d.path().join("none.sock")).await.is_err());
    }
}
