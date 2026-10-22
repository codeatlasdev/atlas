//! Codex engine — `codex exec` with JSONL output.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use tokio::time::timeout;

use crate::bots::store::{BotSession, EngineConfig, UsageRecord};
use crate::paths;

use super::{BOT_TASK_TIMEOUT, TurnResult};

/// Build the codex argv (excluding the binary name) for one turn.
///
/// `session_id` is the prior `thread_id`; `None` for first turn.
/// `result_file` receives the final assistant message text.
pub(crate) fn exec_args(session_id: Option<&str>, result_file: &Path, prompt: &str) -> Vec<String> {
    let mut v = vec!["exec".to_string()];
    if let Some(id) = session_id {
        v.push("resume".to_string());
        v.push(id.to_string());
    }
    v.push("--json".to_string());
    v.push("--output-last-message".to_string());
    v.push(result_file.to_string_lossy().to_string());
    v.push("--dangerously-bypass-hook-trust".to_string());
    v.push("--skip-git-repo-check".to_string());
    v.push(prompt.to_string());
    v
}

pub(super) async fn run(
    engine: &EngineConfig,
    session: BotSession,
    prompt: &str,
    bot_dir: &Path,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
) -> Result<TurnResult, String> {
    std::fs::create_dir_all(bot_dir).map_err(|e| format!("codex: create bot_dir: {e}"))?;

    // codex writes the last agent message to this file.
    let result_file = bot_dir.join(format!("codex-out-{}.txt", paths::new_id()));

    let mut cmd = tokio::process::Command::new("codex");
    for arg in exec_args(session.session_id.as_deref(), &result_file, prompt) {
        cmd.arg(arg);
    }

    cmd.current_dir(work_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let out = timeout(BOT_TASK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "codex: timed out after 2 h".to_string())?
        .map_err(|e| format!("codex: spawn failed: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "codex: exited {}: {stderr}",
            out.status.code().unwrap_or(-1)
        ));
    }

    // Parse JSONL stdout for session id and usage.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut thread_id: Option<String> = session.session_id.clone();
    let mut usage_in: Option<u64> = None;
    let mut usage_out: Option<u64> = None;

    for line in stdout.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match val.get("type").and_then(|t| t.as_str()) {
            Some("thread.started") => {
                if let Some(id) = val.get("thread_id").and_then(|v| v.as_str()) {
                    thread_id = Some(id.to_string());
                }
            }
            Some("usage") => {
                usage_in = val
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .or(usage_in);
                usage_out = val
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .or(usage_out);
            }
            _ => {}
        }
    }

    // Reply is the content of the output file; empty string if missing.
    let reply = std::fs::read_to_string(&result_file)
        .unwrap_or_default()
        .trim()
        .to_string();
    let _ = std::fs::remove_file(&result_file);

    let usage = if usage_in.is_some() || usage_out.is_some() {
        Some(UsageRecord {
            model: engine.model.clone(),
            input_tokens: usage_in,
            output_tokens: usage_out,
            cache_read: None,
            cache_write: None,
            cost_usd: None,
            duration_ms: None,
        })
    } else {
        None
    };

    Ok(TurnResult {
        reply,
        session: BotSession {
            session_id: thread_id,
            session_file: None,
            created_at: session.created_at,
        },
        usage,
    })
}
