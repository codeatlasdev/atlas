//! The server half: the socket, one task per connection, bounded line reads
//! and the subscription stream.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_core::clock::Clock;
use atlas_core::ipc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use super::guard::Guard;
use super::protocol::{ControlCmd, ControlRequest, ControlResponse, WireRequest};
use super::{MAX_LINE_BYTES, REPLY_TIMEOUT, SUBSCRIBER_BUFFER, socket_path};

/// Requests waiting for the run loop (#304). A request that finds the queue full waits for a slot
/// up to [`QUEUE_WAIT`] (the run loop is only briefly busy: a restart grace, a health check, a log
/// flood), then gets [`BUSY`].
pub(crate) const REQUEST_QUEUE: usize = 32;
/// How long a request waits for a queue slot. With the reply wait it stays within what a client
/// waits (`REPLY_TIMEOUT` plus 5 s), so the client sees "busy", not its own timeout.
const QUEUE_WAIT: Duration = Duration::from_secs(4);
/// Connections served at once; one more is answered [`BUSY`] and closed (#304).
pub(crate) const MAX_CONNECTIONS: usize = 64;
/// The error a client gets when the queue or the connection limit is full.
pub(crate) const BUSY: &str = "atlas dev is busy; try again";

/// Owns the socket file: removed on drop. Hold it for the life of the run.
#[derive(Debug)]
pub struct ControlServer {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
    /// Connections being served, kept current by the accept task (tests wait on it).
    #[cfg(test)]
    served: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

impl ControlServer {
    /// Bind `.logs/atlas.sock` (mode 0600), replacing a stale socket file.
    /// MUST be called only while holding the singleton lock — that is what makes
    /// any pre-existing socket file stale.
    pub fn bind(
        root_dir: &Path,
        guard: Guard,
    ) -> std::io::Result<(Self, mpsc::Receiver<ControlRequest>)> {
        Self::bind_with(root_dir, guard, QUEUE_WAIT, Clock::default())
    }

    /// [`bind`](Self::bind) with another wait for a queue slot, measured on `clock` (tests shorten it and move
    /// the clock by hand).
    pub(crate) fn bind_with(
        root_dir: &Path,
        guard: Guard,
        queue_wait: Duration,
        clock: Clock,
    ) -> std::io::Result<(Self, mpsc::Receiver<ControlRequest>)> {
        let path = socket_path(root_dir);
        crate::runtime::ensure_log_dir(&crate::runtime::log_dir(root_dir))?;
        let _ = ipc::remove_stale(&path);
        let listener = ipc::Listener::bind(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        // Bound under the singleton lock, so this is the one live run: it starts a fresh audit
        // file (the previous run's becomes `atlas-audit.prev.log`).
        guard.start_run();
        let (tx, rx) = mpsc::channel(REQUEST_QUEUE);
        #[cfg(test)]
        let served = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(test)]
        let served_by_task = served.clone();
        let task = tokio::spawn(async move {
            // Connections live in a JoinSet owned by the accept task: aborting the
            // accept task drops the set, which aborts every in-flight connection.
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok(stream) = accepted else { break };
                        if conns.len() >= MAX_CONNECTIONS {
                            refuse_busy(stream).await;
                            continue;
                        }
                        conns.spawn(serve_conn(stream, tx.clone(), guard.clone(), queue_wait, clock.clone()));
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
                #[cfg(test)]
                served_by_task.store(conns.len(), std::sync::atomic::Ordering::SeqCst);
            }
        });
        Ok((
            Self {
                path,
                task,
                #[cfg(test)]
                served,
            },
            rx,
        ))
    }

    /// Connections being served right now.
    #[cfg(test)]
    pub(crate) fn served(&self) -> usize {
        self.served.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Subscription phase of a connection: forward pushed lines until the loop drops
/// the sender (shutdown) or the client goes away (read EOF/error, write error).
async fn pump_stream(
    mut rd: BufReader<ipc::ReadHalf>,
    mut wr: ipc::WriteHalf,
    mut events: mpsc::Receiver<String>,
) {
    let mut sink = Vec::new();
    loop {
        tokio::select! {
            line = events.recv() => {
                let Some(mut line) = line else { break };
                line.push('\n');
                if wr.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
            // The subscriber never sends anything useful; a read returning means it left.
            n = rd.read_until(b'\n', &mut sink) => {
                if n.map_or(true, |n| n == 0) {
                    break;
                }
                sink.clear();
            }
        }
    }
}

/// Read one `\n`-terminated line into `buf` without ever buffering more than
/// `MAX_LINE_BYTES` (+ the newline). `Ok(None)` = EOF (or the peer hung up
/// mid-line); `Err(InvalidData)` = the line is longer than the limit and the
/// stream cannot be resynchronised.
pub(crate) async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<()>> {
    let n = r
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', buf)
        .await?;
    if n == 0 {
        return Ok(None);
    }
    let terminated = buf.last() == Some(&b'\n');
    if n > MAX_LINE_BYTES && !(terminated && n == MAX_LINE_BYTES + 1) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request too large",
        ));
    }
    // Peer hung up mid-line: nothing sensible to answer.
    Ok(terminated.then_some(()))
}

