//! OpenCode engine — `opencode run --format json --standalone`.

use std::collections::BTreeMap;

use serde_json::Value;
use tokio::time::timeout;

use crate::bots::store::{BotSession, EngineConfig, UsageRecord};

use super::{BOT_TASK_TIMEOUT, TurnResult};

pub(super) async fn run(
    engine: &EngineConfig,
    session: BotSession,
    prompt: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
) -> Result<TurnResult, String> {
    let mut cmd = tokio::process::Command::new("opencode");
    cmd.arg("run")
        .arg("--format")
        .arg("json")
        .arg("--standalone");

    if let Some(ses_id) = &session.session_id {
        cmd.arg("--session").arg(ses_id);
    }

    cmd.arg("--auto").arg(prompt);

    cmd.current_dir(work_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let out = timeout(BOT_TASK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "opencode: timed out after 2 h".to_string())?
        .map_err(|e| format!("opencode: spawn failed: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "opencode: exited {}: {stderr}",
            out.status.code().unwrap_or(-1)
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);

    // opencode may emit a single JSON object or JSONL; find the result line.
    // Prefer lines that carry a reply field; fall back to parsing the whole stdout.
    let result_val: Value = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| {
            v.get("result").is_some() || v.get("message").is_some() || v.get("text").is_some()
        })
        .or_else(|| serde_json::from_str::<Value>(stdout.trim()).ok())
        .ok_or_else(|| format!("opencode: no JSON in output:\n{stdout}"))?;

    // Reply — try result, message, text in order.
    let reply = result_val
        .get("result")
        .or_else(|| result_val.get("message"))
        .or_else(|| result_val.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Session id — try session_id, session, id in order; fall back to prior.
    let new_session_id = result_val
        .get("session_id")
        .or_else(|| result_val.get("session"))
        .or_else(|| result_val.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or(session.session_id);

    // Usage — check both top-level and nested `usage` object.
    let usage_obj = result_val.get("usage");
    let input_tokens = result_val
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage_obj
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
        });
    let output_tokens = result_val
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage_obj
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
        });
    let cost_usd = result_val.get("cost_usd").and_then(|v| v.as_f64());
    let duration_ms = result_val.get("duration_ms").and_then(|v| v.as_u64());

    let usage = if input_tokens.is_some() || output_tokens.is_some() || cost_usd.is_some() {
        Some(UsageRecord {
            model: engine.model.clone(),
            input_tokens,
            output_tokens,
            cache_read: None,
            cache_write: None,
            cost_usd,
            duration_ms,
        })
    } else {
        None
    };

    Ok(TurnResult {
        reply,
        session: BotSession {
            session_id: new_session_id,
            session_file: None,
            created_at: session.created_at,
        },
        usage,
    })
}
