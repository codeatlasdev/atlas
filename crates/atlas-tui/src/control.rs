//! Control channel for a running `atlas dev` (TUI or `--headless`).
//!
//! A Unix domain socket at `.logs/atlas.sock` accepts line-delimited JSON:
//! request `{"cmd": "...", "args": {...}}` → response `{"ok": bool, "error"?, "data"?}`.
//! The server only parses and forwards: every request becomes a [`ControlRequest`]
//! on a channel owned by the run loop, which turns it into the same Messages /
//! Effects / manager calls the keyboard uses (so `confirm`, `pause_services` and
//! in-flight guards apply). Clients (the `atlas mcp` server) use [`call`].
//!
//! Beyond service control the socket carries the runner/attach protocol:
//! `info` (who owns this run), `shutdown` (graceful stop, `atlas down`) and
//! `subscribe` (snapshot reply, then one pushed JSON line per event until either
//! side hangs up, see `runner.rs`). A subscription connection is read-only; an
//! attached client sends its requests on separate connections.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::config::ProjectConfig;

pub const SOCKET_NAME: &str = "atlas.sock";
/// Longest a client waits for the owning loop to answer one request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// Requests larger than this are rejected (a control line is a few hundred bytes).
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;
/// Events buffered per subscriber before the hub drops it as a stalled reader.
pub(crate) const SUBSCRIBER_BUFFER: usize = 4096;

pub const NOT_RUNNING: &str = "atlas dev is not running; start it in a terminal";

pub fn socket_path(root_dir: &Path) -> PathBuf {
    crate::runtime::log_dir(root_dir).join(SOCKET_NAME)
}

#[derive(Debug, Deserialize)]
struct WireRequest {
    cmd: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ControlResponse {
    pub fn ok(data: Value) -> Self {
        Self {
            ok: true,
            error: None,
            data: Some(data),
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            data: None,
        }
    }
}

/// A parsed, validated control command.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlCmd {
    StartService(String),
    StopService(String),
    RestartService(String),
    /// Run the action whose key or label matches `target`.
    RunAction {
        target: String,
        confirmed: bool,
    },
    /// Who owns this run: `{mode, pid, project, clients, version}`.
    Info,
    /// Stop every service and exit (`atlas down`, "Stop everything" while attached).
    Shutdown,
    /// Snapshot reply, then a stream of events (see `runner.rs`).
    Subscribe,
}

impl ControlCmd {
    pub fn parse(cmd: &str, args: &Value) -> Result<Self, String> {
        let str_arg = |k: &str| {
            args.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let service = || -> Result<String, String> {
            str_arg("service").ok_or_else(|| "missing required arg: service".to_string())
        };
        match cmd {
            "start_service" => Ok(Self::StartService(service()?)),
            "stop_service" => Ok(Self::StopService(service()?)),
            "restart_service" => Ok(Self::RestartService(service()?)),
            "run_action" => Ok(Self::RunAction {
                target: str_arg("action").ok_or("missing required arg: action (key or label)")?,
                confirmed: args
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            "info" => Ok(Self::Info),
            "shutdown" => Ok(Self::Shutdown),
            "subscribe" => Ok(Self::Subscribe),
            other => Err(format!("unknown cmd: {other}")),
        }
    }
}

/// What the run loop must do for a [`ControlCmd`], after config-level validation.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    StartService(String),
    StopService(String),
    RestartService(String),
    /// Action key plus its label (for the reply).
    RunAction {
        key: char,
        label: String,
    },
}

/// Validate `cmd` against the loaded config: unknown service/action and the
/// `confirm` gate are rejected here with agent-readable errors.
pub fn resolve(cmd: ControlCmd, cfg: &ProjectConfig) -> Result<Resolved, String> {
    let known = |name: &str| -> Result<(), String> {
        if cfg.services.contains_key(name) {
            return Ok(());
        }
        let mut names: Vec<&str> = cfg.services.keys().map(String::as_str).collect();
        names.sort_unstable();
        Err(format!(
            "unknown service '{name}'. Known services: {}",
            names.join(", ")
        ))
    };
    match cmd {
        ControlCmd::Info | ControlCmd::Shutdown | ControlCmd::Subscribe => {
            Err("not a service command".to_string())
        }
        ControlCmd::StartService(n) => known(&n).map(|()| Resolved::StartService(n)),
        ControlCmd::StopService(n) => known(&n).map(|()| Resolved::StopService(n)),
        ControlCmd::RestartService(n) => known(&n).map(|()| Resolved::RestartService(n)),
        ControlCmd::RunAction { target, confirmed } => {
            let action = find_action(cfg, &target).ok_or_else(|| {
                let all: Vec<String> = cfg
                    .actions
                    .iter()
                    .map(|a| format!("{} ({})", a.key, a.label))
                    .collect();
                format!(
                    "unknown action '{target}'. Known actions: {}",
                    all.join(", ")
                )
            })?;
            if let Some(text) = action.confirm.as_deref().filter(|c| !c.is_empty())
                && !confirmed
            {
                return Err(format!(
                    "action '{}' requires confirmation: \"{text}\". Ask the user, then retry with confirmed: true.",
                    action.label
                ));
            }
            Ok(Resolved::RunAction {
                key: action.key,
                label: action.label.clone(),
            })
        }
    }
}

/// Match by exact key char first, then case-insensitive label.
pub fn find_action<'a>(
    cfg: &'a ProjectConfig,
    target: &str,
) -> Option<&'a crate::config::ActionConfig> {
    let mut chars = target.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && let Some(a) = cfg.actions.iter().find(|a| a.key == c)
    {
        return Some(a);
    }
    cfg.actions
        .iter()
        .find(|a| a.label.eq_ignore_ascii_case(target))
}

