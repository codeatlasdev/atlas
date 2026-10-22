//! Headless engine runners — one conversational turn per engine type.
//!
//! Each engine module implements the same contract: spawn the CLI tool,
//! parse its output, and return a [`TurnResult`] with the reply text,
//! updated session state, and optional token/cost usage.

pub mod claude;
pub mod codex;
pub mod omp;
pub mod opencode;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::bots::store::{BotSession, EngineConfig, UsageRecord};

/// 2-hour hard timeout per bot task (mirrors runner::BOT_TASK_TIMEOUT).
pub(super) const BOT_TASK_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Result of one conversational turn.
pub struct TurnResult {
    /// The engine's reply text.
    pub reply: String,
    /// Updated session state to persist for the next turn.
    pub session: BotSession,
    /// Token/cost usage, only set when the engine reports it.
    pub usage: Option<UsageRecord>,
}

/// Run one turn of conversation for the given engine.
///
/// * `engine`     — engine kind + optional model/provider/base_url
/// * `session`    — prior session state (empty for first turn)
/// * `prompt`     — the full prompt text to send
/// * `bot_dir`    — `<store>/bots/<id>/` — scratch space (mcp config, codex output file, …)
/// * `bot_id`     — bot identifier, forwarded to the MCP server
/// * `hook_port`  — running hook server port
/// * `hook_token` — running hook server token
/// * `pane_key`   — PTY pane key for permission prompts (`bot:<id>:<task_id>`)
/// * `extra_env`  — bot-scoped secrets, forwarded as subprocess env vars
/// * `work_dir`   — working directory for the subprocess
/// * `yolo`       — when true, skip interactive permission prompts (auto-approve)
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    engine: &EngineConfig,
    session: BotSession,
    prompt: &str,
    bot_dir: &Path,
    bot_id: &str,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
    yolo: bool,
) -> Result<TurnResult, String> {
    // Stamp created_at on first turn.
    let session = if session.created_at == 0 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        BotSession {
            created_at: now,
            ..session
        }
    } else {
        session
    };

    // In tests, a fake binary overrides all engine dispatch.
    // Clone out of the lock before awaiting so the guard is not held across an await point.
    #[cfg(test)]
    let test_bin = TEST_ENGINE_BIN.lock().clone();
    #[cfg(test)]
    if let Some(bin) = test_bin {
        return run_fake(&bin, session, prompt, extra_env, work_dir).await;
    }

    match engine.kind.as_str() {
        "claude" => {
            claude::run(
                engine, session, prompt, bot_dir, bot_id, hook_port, hook_token, pane_key,
                extra_env, work_dir, yolo,
            )
            .await
        }
        "codex" => codex::run(engine, session, prompt, bot_dir, extra_env, work_dir).await,
        "opencode" => opencode::run(engine, session, prompt, extra_env, work_dir).await,
        "omp" => omp::run(engine, session, prompt, extra_env, work_dir).await,
        other => Err(format!("unknown engine kind '{other}'")),
    }
}

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// Test-only: override all engine dispatching with a fake binary.
/// Acquire `runner::TEST_BIN_LOCK` before setting to serialise tests.
#[cfg(test)]
pub(crate) static TEST_ENGINE_BIN: parking_lot::Mutex<Option<String>> =
    parking_lot::Mutex::new(None);

/// Set (or clear) the fake engine binary used in tests.
#[cfg(all(test, unix))]
pub(crate) fn set_test_engine_bin(bin: Option<&str>) {
    *TEST_ENGINE_BIN.lock() = bin.map(str::to_string);
}

/// Run a fake binary in place of the real engine CLI. The binary receives the
/// prompt as its first argument and its stdout is parsed as Claude-style JSON:
/// `{"type":"result","session_id":"...","result":"...","is_error":false}`.
/// Unknown shapes fall back to treating stdout as the plain reply text.
#[cfg(test)]
pub(crate) async fn run_fake(
    bin: &str,
    session: BotSession,
    prompt: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
) -> Result<TurnResult, String> {
    let mut cmd = tokio::process::Command::new(bin);
    // Forward session_id via arg so the fake script can detect --resume.
    if let Some(ref sid) = session.session_id {
        cmd.arg("--resume").arg(sid);
    }
    cmd.arg(prompt);
    cmd.current_dir(work_dir).kill_on_drop(true);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("fake engine: {e}"))?;
    if !out.status.success() {
        return Err(format!("fake engine exited {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let trimmed = text.trim();
    let v: serde_json::Value =
        serde_json::from_str(trimmed).unwrap_or(serde_json::json!({ "result": trimmed }));
    let reply = v["result"].as_str().unwrap_or(trimmed).to_string();
    let session_id = v["session_id"].as_str().map(str::to_string);
    let new_session = BotSession {
        session_id,
        ..session
    };
    Ok(TurnResult {
        reply,
        session: new_session,
        usage: None,
    })
}
