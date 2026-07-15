//! ACP Transport — manages a subprocess with JSON-RPC 2.0 over stdio.
//!
//! The transport handles:
//! - Spawning the agent binary (`kiro-cli acp`)
//! - Sending requests (with response correlation via id)
//! - Receiving notifications (session/update) and broadcasting as AgentEvents
//! - Handling agent→client requests (fs, terminal, permissions)
//! - Graceful shutdown on drop

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{debug, error, info, warn};

use super::events::*;

// ─── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct RpcMessage {
    jsonrpc: Option<String>,
    id: Option<u64>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

// ─── Client capability handler ──────────────────────────────────────────────

/// Trait for handling agent→client requests (filesystem, terminal, permissions).
/// Implementors provide the actual execution of these operations.
#[async_trait::async_trait]
pub trait AcpClientHandler: Send + Sync + 'static {
    /// Read a file's content.
    async fn read_file(&self, path: &str) -> Result<String, String>;
    /// Write content to a file.
    async fn write_file(&self, path: &str, content: &str) -> Result<(), String>;
    /// Create a terminal session and return its ID.
    async fn terminal_create(&self, command: &str, cwd: &str) -> Result<String, String>;
    /// Write input to a terminal.
    async fn terminal_input(&self, terminal_id: &str, data: &str) -> Result<(), String>;
}

/// Default handler that executes filesystem operations directly.
pub struct DirectClientHandler {
    cwd: PathBuf,
}

impl DirectClientHandler {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

#[async_trait::async_trait]
impl AcpClientHandler for DirectClientHandler {
    async fn read_file(&self, path: &str) -> Result<String, String> {
        let full_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            self.cwd.join(path)
        };
        tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| format!("read {}: {e}", full_path.display()))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let full_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            self.cwd.join(path)
        };
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| format!("write {}: {e}", full_path.display()))
    }

    async fn terminal_create(&self, command: &str, cwd: &str) -> Result<String, String> {
        // Terminal creation is delegated to the daemon's PtyManager
        // This default handler returns an error — the real handler is set by the daemon
        Err(format!("terminal_create not implemented in direct handler: {command} @ {cwd}"))
    }

    async fn terminal_input(&self, terminal_id: &str, data: &str) -> Result<(), String> {
        Err(format!("terminal_input not implemented: {terminal_id}"))
    }
}

// ─── Pending request tracking ───────────────────────────────────────────────

type ResponseSender = oneshot::Sender<Result<Value, String>>;

// ─── AcpTransport ───────────────────────────────────────────────────────────

pub struct AcpTransport {
    /// Channel to send outbound JSON-RPC messages to the writer task.
    writer_tx: mpsc::UnboundedSender<String>,
    /// Broadcast channel for AgentEvents (subscribers get real-time updates).
    event_tx: broadcast::Sender<AgentEvent>,
    /// Pending requests awaiting responses (id → oneshot sender).
    pending: Arc<Mutex<HashMap<u64, ResponseSender>>>,
    /// Next request ID (monotonically increasing).
    next_id: AtomicU64,
    /// ACP session ID (set after session/new).
    session_id: Arc<Mutex<Option<String>>>,
    /// Agent session ID in Atlas (for event attribution).
    atlas_session_id: String,
    /// Reader task handle.
    reader_handle: tokio::task::JoinHandle<()>,
    /// Writer task handle.
    writer_handle: tokio::task::JoinHandle<()>,
    /// Child process.
    child: Arc<Mutex<Child>>,
}

/// Configuration for spawning an ACP transport.
pub struct AcpSpawnConfig {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub atlas_session_id: String,
}