/// One request handed to the run loop. The loop MUST answer through `reply`.
#[derive(Debug)]
pub struct ControlRequest {
    pub cmd: ControlCmd,
    pub reply: oneshot::Sender<ControlResponse>,
    /// `Subscribe` only: where the loop pushes event lines after the snapshot.
    /// Bounded: a client that stops reading is dropped by the hub, not buffered forever.
    pub stream: Option<mpsc::Sender<String>>,
}

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
        use std::os::unix::fs::PermissionsExt;
        let path = socket_path(root_dir);
        std::fs::create_dir_all(crate::runtime::log_dir(root_dir))?;
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            // Connections live in a JoinSet owned by the accept task: aborting the
            // accept task drops the set, which aborts every in-flight connection.
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
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
    mut rd: BufReader<tokio::net::unix::OwnedReadHalf>,
    mut wr: tokio::net::unix::OwnedWriteHalf,
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

async fn serve_conn(stream: UnixStream, tx: mpsc::UnboundedSender<ControlRequest>) {
    let (rd, mut wr) = stream.into_split();
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

/// Send one request to the running atlas. `Err` = could not reach it
/// (use [`NOT_RUNNING`] wording for connect failures).
pub async fn call(root_dir: &Path, cmd: &str, args: Value) -> Result<ControlResponse, String> {
    let stream = UnixStream::connect(socket_path(root_dir))
        .await
        .map_err(|_| NOT_RUNNING.to_string())?;
    let (rd, mut wr) = stream.into_split();
    let mut line = json!({ "cmd": cmd, "args": args }).to_string();
    line.push('\n');
    wr.write_all(line.as_bytes())
        .await
        .map_err(|e| format!("control socket write failed: {e}"))?;
    let mut lines = BufReader::new(rd).lines();
    let reply = tokio::time::timeout(REPLY_TIMEOUT + Duration::from_secs(5), lines.next_line())
        .await
        .map_err(|_| "timed out waiting for atlas dev".to_string())?
        .map_err(|e| format!("control socket read failed: {e}"))?
        .ok_or_else(|| "atlas dev closed the connection".to_string())?;
    serde_json::from_str(&reply).map_err(|e| format!("bad reply from atlas dev: {e}"))
}

/// An open subscription: the snapshot, then pushed event lines.
pub struct Subscription {
    pub snapshot: Value,
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    // Keeps the write half open: dropping it would look like a hang-up to the runner.
    _wr: tokio::net::unix::OwnedWriteHalf,
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
    let stream = UnixStream::connect(socket_path(root_dir))
        .await
        .map_err(|_| NOT_RUNNING.to_string())?;
    let (rd, mut wr) = stream.into_split();
    let mut line = json!({ "cmd": "subscribe", "args": {} }).to_string();
    line.push('\n');
    wr.write_all(line.as_bytes())
        .await
        .map_err(|e| format!("control socket write failed: {e}"))?;
    let mut lines = BufReader::new(rd).lines();
    let reply = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .map_err(|_| "timed out waiting for atlas dev".to_string())?
        .map_err(|e| format!("control socket read failed: {e}"))?
        .ok_or_else(|| "atlas dev closed the connection".to_string())?;
    let resp: ControlResponse =
        serde_json::from_str(&reply).map_err(|e| format!("bad reply from atlas dev: {e}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProjectConfig {
        serde_yaml::from_str(
            r#"
name: demo
services:
  api: { command: "sleep 1", port: 3000 }
  web: { command: "sleep 1" }
actions:
  - { key: m, label: Migrate, command: "true" }
  - { key: x, label: Wipe DB, command: "true", confirm: "This wipes the DB." }
"#,
        )
        .unwrap()
    }

    #[test]
    fn parse_requires_service_arg() {
        assert!(ControlCmd::parse("restart_service", &json!({})).is_err());
        assert!(ControlCmd::parse("restart_service", &json!({"service": "  "})).is_err());
        assert_eq!(
            ControlCmd::parse("stop_service", &json!({"service": "api"})).unwrap(),
            ControlCmd::StopService("api".into())
        );
    }

    #[test]
    fn parse_rejects_unknown_cmd() {
        let e = ControlCmd::parse("rm_rf", &json!({})).unwrap_err();
        assert!(e.contains("unknown cmd"));
    }

    #[test]
    fn resolve_rejects_unknown_service_listing_known() {
        let e = resolve(ControlCmd::RestartService("nope".into()), &cfg()).unwrap_err();
        assert!(e.contains("api, web"), "{e}");
    }

    #[test]
    fn resolve_action_by_key_or_label() {
        let by_key = resolve(
            ControlCmd::RunAction {
                target: "m".into(),
                confirmed: false,
            },
            &cfg(),
        )
        .unwrap();
        let by_label = resolve(
            ControlCmd::RunAction {
                target: "migrate".into(),
                confirmed: false,
            },
            &cfg(),
        )
        .unwrap();
        assert_eq!(by_key, by_label);
    }

    #[test]
    fn confirm_action_refused_without_confirmed_and_returns_text() {
        let cmd = |confirmed| ControlCmd::RunAction {
            target: "x".into(),
            confirmed,
        };
        let e = resolve(cmd(false), &cfg()).unwrap_err();
        assert!(e.contains("This wipes the DB."), "{e}");
        assert!(e.contains("confirmed: true"), "{e}");
        assert_eq!(
            resolve(cmd(true), &cfg()).unwrap(),
            Resolved::RunAction {
                key: 'x',
                label: "Wipe DB".into()
            }
        );
    }

    #[tokio::test]
    async fn call_without_server_is_not_running() {
        let dir = tempfile::TempDir::new().unwrap();
        let e = call(dir.path(), "restart_service", json!({"service": "api"}))
            .await
            .unwrap_err();
        assert_eq!(e, NOT_RUNNING);
    }

    #[tokio::test]
    async fn socket_roundtrip_perms_and_cleanup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        // A stale socket file from a crashed run must be replaced.
        std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
        std::fs::write(socket_path(dir.path()), "stale").unwrap();

        let (server, mut rx) = ControlServer::bind(dir.path()).unwrap();
        let mode = std::fs::metadata(socket_path(dir.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        let root = dir.path().to_path_buf();
        let client =
            tokio::spawn(
                async move { call(&root, "restart_service", json!({"service": "api"})).await },
            );
        let req = rx.recv().await.unwrap();
        assert_eq!(req.cmd, ControlCmd::RestartService("api".into()));
        req.reply
            .send(ControlResponse::ok(json!({"x": 1})))
            .unwrap();
        let resp = client.await.unwrap().unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data.unwrap()["x"], 1);

        // Malformed input gets an error response, not a hang or a loop request.
        let bad = call(dir.path(), "nope", json!({})).await.unwrap();
        assert!(!bad.ok);
        assert!(rx.try_recv().is_err());

        drop(server);
        assert!(!socket_path(dir.path()).exists());
    }
}
