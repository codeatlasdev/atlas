//! Codex engine — `codex exec` with JSONL output.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::crew::store::{CrewmateSession, EngineConfig, UsageRecord};
use crate::paths;

use super::proc::{Cmd, Runner};
use super::{TASK_TIMEOUT, TurnResult};

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
    session: CrewmateSession,
    prompt: &str,
    runner: &Runner,
    crewmate_dir: &Path,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
) -> Result<TurnResult, String> {
    std::fs::create_dir_all(crewmate_dir)
        .map_err(|e| format!("codex: create crewmate_dir: {e}"))?;

    // codex writes the last agent message to this file.
    let result_file = crewmate_dir.join(format!("codex-out-{}.txt", paths::new_id()));

    let mut cmd = Cmd::new("codex", work_dir);
    cmd.args(exec_args(session.session_id.as_deref(), &result_file, prompt));
    cmd.envs(extra_env);

    let out = runner.output(cmd, TASK_TIMEOUT).await?;

    if !out.success() {
        return Err(format!(
            "codex: exited {}: {}",
            out.code.unwrap_or(-1),
            out.error_text()
        ));
    }

    // Parse JSONL stdout for session id and usage.
    let stdout = out.stdout.as_str();
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
        session: CrewmateSession {
            session_id: thread_id,
            session_file: None,
            created_at: session.created_at,
        },
        usage,
    })
}