impl AcpTransport {
    /// Spawn an agent process and establish ACP transport.
    pub async fn spawn(
        config: AcpSpawnConfig,
        client_handler: Arc<dyn AcpClientHandler>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(&config.binary);
        cmd.args(&config.args)
            .current_dir(&config.cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|e| {
            format!("failed to spawn {}: {e}", config.binary.display())
        })?;

        let stdin = child.stdin.take().ok_or("no stdin on child")?;
        let stdout = child.stdout.take().ok_or("no stdout on child")?;
        let stderr = child.stderr.take();

        // Log stderr in background for diagnostics
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                debug!(target: "acp_stderr", "{}", trimmed);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, _) = broadcast::channel::<AgentEvent>(512);
        let pending: Arc<Mutex<HashMap<u64, ResponseSender>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task: sends JSON-RPC messages to child's stdin
        let mut stdin = stdin;
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = writer_rx.recv().await {
                if stdin.write_all(msg.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Reader task: reads child's stdout, dispatches responses and notifications
        let reader_event_tx = event_tx.clone();
        let reader_pending = Arc::clone(&pending);
        let reader_writer_tx = writer_tx.clone();
        let atlas_sid = config.atlas_session_id.clone();
        let reader_client = client_handler;

        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF — process exited
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let msg: RpcMessage = match serde_json::from_str(trimmed) {
                            Ok(m) => m,
                            Err(e) => {
                                debug!(raw = %trimmed, "non-JSON line from agent: {e}");
                                continue;
                            }
                        };

                        // Case 1: Response to our request (has id + result/error)
                        if let Some(id) = msg.id {
                            if msg.result.is_some() || msg.error.is_some() {
                                let mut p = reader_pending.lock().await;
                                if let Some(tx) = p.remove(&id) {
                                    let result = match msg.error {
                                        Some(e) => Err(format!("ACP error: {e}")),
                                        None => Ok(msg.result.unwrap_or(Value::Null)),
                                    };
                                    let _ = tx.send(result);
                                }
                                continue;
                            }
                        }

                        // Case 2: Agent→Client request (has method + id, expects response)
                        if let (Some(method), Some(id)) = (&msg.method, msg.id) {
                            let response = handle_client_request(
                                method,
                                msg.params.as_ref(),
                                &reader_client,
                            )
                            .await;

                            match response {
                                Ok(result) => {
                                    let resp_json = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": result,
                                    });
                                    let mut out =
                                        serde_json::to_string(&resp_json).unwrap_or_default();
                                    out.push('\n');
                                    let _ = reader_writer_tx.send(out);
                                }
                                Err(ref e) if e == "__PERMISSION_DEFERRED__" => {
                                    // Permission request — emit as event, hold response pending
                                    // The UI will call respond_permission() which sends the response
                                    if let Some(params) = msg.params.as_ref() {
                                        let options = params
                                            .get("options")
                                            .and_then(|v| v.as_array())
                                            .map(|arr| {
                                                arr.iter()
                                                    .filter_map(|o| {
                                                        Some(PermissionOption {
                                                            option_id: o.get("optionId")?.as_str()?.to_string(),
                                                            name: o.get("name")?.as_str()?.to_string(),
                                                            kind: match o.get("kind")?.as_str()? {
                                                                "allow_once" => PermissionKind::AllowOnce,
                                                                "allow_session" => PermissionKind::AllowSession,
                                                                "allow_always" => PermissionKind::AllowAlways,
                                                                "reject_once" => PermissionKind::RejectOnce,
                                                                "reject_always" => PermissionKind::RejectAlways,
                                                                _ => PermissionKind::AllowOnce,
                                                            },
                                                        })
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default();

                                        let tool_call = params.get("toolCall");
                                        let tool_call_id = tool_call
                                            .and_then(|t| t.get("toolCallId"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();

                                        let event = AgentEvent {
                                            session_id: atlas_sid.clone(),
                                            event: AgentEventKind::PermissionRequest(PermissionRequest {
                                                request_id: id,
                                                tool_call_id,
                                                tool_name: params
                                                    .get("toolName")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("unknown")
                                                    .to_string(),
                                                description: params
                                                    .get("description")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("Agent requests permission")
                                                    .to_string(),
                                                options,
                                            }),
                                            timestamp_ms: now_ms(),
                                        };
                                        let _ = reader_event_tx.send(event);
                                    }
                                    // Don't respond — respond_permission() will handle it
                                }
                                Err(err) => {
                                    let resp_json = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "error": { "code": -1, "message": err },
                                    });
                                    let mut out =
                                        serde_json::to_string(&resp_json).unwrap_or_default();
                                    out.push('\n');
                                    let _ = reader_writer_tx.send(out);
                                }
                            }
                            continue;
                        }

                        // Case 3: Notification (has method, no id)
                        if let Some(method) = &msg.method {
                            if let Some(event) = parse_notification(
                                method,
                                msg.params.as_ref(),
                                &atlas_sid,
                            ) {
                                let _ = reader_event_tx.send(event);
                            }
                        }
                    }
                    Err(e) => {
                        error!("ACP reader error: {e}");
                        break;
                    }
                }
            }

            info!("ACP reader task exited");

            // Emit session terminated event so subscribers know
            let _ = reader_event_tx.send(AgentEvent {
                session_id: atlas_sid.clone(),
                event: AgentEventKind::SessionStatus(SessionStatus::Terminated),
                timestamp_ms: now_ms(),
            });

            // Drain pending requests so callers don't wait until timeout
            let mut p = reader_pending.lock().await;
            for (_, tx) in p.drain() {
                let _ = tx.send(Err("transport closed".to_string()));
            }
        });

        Ok(Self {
            writer_tx,
            event_tx,
            pending,
            next_id: AtomicU64::new(1),
            session_id: Arc::new(Mutex::new(None)),
            atlas_session_id: config.atlas_session_id,
            reader_handle,
            writer_handle,
            child: Arc::new(Mutex::new(child)),
        })
    }

    /// Subscribe to AgentEvent stream.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Get the broadcast sender (for wiring into daemon's notification system).
    pub fn event_sender(&self) -> broadcast::Sender<AgentEvent> {
        self.event_tx.clone()
    }

    /// Clone the writer channel (for sending messages from other tasks).
    pub fn writer_tx_clone(&self) -> mpsc::UnboundedSender<String> {
        self.writer_tx.clone()
    }

    /// Get the next request ID.
    pub fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a JSON-RPC request and wait for response.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = RpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        line.push('\n');

        let (tx, rx) = oneshot::channel();
        {
            let mut p = self.pending.lock().await;
            p.insert(id, tx);
        }

        self.writer_tx
            .send(line)
            .map_err(|_| "writer channel closed".to_string())?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("response channel dropped".to_string()),
            Err(_) => Err("request timed out (300s)".to_string()),
        }
    }

    /// Send a notification (no response expected).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        line.push('\n');
        self.writer_tx
            .send(line)
            .map_err(|_| "writer channel closed".to_string())
    }

    // ─── ACP Lifecycle Methods ──────────────────────────────────────────

    /// Initialize the ACP connection.
    pub async fn initialize(
        &self,
        client_name: &str,
        client_version: &str,
    ) -> Result<Value, String> {
        self.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {
                        "readTextFile": true,
                        "writeTextFile": true
                    },
                    "terminal": true
                },
                "clientInfo": {
                    "name": client_name,
                    "version": client_version
                }
            }),
        )
        .await
    }

    /// Create a new ACP session.
    pub async fn new_session(&self, cwd: &str) -> Result<String, String> {
        let result = self
            .request(
                "session/new",
                serde_json::json!({
                    "cwd": cwd,
                    "mcpServers": []
                }),
            )
            .await?;

        let session_id = result
            .get("sessionId")
            .or_else(|| result.get("session_id"))
            .and_then(|v| v.as_str())
            .ok_or("missing sessionId in response")?
            .to_string();

        *self.session_id.lock().await = Some(session_id.clone());
        Ok(session_id)
    }

    /// Send a prompt to the agent.
    pub async fn prompt(&self, text: &str) -> Result<Value, String> {
        let session_id = self
            .session_id
            .lock()
            .await
            .clone()
            .ok_or("no active session")?;

        self.request(
            "session/prompt",
            serde_json::json!({
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": text }]
            }),
        )
        .await
    }

    /// Cancel the current operation.
    pub async fn cancel(&self) -> Result<(), String> {
        let session_id = self
            .session_id
            .lock()
            .await
            .clone()
            .ok_or("no active session")?;

        self.notify(
            "session/cancel",
            serde_json::json!({ "sessionId": session_id }),
        )
    }

    /// Respond to a permission request from the agent.
    pub async fn respond_permission(
        &self,
        request_id: u64,
        option_id: &str,
    ) -> Result<(), String> {
        // Permission responses are sent as JSON-RPC responses to the agent's request
        let resp = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "outcome": { "outcome": "selected", "optionId": option_id }
            }
        });
        let mut line = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
        line.push('\n');
        self.writer_tx
            .send(line)
            .map_err(|_| "writer channel closed".to_string())
    }

    /// Check if the transport is still alive.
    pub async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }

    /// Kill the agent process.
    pub async fn kill(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}

