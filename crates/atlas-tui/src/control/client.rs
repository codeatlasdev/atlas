//! The client half: one request (`call`), a subscription, and `info`.

use std::path::Path;
use std::time::Duration;

use atlas_core::ipc::{self, ReadHalf as OwnedReadHalf, WriteHalf as OwnedWriteHalf};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::protocol::ControlResponse;
use super::{NOT_RUNNING, REPLY_TIMEOUT, socket_path};

/// Connect, send one request line and read the first reply line. The read and
/// write halves come back so a subscription can keep streaming on them.
async fn send_request(
    root_dir: &Path,
    cmd: &str,
    args: Value,
    wait: Duration,
) -> Result<
    (
        ControlResponse,
        tokio::io::Lines<BufReader<OwnedReadHalf>>,
        OwnedWriteHalf,
    ),
    String,
> {
    let stream = ipc::connect(&socket_path(root_dir))
        .await
        .map_err(|_| NOT_RUNNING.to_string())?;
    let (rd, mut wr) = ipc::split(stream);
    let mut line = json!({ "cmd": cmd, "args": args }).to_string();
    line.push('\n');
    wr.write_all(line.as_bytes())
        .await
        .map_err(|e| format!("control socket write failed: {e}"))?;
    let mut lines = BufReader::new(rd).lines();
    let reply = tokio::time::timeout(wait, lines.next_line())
        .await
        .map_err(|_| "timed out waiting for atlas dev".to_string())?
        .map_err(|e| format!("control socket read failed: {e}"))?
        .ok_or_else(|| "atlas dev closed the connection".to_string())?;
    let resp =
        serde_json::from_str(&reply).map_err(|e| format!("bad reply from atlas dev: {e}"))?;
    Ok((resp, lines, wr))
}

/// Send one request to the running atlas. `Err` = could not reach it
/// (use [`NOT_RUNNING`] wording for connect failures).
pub async fn call(root_dir: &Path, cmd: &str, args: Value) -> Result<ControlResponse, String> {
    let wait = REPLY_TIMEOUT + Duration::from_secs(5);
    send_request(root_dir, cmd, args, wait)
        .await
        .map(|(resp, _, _)| resp)
}

/// An open subscription: the snapshot, then pushed event lines.
pub struct Subscription {
    pub snapshot: Value,
    lines: tokio::io::Lines<BufReader<OwnedReadHalf>>,
    // Keeps the write half open: dropping it would look like a hang-up to the runner.
    _wr: OwnedWriteHalf,
}

impl Subscription {
    /// Next pushed event line (parsed). `None` = the runner closed the stream.
    pub async fn next(&mut self) -> Option<Value> {
        loop {
            let line = self.lines.next_line().await.ok().flatten()?;
            if let Ok(v) = serde_json::from_str(&line) {
                return Some(v);
            }
        }
    }
}

/// Open a subscription on the running atlas. The snapshot and every later event
/// are consistent: nothing is lost or repeated between them.
pub async fn subscribe(root_dir: &Path) -> Result<Subscription, String> {
    let (resp, lines, wr) =
        send_request(root_dir, "subscribe", json!({}), Duration::from_secs(10)).await?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "subscribe refused".into()));
    }
    Ok(Subscription {
        snapshot: resp.data.unwrap_or(Value::Null),
        lines,
        _wr: wr,
    })
}

/// Answer of the `info` command.
#[derive(Debug, Clone, PartialEq)]
pub struct RunInfo {
    /// `runner` (attachable: `--detach` / `--headless`) or `tui` (owns a terminal).
    pub mode: String,
    pub pid: i64,
    pub project: String,
    pub clients: u64,
}

/// Ask the running atlas who it is. `None` = nothing answers on the socket.
pub async fn info(root_dir: &Path) -> Option<RunInfo> {
    let resp = call(root_dir, "info", json!({})).await.ok()?;
    let d = resp.data.filter(|_| resp.ok)?;
    Some(RunInfo {
        mode: d["mode"].as_str()?.to_string(),
        pid: d["pid"].as_i64()?,
        project: d["project"].as_str().unwrap_or_default().to_string(),
        clients: d["clients"].as_u64().unwrap_or(0),
    })
}