/// Tell a connection over the limit why it is closed. One short line into a fresh socket's buffer;
/// the timeout keeps the accept loop from ever waiting on a peer.
async fn refuse_busy(stream: ipc::Stream) {
    let (_rd, mut wr) = ipc::split(stream);
    if let Ok(mut line) = serde_json::to_string(&ControlResponse::err(BUSY)) {
        line.push('\n');
        let _ =
            tokio::time::timeout(Duration::from_millis(100), wr.write_all(line.as_bytes())).await;
    }
}

async fn serve_conn(
    stream: ipc::Stream,
    tx: mpsc::Sender<ControlRequest>,
    guard: Guard,
    queue_wait: Duration,
    clock: Clock,
) {
    let (rd, mut wr) = ipc::split(stream);
    let mut rd = BufReader::new(rd);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let too_large = match read_bounded_line(&mut rd, &mut buf).await {
            Ok(Some(())) => false,
            Ok(None) => break,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => true,
            Err(_) => break,
        };
        let resp = if too_large {
            (ControlResponse::err("request too large"), None)
        } else {
            let line = String::from_utf8_lossy(&buf);
            handle_line(
                line.trim_end_matches(['\n', '\r']),
                &tx,
                &guard,
                queue_wait,
                &clock,
            )
            .await
        };
        let Ok(mut text) = serde_json::to_string(&resp.0) else {
            break;
        };
        text.push('\n');
        if wr.write_all(text.as_bytes()).await.is_err() || too_large {
            // An oversized line cannot be resynchronised: answer, then close.
            break;
        }
        if let Some(events) = resp.1 {
            pump_stream(rd, wr, events).await;
            return;
        }
    }
}

/// Answer for one request line, plus the event receiver when it was a
/// successful `subscribe` (the connection then turns into a stream).
type LineReply = (ControlResponse, Option<mpsc::Receiver<String>>);

async fn handle_line(
    line: &str,
    tx: &mpsc::Sender<ControlRequest>,
    guard: &Guard,
    queue_wait: Duration,
    clock: &Clock,
) -> LineReply {
    if line.len() > MAX_LINE_BYTES {
        return (ControlResponse::err("request too large"), None);
    }
    let req: WireRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return (ControlResponse::err(format!("invalid request: {e}")), None),
    };
    let cmd = match ControlCmd::parse(&req.cmd, &req.args) {
        Ok(c) => c,
        Err(e) => return (ControlResponse::err(e), None),
    };
    let call = guard.call_of(&cmd);
    // The policy is decided here, before the run loop ever sees the request.
    if let Some(refusal) = guard.check(&req.caller, &cmd, call.as_ref()) {
        if let Some(call) = &call {
            guard.audit(&req.caller, call, &req.args, true, &refusal);
        }
        return (refusal, None);
    }
    let (reply, rx) = oneshot::channel();
    let (stream, events) = if cmd == ControlCmd::Subscribe {
        let (s, e) = mpsc::channel(SUBSCRIBER_BUFFER);
        (Some(s), Some(e))
    } else {
        (None, None)
    };
    // `biased`: a free slot wins over an expired wait, as `tokio::time::timeout` polled the send first.
    let queued = tokio::select! {
        biased;
        sent = tx.send(ControlRequest { cmd, reply, stream }) => Some(sent),
        () = clock.sleep(queue_wait) => None,
    };
    match queued {
        Some(Ok(())) => {}
        Some(Err(_)) => return (ControlResponse::err("atlas dev is shutting down"), None),
        None => return (ControlResponse::err(BUSY), None),
    }
    let resp = match tokio::time::timeout(REPLY_TIMEOUT, rx).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(_)) => ControlResponse::err("atlas dev dropped the request"),
        Err(_) => ControlResponse::err("timed out waiting for atlas dev"),
    };
    if let Some(call) = &call {
        guard.audit(&req.caller, call, &req.args, false, &resp);
    }
    // Only a successful subscribe keeps the connection as a stream.
    let events = events.filter(|_| resp.ok);
    (resp, events)
}
