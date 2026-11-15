//! Claude engine — headless `-p` mode with JSON output.

use serde_json::Value;

use crate::crew::store::{CrewmateSession, UsageRecord};

use super::{TurnCtx, TurnResult, launch, program};

pub(super) async fn run(ctx: TurnCtx<'_>) -> Result<TurnResult, String> {
    let mcp_config_path = write_mcp_config(&ctx)?;

    let mut cmd = ctx.command(&program("claude"));
    cmd.arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--mcp-config")
        .arg(&mcp_config_path)
        .arg("--permission-prompt-tool")
        .arg("atlas:approve");

    if ctx.yolo {
        cmd.arg("--dangerously-skip-permissions");
    }

    if let Some(sid) = &ctx.session.session_id {
        cmd.arg("--resume").arg(sid);
    }

    cmd.arg(ctx.prompt);

    let out = launch::output(cmd, ctx.cancel, "claude").await?;
    let (engine, session) = (ctx.engine, ctx.session);

    let stdout = String::from_utf8_lossy(&out.stdout);

    // A non-zero exit that still printed a result is parsed below; one that printed nothing is the error.
    if !out.status.success() && stdout.trim().is_empty() {
        return Err(launch::exit_error("claude", &out));
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

/// Write `<crewmate_dir>/mcp-config.json` for the claude `--mcp-config` flag and return its path.
fn write_mcp_config(ctx: &TurnCtx<'_>) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(ctx.crewmate_dir)
        .map_err(|e| format!("claude: create crewmate_dir: {e}"))?;

    let json = serde_json::to_string_pretty(&ctx.mcp_server().claude_json())
        .map_err(|e| format!("claude: serialize mcp config: {e}"))?;

    let path = ctx.crewmate_dir.join("mcp-config.json");
    std::fs::write(&path, json).map_err(|e| format!("claude: write mcp-config.json: {e}"))?;
    Ok(path)
}
