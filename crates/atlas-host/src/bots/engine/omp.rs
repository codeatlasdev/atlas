//! omp engine — RPC mode.
//!
//! Protocol (per-turn):
//! 1. Spawn `omp --mode rpc [--auto-approve | --extension <path>] [--resume <session.jsonl>]`
//!    with piped stdin/stdout.  When `yolo` (approval == Auto) we pass `--auto-approve`.
//!    When `!yolo` (approval == Ask) we write a TypeScript extension to `<bot_dir>/omp-approval.ts`
//!    and load it with `--extension`; it blocks tool_call events via hostd's /decision endpoint.
//! 2. Send `{"id":"r1","type":"prompt","message":"<prompt>"}` on stdin.
//! 3. Read stdout lines until `prompt_result` for id `r1` arrives.
//!    Collect `agent_end` reply text and `message_end` usage along the way.
//! 4. Send `{"id":"g","type":"get_state"}` and wait for the matching `response`
//!    to learn the session file path for the next turn.
//! 5. Kill the process.
use std::collections::BTreeMap;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use crate::bots::store::{BotSession, EngineConfig, UsageRecord};

use super::{BOT_TASK_TIMEOUT, TurnResult};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    _engine: &EngineConfig,
    session: BotSession,
    prompt: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
    bot_dir: &std::path::Path,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
    yolo: bool,
) -> Result<TurnResult, String> {
    let mut cmd = tokio::process::Command::new("omp");
    cmd.arg("--mode").arg("rpc");

    if yolo {
        cmd.arg("--auto-approve");
    } else {
        // Write the approval extension and load it.
        let ext_path = bot_dir.join("omp-approval.ts");
        write_approval_extension(&ext_path, hook_port, hook_token, pane_key)?;
        cmd.arg("--extension").arg(&ext_path);
        // Inject env so the extension can reach hostd.
        cmd.env("ATLAS_HOOK_PORT", hook_port.to_string());
        cmd.env("ATLAS_HOOK_TOKEN", hook_token);
        cmd.env("ATLAS_PANE_KEY", pane_key);
    }

    if let Some(session_file) = &session.session_file {
        cmd.arg("--resume").arg(session_file);
    }

    cmd.current_dir(work_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("omp: spawn failed: {e}"))?;

    let mut stdin = child.stdin.take().ok_or("omp: no stdin pipe")?;
    let stdout = child.stdout.take().ok_or("omp: no stdout pipe")?;

    let result = timeout(BOT_TASK_TIMEOUT, drive_rpc(&mut stdin, stdout, prompt)).await;

    // Always kill — the process is not expected to exit on its own.
    let _ = child.kill().await;
    let _ = child.wait().await;

    let (reply, new_session_file, usage) = result
        .map_err(|_| "omp: timed out after 2 h".to_string())?
        .map_err(|e| format!("omp: {e}"))?;

    Ok(TurnResult {
        reply,
        session: BotSession {
            session_id: None,
            session_file: new_session_file.or(session.session_file),
            created_at: session.created_at,
        },
        usage,
    })
}

