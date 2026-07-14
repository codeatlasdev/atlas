//! Agent events — the unified enum representing all structured activity
//! that flows from an agent session to the UI.
//!
//! These events are protocol-agnostic: they're produced by ACP transport
//! but could also be produced by future protocol adapters (Claude Code's
//! stream-json, custom plugins, etc).

use serde::{Deserialize, Serialize};

/// A single agent event, timestamped and attributed to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub session_id: String,
    pub event: AgentEventKind,
    pub timestamp_ms: u64,
}

/// The kinds of events an agent session can produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum AgentEventKind {
    /// Streaming text chunk from the agent's response.
    TextChunk(TextChunk),

    /// Agent is thinking/reasoning (extended thinking block).
    ThinkingChunk(ThinkingChunk),

    /// Agent started a tool call.
    ToolCallStart(ToolCallStart),

    /// Tool call progress update (output, status change).
    ToolCallUpdate(ToolCallUpdate),

    /// Agent shared its execution plan.
    Plan(Plan),

    /// Agent requests permission to proceed.
    PermissionRequest(PermissionRequest),

    /// Token usage / cost update.
    UsageUpdate(UsageUpdate),

    /// A subagent was spawned.
    SubagentSpawned(SubagentSpawned),

    /// A subagent completed.
    SubagentCompleted(SubagentCompleted),

    /// The current turn ended.
    TurnEnd(TurnEnd),

    /// Session-level status change.
    SessionStatus(SessionStatus),
}

// ─── Event Data Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub message_id: String,
    pub text: String,
    /// If true, this continues the previous chunk (append). If false, new message.
    pub is_continuation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingChunk {
    pub message_id: String,
    pub text: String,
    pub is_continuation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStart {
    pub tool_call_id: String,
    pub tool_name: String,
    pub title: String,
    pub tool_kind: ToolKind,
    /// Structured input params (e.g., file_path, command, pattern)
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallUpdate {
    pub tool_call_id: String,
    pub status: ToolStatus,
    /// Optional output content.
    pub content: Option<ToolContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Write,
    Delete,
    Search,
    Execute,
    Think,
    Fetch,
    Glob,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ToolContent {
    /// Plain text output.
    Text(String),
    /// A file diff.
    Diff(DiffContent),
    /// Terminal/command output.
    Terminal(TerminalContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffContent {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalContent {
    pub terminal_id: String,
    pub output: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanPriority,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub request_id: u64,
    pub tool_call_id: String,
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    AllowOnce,
    AllowSession,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageUpdate {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSpawned {
    pub subagent_session_id: String,
    pub task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentCompleted {
    pub subagent_session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEnd {
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    Cancelled,
    Refusal,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Initializing,
    Ready,
    Working,
    WaitingPermission,
    Compacting,
    Idle,
    Terminated,
}
