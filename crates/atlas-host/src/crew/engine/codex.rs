//! Codex engine — `codex exec` with JSONL output.

use std::path::Path;

use serde_json::Value;

use crate::crew::store::{CrewmateSession, UsageRecord};
use crate::paths;

use super::launch::McpServer;
use super::{TurnCtx, TurnResult, launch, program};
use crate::crew::spawn::layout;

/// The most of the reply file hostd reads from a crewmate's tree.
const MAX_REPLY: u64 = 4 << 20;

/// Build the codex argv (excluding the binary name) for one turn.
///
/// `session_id` is the prior `thread_id`; `None` for first turn.
/// `result_file` receives the final assistant message text.
/// `mcp` is the atlas MCP server, given to this run only as `-c mcp_servers.atlas.*` overrides.
pub(crate) fn exec_args(
    session_id: Option<&str>,
    result_file: &Path,
    prompt: &str,
    mcp: &McpServer,
) -> Vec<String> {
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
    v.extend(mcp.codex_overrides());
    v.push(prompt.to_string());
    v
}

pub(super) async fn run(ctx: TurnCtx<'_>) -> Result<TurnResult, String> {
    // tree-access: ctx.crewmate_dir and the result file are hostd's own (crew-runtime under isolation)
    std::fs::create_dir_all(ctx.crewmate_dir)
        .map_err(|e| format!("codex: create crewmate_dir: {e}"))?;

    // codex writes the last agent message to this file. An isolated crewmate cannot write into `crew-runtime`
    // (the agent reads it, never changes it), so its file is a name directly in its work tree, which it owns and
    // hostd (the crew group) reads beneath the tree and removes. Otherwise the crewmate's dir, as before.
    let name = format!("codex-out-{}.txt", paths::new_id());
    let isolated = ctx.exec.is_isolated();
    let work = Path::new(ctx.work_dir);
    let result_file = if isolated {
        work.join(format!(".atlas-{name}"))
    } else {
        ctx.crewmate_dir.join(&name)
    };

    let mut cmd = ctx.command(&program("codex"));
    cmd.args(exec_args(
        ctx.session.session_id.as_deref(),
        &result_file,
        ctx.prompt,
        &ctx.mcp_server(),
    ));

    let out = launch::output(ctx.exec, cmd, ctx.cancel, "codex").await?;
    let (engine, session) = (ctx.engine, ctx.session);

    if !out.success() {
        return Err(launch::exit_error("codex", &out));
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
    let reply = if isolated {
        let rel = format!(".atlas-{name}");
        let text = layout::read_beneath(work, &rel, MAX_REPLY).unwrap_or_default();
        // tree-access: the name is ours and `work/` is an entry of the root-owned slug dir; unlink never follows the last link
        let _ = std::fs::remove_file(&result_file);
        text
    } else {
        // tree-access: ctx.crewmate_dir and the result file are hostd's own (crew-runtime under isolation)
        let text = std::fs::read_to_string(&result_file).unwrap_or_default();
        // tree-access: ctx.crewmate_dir and the result file are hostd's own (crew-runtime under isolation)
        let _ = std::fs::remove_file(&result_file);
        text
    }
    .trim()
    .to_string();

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
