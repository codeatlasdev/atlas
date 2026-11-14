//! Claude engine — headless `-p` mode with JSON output.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::crew::store::{CrewmateSession, EngineConfig, UsageRecord};

use super::proc::{Cmd, Runner};
use super::{TASK_TIMEOUT, TurnResult};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    engine: &EngineConfig,
    session: CrewmateSession,
    prompt: &str,
    runner: &Runner,
    crewmate_dir: &Path,
    crewmate_id: &str,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
    yolo: bool,
) -> Result<TurnResult, String> {
    write_mcp_config(crewmate_dir, crewmate_id, hook_port, hook_token, pane_key)?;

    let mcp_config_path = crewmate_dir.join("mcp-config.json");

    let mut cmd = Cmd::new("claude", work_dir);
    cmd.arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--mcp-config")
        .arg(mcp_config_path.to_string_lossy())
        .arg("--permission-prompt-tool")
        .arg("atlas:approve");

    if yolo {
        cmd.arg("--dangerously-skip-permissions");
    }

    if let Some(ref sid) = session.session_id {
        cmd.arg("--resume").arg(sid);
    }

    cmd.arg(prompt);

    cmd.envs(extra_env);
    // ATLAS_* always win over crewmate secrets.
    cmd.env("ATLAS_HOOK_PORT", hook_port.to_string());
    cmd.env("ATLAS_HOOK_TOKEN", hook_token);
    cmd.env("ATLAS_CREWMATE_ID", crewmate_id);
    cmd.env("ATLAS_PANE_KEY", pane_key);
    cmd.env("ATLAS_HOOK_DECISION_SECS", "1800");

    let out = runner
        .output(cmd, TASK_TIMEOUT)
        .await
        ?;

    let stdout = out.stdout.as_str();

    if !out.success() && stdout.trim().is_empty() {
        return Err(format!(
            "claude: exited {}: {}",
            out.code.unwrap_or(-1),
            out.error_text()
        ));
    }

    // Claude `-p --output-format json` emits a single JSON object on stdout.
    // In practice there may be progress lines before it; find the result object.
    let result_val: Value = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("result"))
        .ok_or_else(|| format!("claude: no result JSON in output:\n{stdout}"))?;

    let is_error = result_val
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let reply = result_val
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if is_error {
        return Err(format!("claude: agent error: {reply}"));
    }

    let new_session_id = result_val
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or(session.session_id);

    // Usage lives in a nested `usage` object; cost/duration are top-level.
    let usage = result_val.get("usage").map(|u| UsageRecord {
        model: engine.model.clone(),
        input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()),
        output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
        cache_read: u.get("cache_read_input_tokens").and_then(|v| v.as_u64()),
        cache_write: u.get("cache_write_input_tokens").and_then(|v| v.as_u64()),
        cost_usd: result_val.get("cost_usd").and_then(|v| v.as_f64()),
        duration_ms: result_val.get("duration_ms").and_then(|v| v.as_u64()),
    });

    Ok(TurnResult {
        reply,
        session: CrewmateSession {
            session_id: new_session_id,
            session_file: None,
            created_at: session.created_at,
        },
        usage,
    })
}

/// Write `<crewmate_dir>/mcp-config.json` for the claude `--mcp-config` flag.
/// The atlas binary path is resolved from the current executable.
fn write_mcp_config(
    crewmate_dir: &Path,
    crewmate_id: &str,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(crewmate_dir)
        .map_err(|e| format!("claude: create crewmate_dir: {e}"))?;

    let atlas_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "atlas".to_string());

    let config = serde_json::json!({
        "mcpServers": {
            "atlas": {
                "command": atlas_bin,
                "args": ["mcp", "--crewmate", crewmate_id],
                "env": {
                    "ATLAS_HOOK_PORT": hook_port.to_string(),
                    "ATLAS_HOOK_TOKEN": hook_token,
                    "ATLAS_CREWMATE_ID": crewmate_id,
                    "ATLAS_PANE_KEY": pane_key,
                }
            }
        }
    });

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("claude: serialize mcp config: {e}"))?;

    std::fs::write(crewmate_dir.join("mcp-config.json"), json)
        .map_err(|e| format!("claude: write mcp-config.json: {e}"))
}