impl Drop for AcpTransport {
    fn drop(&mut self) {
        self.reader_handle.abort();
        self.writer_handle.abort();
        // Kill child process synchronously to prevent zombies
        let child = Arc::clone(&self.child);
        tokio::spawn(async move {
            let mut c = child.lock().await;
            let _ = c.kill().await;
        });
    }
}

// ─── Agent→Client request handler ──────────────────────────────────────────

async fn handle_client_request(
    method: &str,
    params: Option<&Value>,
    handler: &Arc<dyn AcpClientHandler>,
) -> Result<Value, String> {
    let params = params.cloned().unwrap_or(Value::Null);

    match method {
        "fs/readTextFile" => {
            let path = params
                .get("path")
                .or_else(|| params.get("filePath"))
                .and_then(|v| v.as_str())
                .ok_or("missing path param")?;
            let content = handler.read_file(path).await?;
            Ok(serde_json::json!({ "content": content }))
        }
        "fs/writeTextFile" => {
            let path = params
                .get("path")
                .or_else(|| params.get("filePath"))
                .and_then(|v| v.as_str())
                .ok_or("missing path param")?;
            let content = params
                .get("content")
                .or_else(|| params.get("text"))
                .and_then(|v| v.as_str())
                .ok_or("missing content param")?;
            handler.write_file(path, content).await?;
            Ok(serde_json::json!({ "success": true }))
        }
        "terminal/create" => {
            let command = params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("bash");
            let cwd = params
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let id = handler.terminal_create(command, cwd).await?;
            Ok(serde_json::json!({ "terminalId": id }))
        }
        "terminal/input" => {
            let terminal_id = params
                .get("terminalId")
                .and_then(|v| v.as_str())
                .ok_or("missing terminalId")?;
            let data = params
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or("missing data")?;
            handler.terminal_input(terminal_id, data).await?;
            Ok(serde_json::json!({ "success": true }))
        }
        // Permission requests are NOT handled here — they're sent as
        // normal JSON-RPC requests from agent→client, but we need to
        // hold the response pending until the user responds via UI.
        // Return a special marker error so the caller knows to defer.
        "session/request_permission" => {
            Err("__PERMISSION_DEFERRED__".to_string())
        }
        _ => {
            warn!(method = method, "unhandled agent→client request");
            Err(format!("method not supported: {method}"))
        }
    }
}