/// Send the prompt, collect the reply, obtain the session file path.
async fn drive_rpc(
    stdin: &mut tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    prompt: &str,
) -> Result<(String, Option<String>, Option<UsageRecord>), String> {
    let mut lines = BufReader::new(stdout).lines();

    // Send the prompt message.
    let prompt_msg = serde_json::json!({
        "id": "r1",
        "type": "prompt",
        "message": prompt,
    })
    .to_string();
    stdin
        .write_all(format!("{prompt_msg}\n").as_bytes())
        .await
        .map_err(|e| format!("write prompt: {e}"))?;
    stdin.flush().await.map_err(|e| format!("flush: {e}"))?;

    let mut reply = String::new();
    let mut session_file: Option<String> = None;
    let mut input_tokens: Option<u64> = None;
    let mut output_tokens: Option<u64> = None;
    let mut prompt_done = false;

    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        match val.get("type").and_then(|t| t.as_str()) {
            Some("agent_end") => {
                // Extract the last assistant message text.
                if let Some(messages) = val.get("messages").and_then(|m| m.as_array()) {
                    for msg in messages.iter().rev() {
                        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant")
                            && let Some(text) = extract_text(msg)
                        {
                            reply = text;
                            break;
                        }
                    }
                }
            }
            Some("message_end") => {
                if let Some(u) = val.get("usage") {
                    input_tokens = u
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .or(input_tokens);
                    output_tokens = u
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .or(output_tokens);
                }
            }
            Some("prompt_result") if val.get("id").and_then(|v| v.as_str()) == Some("r1") => {
                prompt_done = true;
                // Ask for session state so we can resume next turn.
                let get_state = serde_json::json!({"id": "g", "type": "get_state"}).to_string();
                let _ = stdin.write_all(format!("{get_state}\n").as_bytes()).await;
                let _ = stdin.flush().await;
            }
            Some("prompt_result") => {}
            Some("response") if val.get("id").and_then(|v| v.as_str()) == Some("g") => {
                session_file = val
                    .get("data")
                    .and_then(|d| d.get("sessionFile"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                break; // Have everything we need.
            }
            _ => {}
        }

        // If prompt is done but the process already closed stdout without sending
        // a get_state response, don't hang — exit the loop.
        if prompt_done && session_file.is_none() {
            // Check if the line stream ended (next_line returns None).
            // We can't peek, so we just continue reading; the loop exits naturally
            // when next_line returns Ok(None).
        }
    }

    let usage = if input_tokens.is_some() || output_tokens.is_some() {
        Some(UsageRecord {
            model: None,
            input_tokens,
            output_tokens,
            cache_read: None,
            cache_write: None,
            cost_usd: None,
            duration_ms: None,
        })
    } else {
        None
    };

    Ok((reply, session_file, usage))
}
/// Public wrapper for use from runner.rs.
pub(crate) fn write_approval_extension_pub(
    path: &std::path::Path,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
) -> Result<(), String> {
    write_approval_extension(path, hook_port, hook_token, pane_key)
}

/// Write the TypeScript omp approval extension to `path`.
/// The extension listens for `tool_call` events via the omp Extension API and
/// POSTs to hostd's `/hook/omp/decision` endpoint, blocking until the user
/// allows or denies the tool call (up to 30 minutes).
fn write_approval_extension(
    path: &std::path::Path,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
) -> Result<(), String> {
    // Embed hook coordinates directly so the extension works even if env vars
    // are not propagated by the omp extension runtime.
    let content = format!(
        r#"// Atlas hostd approval extension — generated, do not edit.
// FAIL-CLOSED: any error, timeout, non-2xx, or malformed reply blocks the tool call.

export default function atlasApproval(pi) {{
    const PORT  = process.env.ATLAS_HOOK_PORT  || "{port}";
    const TOKEN = process.env.ATLAS_HOOK_TOKEN || "{token}";
    const PANE  = process.env.ATLAS_PANE_KEY   || "{pane}";

    pi.on("tool_call", async (event) => {{
        let decision = null;
        try {{
            const resp = await fetch(
                `http://127.0.0.1:${{PORT}}/hook/omp/decision`,
                {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "x-atlas-hook-token": TOKEN,
                    }},
                    body: JSON.stringify({{
                        pane_key: PANE,
                        payload: {{ tool_name: event.toolName, tool_input: event.input }},
                    }}),
                    signal: AbortSignal.timeout(1_800_000), // 30 min
                }},
            );
            if (resp.ok) {{
                const data = await resp.json();
                // Only "allow" when we get an explicit allow decision.
                if (data && data.decision === "allow") {{
                    decision = "allow";
                }}
            }}
        }} catch {{
            // Network error, timeout, malformed JSON — fall through to block.
        }}
        // Fail-closed: block unless we received an explicit "allow".
        if (decision !== "allow") {{
            return {{ block: true, reason: "no approval received" }};
        }}
    }});
}}
"#,
        port = hook_port,
        token = hook_token,
        pane = pane_key,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("omp extension dir: {e}"))?;
    }
    std::fs::write(path, content).map_err(|e| format!("omp extension write: {e}"))
}

/// Extract plain text from an omp message object.
/// Content can be a bare string or an array of typed content parts.
fn extract_text(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;

    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    if let Some(parts) = content.as_array() {
        let text: String = parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}
