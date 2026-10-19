//! The server half: the socket, one task per connection, bounded line reads
//! and the subscription stream.

use std::path::{Path, PathBuf};

use atlas_core::ipc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use super::protocol::{ControlCmd, ControlRequest, ControlResponse, WireRequest};
use super::{MAX_LINE_BYTES, REPLY_TIMEOUT, SUBSCRIBER_BUFFER, socket_path};

/// Owns the socket file: removed on drop. Hold it for the life of the run.
#[derive(Debug)]
pub struct ControlServer {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
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
    ) -> std::io::Result<(Self, mpsc::UnboundedReceiver<ControlRequest>)> {
        let path = socket_path(root_dir);
        std::fs::create_dir_all(crate::runtime::log_dir(root_dir))?;
        let _ = ipc::remove_stale(&path);
        let listener = ipc::Listener::bind(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            // Connections live in a JoinSet owned by the accept task: aborting the
            // accept task drops the set, which aborts every in-flight connection.
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok(stream) = accepted else { break };
                        conns.spawn(serve_conn(stream, tx.clone()));
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
            }
        });
        Ok((Self { path, task }, rx))
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

async fn serve_conn(stream: ipc::Stream, tx: mpsc::UnboundedSender<ControlRequest>) {
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
            handle_line(line.trim_end_matches(['\n', '\r']), &tx).await
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

async fn handle_line(line: &str, tx: &mpsc::UnboundedSender<ControlRequest>) -> LineReply {
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
    let (reply, rx) = oneshot::channel();
    let (stream, events) = if cmd == ControlCmd::Subscribe {
        let (s, e) = mpsc::channel(SUBSCRIBER_BUFFER);
        (Some(s), Some(e))
    } else {
        (None, None)
    };
    if tx.send(ControlRequest { cmd, reply, stream }).is_err() {
        return (ControlResponse::err("atlas dev is shutting down"), None);
    }
    match tokio::time::timeout(REPLY_TIMEOUT, rx).await {
        // Only a successful subscribe keeps the connection as a stream.
        Ok(Ok(resp)) => {
            let events = events.filter(|_| resp.ok);
            (resp, events)
        }
        Ok(Err(_)) => (ControlResponse::err("atlas dev dropped the request"), None),
        Err(_) => (
            ControlResponse::err("timed out waiting for atlas dev"),
            None,
        ),
    }
}