// ─── Notification → AgentEvent parsing ──────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn parse_notification(
    method: &str,
    params: Option<&Value>,
    atlas_session_id: &str,
) -> Option<AgentEvent> {
    let params = params?;
    let update = params.get("update")?;
    let session_update = update.get("sessionUpdate").and_then(|v| v.as_str())?;

    let event_kind = match session_update {
        "agent_message_chunk" => {
            let message_id = update
                .get("messageId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let content = update.get("content")?;
            let content_type = content.get("type").and_then(|v| v.as_str())?;

            match content_type {
                "thinking" => {
                    let text = content.get("thinking").and_then(|v| v.as_str())?;
                    AgentEventKind::ThinkingChunk(ThinkingChunk {
                        message_id,
                        text: text.to_string(),
                        is_continuation: true,
                    })
                }
                _ => {
                    let text = content.get("text").and_then(|v| v.as_str())?;
                    AgentEventKind::TextChunk(TextChunk {
                        message_id,
                        text: text.to_string(),
                        is_continuation: true,
                    })
                }
            }
        }

        "tool_call" => {
            let tool_call_id = update
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let title = update
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let kind_str = update
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("other");
            let tool_kind = match kind_str {
                "read" => ToolKind::Read,
                "edit" => ToolKind::Edit,
                "write" => ToolKind::Write,
                "delete" => ToolKind::Delete,
                "search" => ToolKind::Search,
                "execute" => ToolKind::Execute,
                "think" => ToolKind::Think,
                "fetch" => ToolKind::Fetch,
                _ => ToolKind::Other,
            };
            let tool_name = update
                .get("toolName")
                .or_else(|| update.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(&title)
                .to_string();

            AgentEventKind::ToolCallStart(ToolCallStart {
                tool_call_id,
                tool_name,
                title,
                tool_kind,
                input: update.get("input").cloned().unwrap_or(Value::Null),
            })
        }

        "tool_call_update" => {
            let tool_call_id = update
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let status_str = update
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("in_progress");
            let status = match status_str {
                "pending" => ToolStatus::Pending,
                "in_progress" => ToolStatus::InProgress,
                "completed" => ToolStatus::Completed,
                "failed" => ToolStatus::Failed,
                _ => ToolStatus::InProgress,
            };

            let content = parse_tool_content(update.get("content"));

            AgentEventKind::ToolCallUpdate(ToolCallUpdate {
                tool_call_id,
                status,
                content,
            })
        }

        "plan" => {
            let entries = update
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|entry| {
                            let content =
                                entry.get("content").and_then(|v| v.as_str())?.to_string();
                            let priority = match entry
                                .get("priority")
                                .and_then(|v| v.as_str())
                                .unwrap_or("medium")
                            {
                                "high" => PlanPriority::High,
                                "low" => PlanPriority::Low,
                                _ => PlanPriority::Medium,
                            };
                            let status = match entry
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending")
                            {
                                "in_progress" => PlanStatus::InProgress,
                                "completed" => PlanStatus::Completed,
                                "skipped" => PlanStatus::Skipped,
                                _ => PlanStatus::Pending,
                            };
                            Some(PlanEntry {
                                content,
                                priority,
                                status,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            AgentEventKind::Plan(Plan { entries })
        }

        "usage_update" => {
            let cost = update
                .get("cost")
                .and_then(|c| c.get("amount"))
                .and_then(|v| v.as_f64());

            AgentEventKind::UsageUpdate(UsageUpdate {
                input_tokens: update
                    .get("used")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_usd: cost,
            })
        }

        _ => return None,
    };

    Some(AgentEvent {
        session_id: atlas_session_id.to_string(),
        event: event_kind,
        timestamp_ms: now_ms(),
    })
}

fn parse_tool_content(content: Option<&Value>) -> Option<ToolContent> {
    let content = content?;

    // Content can be an array of content blocks
    let blocks = if let Some(arr) = content.as_array() {
        arr.as_slice()
    } else {
        return None;
    };

    for block in blocks {
        let block_type = block.get("type").and_then(|v| v.as_str())?;
        match block_type {
            "content" => {
                let inner = block.get("content")?;
                let inner_type = inner.get("type").and_then(|v| v.as_str())?;
                if inner_type == "text" {
                    let text = inner.get("text").and_then(|v| v.as_str())?;
                    return Some(ToolContent::Text(text.to_string()));
                }
            }
            "diff" => {
                let path = block.get("path").and_then(|v| v.as_str())?;
                let old = block.get("oldText").and_then(|v| v.as_str()).unwrap_or("");
                let new = block.get("newText").and_then(|v| v.as_str()).unwrap_or("");
                return Some(ToolContent::Diff(DiffContent {
                    path: path.to_string(),
                    old_text: old.to_string(),
                    new_text: new.to_string(),
                }));
            }
            "terminal" => {
                let tid = block
                    .get("terminalId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let output = block.get("output").and_then(|v| v.as_str()).unwrap_or("");
                return Some(ToolContent::Terminal(TerminalContent {
                    terminal_id: tid.to_string(),
                    output: output.to_string(),
                    exit_code: block.get("exitCode").and_then(|v| v.as_i64()).map(|v| v as i32),
                }));
            }
            _ => continue,
        }
    }

    None
}
