//! Audit trail of control calls: one JSON object per line in `.logs/atlas-audit.log`.
//!
//! Written by whoever answers the request (the control server for socket calls, the TUI
//! for keyboard actions), so it covers every caller, not just MCP. Rotated like the other
//! logs: the previous run's file becomes `atlas-audit.prev.log` when atlas starts, and the
//! current file rotates the same way once past [`MAX_AUDIT_BYTES`].

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::protocol::Caller;

/// Log name: `.logs/atlas-audit.log` (and `atlas-audit.prev.log`).
pub const AUDIT_LOG: &str = "atlas-audit";
/// Rotate the current file once it passes this size.
const MAX_AUDIT_BYTES: u64 = 5 * 1024 * 1024;
/// Longest value kept per argument in the summary.
const MAX_ARG_CHARS: usize = 200;

/// How a control call ended.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Let through and accepted by atlas.
    Allowed,
    /// Refused by the `agents:` policy.
    Denied,
    /// Let through, but atlas refused or failed it (unknown service, confirm gate, …).
    Error,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Error => "error",
        }
    }
}

/// One audit line, as written and as read back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    /// RFC 3339, local time.
    pub ts: String,
    /// `human` or `agent`.
    pub caller: String,
    /// MCP client name and version from the `initialize` handshake, when it sent them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Agent session labels from the environment (`OMP_SESSION_ID`, `CLAUDE_SESSION_ID`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Value>,
    /// Control command: `restart_service`, `run_action`, …
    pub tool: String,
    /// Service name or action key/label.
    pub target: String,
    /// Other arguments, shortened (`{"confirmed": true}`).
    #[serde(default)]
    pub args: Value,
    pub result: Outcome,
    /// Denial reason or error text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AuditEntry {
    pub fn new(
        caller: &Caller,
        tool: &str,
        target: &str,
        args: &Value,
        result: Outcome,
        detail: Option<String>,
    ) -> Self {
        Self {
            ts: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
            caller: caller.kind().to_string(),
            client: caller.client_label(),
            session: caller.session().cloned(),
            tool: tool.to_string(),
            target: target.to_string(),
            args: summarize_args(args),
            result,
            detail,
        }
    }

    /// One line for humans: `10:00:05 agent omp/14.1 restart_service api -> denied: …`.
    pub fn render(&self) -> String {
        let time = self.ts.get(11..19).unwrap_or(&self.ts);
        let who = match &self.client {
            Some(c) => format!("{} {c}", self.caller),
            None => self.caller.clone(),
        };
        let result = match &self.detail {
            Some(d) => format!("{}: {d}", self.result.as_str()),
            None => self.result.as_str().to_string(),
        };
        format!("{time} {who} {} {} -> {result}", self.tool, self.target)
    }
}

/// Every argument except the target, with long strings cut: the log must stay one short
/// line per call whatever an agent sends.
fn summarize_args(args: &Value) -> Value {
    let Some(obj) = args.as_object() else {
        return json!({});
    };
    let kept = obj
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "service" | "action"))
        .map(|(k, v)| {
            let v = match v {
                Value::String(s) if s.chars().count() > MAX_ARG_CHARS => {
                    let cut: String = s.chars().take(MAX_ARG_CHARS).collect();
                    Value::String(format!("{cut}…"))
                }
                other => other.clone(),
            };
            (k.clone(), v)
        })
        .collect();
    Value::Object(kept)
}

fn path(log_dir: &Path) -> std::path::PathBuf {
    log_dir.join(format!("{AUDIT_LOG}.log"))
}

/// Start a run's audit file: the previous run's file becomes `atlas-audit.prev.log`.
/// Best-effort like every disk log: auditing never blocks a control call.
pub fn rotate_for_new_run(log_dir: &Path) {
    let _ = std::fs::create_dir_all(log_dir);
    let cur = path(log_dir);
    if cur.exists() {
        let _ = std::fs::rename(&cur, log_dir.join(format!("{AUDIT_LOG}.prev.log")));
    }
}

/// Append one entry. Rotates first when the file is over [`MAX_AUDIT_BYTES`].
pub fn record(log_dir: &Path, entry: &AuditEntry) {
    let _ = std::fs::create_dir_all(log_dir);
    let cur = path(log_dir);
    if std::fs::metadata(&cur).is_ok_and(|m| m.len() >= MAX_AUDIT_BYTES) {
        rotate_for_new_run(log_dir);
    }
    let Ok(mut line) = serde_json::to_string(entry) else {
        return;
    };
    line.push('\n');
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cur)
    {
        // One write per line: O_APPEND keeps concurrent callers from interleaving.
        let _ = f.write_all(line.as_bytes());
    }
}

/// The newest `limit` entries, oldest first. Lines that are not valid entries are skipped
/// (a half-written tail line after a crash must not hide the rest).
pub fn read_recent(log_dir: &Path, limit: usize, include_previous: bool) -> Vec<AuditEntry> {
    let mut all = Vec::new();
    let files = [
        include_previous.then(|| log_dir.join(format!("{AUDIT_LOG}.prev.log"))),
        Some(path(log_dir)),
    ];
    for f in files.into_iter().flatten() {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        all.extend(
            text.lines()
                .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok()),
        );
    }
    let start = all.len().saturating_sub(limit);
    all.split_off(start)
}
