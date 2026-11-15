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

use crate::crew::store::{CrewmateSession, EngineConfig, UsageRecord};

/// 2-hour hard timeout per crewmate task (mirrors runner::TASK_TIMEOUT).
const TASK_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// The turn timeout: [`TASK_TIMEOUT`], or what a test shortened it to (`TEST_TURN_TIMEOUT`).
pub(super) fn turn_timeout() -> Duration {
    #[cfg(test)]
    if let Some(d) = *TEST_TURN_TIMEOUT.lock() {
        return d;
    }
    TASK_TIMEOUT
}

/// Test-only: a shortened [`TASK_TIMEOUT`], so a test can watch a turn time out.
#[cfg(test)]
pub(crate) static TEST_TURN_TIMEOUT: parking_lot::Mutex<Option<Duration>> =
    parking_lot::Mutex::new(None);

/// Test-only: the program an engine module runs in place of its CLI (`claude`, `codex`, `opencode`,
/// `omp`), by engine kind. Unlike `TEST_ENGINE_BIN` the engine module itself still runs.
#[cfg(test)]
pub(crate) static TEST_ENGINE_PROGRAM: parking_lot::Mutex<Option<(String, String)>> =
    parking_lot::Mutex::new(None);

/// The program `kind`'s module starts: its CLI name, or the test override.
pub(super) fn program(kind: &str) -> String {
    #[cfg(test)]
    if let Some((k, bin)) = TEST_ENGINE_PROGRAM.lock().clone()
        && k == kind
    {
        return bin;
    }
    kind.to_string()
}

/// Whether a crewmate's engine can pause for the user's approval of a tool call. `claude` asks
/// through its permission-prompt tool and `omp` through the approval extension; `codex exec` and
/// `opencode run --auto` have no approval channel here, so they only ever run unattended.
pub(crate) fn can_ask(kind: &str) -> bool {
    matches!(kind, "claude" | "omp")
}

/// A turn's cancel handle: whoever holds a clone can end the turn (`fire`), and the turn sees it
/// (`fired`). One per running task, kept in `CrewHub::turn_cancels` while the turn runs.
#[derive(Clone)]
pub(crate) struct TurnCancel(tokio::sync::watch::Sender<bool>);

impl TurnCancel {
    pub(crate) fn new() -> Self {
        Self(tokio::sync::watch::channel(false).0)
    }

    /// End the turn: its process tree is killed.
    pub(crate) fn fire(&self) {
        self.0.send_replace(true);
    }

    /// Resolves once [`fire`](Self::fire) was called (also when it was before this call).
    pub(super) async fn fired(&self) {
        let mut rx = self.0.subscribe();
        let _ = rx.wait_for(|fired| *fired).await;
    }
}

/// An engine CLI running one turn. It leads its own process group (Unix), and the whole tree is
/// killed when this is dropped: a turn future dropped by a timeout or a cancel can never leave the
/// agent (or the tools it started) running and spending.
pub(super) struct TurnProc {
    child: tokio::process::Child,
    pid: Option<u32>,
}

impl TurnProc {
    /// Start `cmd` as a turn: own process group, tree killed on drop. Pipes are the caller's.
    pub(super) fn spawn(cmd: &mut tokio::process::Command) -> std::io::Result<Self> {
        #[cfg(unix)]
        cmd.process_group(0);
        let child = cmd.kill_on_drop(true).spawn()?;
        let pid = child.id();
        Ok(Self { child, pid })
    }

    pub(super) fn child(&mut self) -> &mut tokio::process::Child {
        &mut self.child
    }

    /// Kill the process and everything it started, and reap it.
    pub(super) async fn kill(&mut self) {
        self.kill_tree();
        let _ = self.child.wait().await;
    }

    fn kill_tree(&self) {
        let Some(pid) = self.pid else { return };
        #[cfg(unix)]
        {
            // SAFETY: `process_group(0)` made the child's pid its group id, so `-pid` is exactly this
            // turn's group (a tool that left it with setsid is not reached, same as the pty). `pid`
            // came from our own spawn, positive and in i32 range.
            if pid <= i32::MAX as u32 {
                unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
            }
        }
        #[cfg(windows)]
        atlas_core::sys::kill_tree(pid);
    }

    /// Wait for the process's output until it exits, `cancel` fires or the turn timeout passes; the
    /// tree is killed in the last two cases.
    pub(super) async fn output(
        mut self,
        cancel: &TurnCancel,
        label: &str,
    ) -> Result<std::process::Output, String> {
        let outcome = tokio::select! {
            r = wait_with_output(&mut self.child) => Ok(r),
            () = cancel.fired() => Err(format!("{label}: cancelled")),
            () = tokio::time::sleep(turn_timeout()) => Err(format!("{label}: timed out after 2 h")),
        };
        match outcome {
            Ok(r) => {
                // The CLI is done, but a tool it left behind must not outlive the turn.
                self.kill_tree();
                r.map_err(|e| format!("{label}: spawn failed: {e}"))
            }
            Err(e) => {
                self.kill().await;
                Err(e)
            }
        }
    }
}

impl Drop for TurnProc {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

/// `Child::wait_with_output` without consuming the child, so a cancelled wait can still kill it:
/// stdout and stderr are read to the end while the process is waited for.
async fn wait_with_output(
    child: &mut tokio::process::Child,
) -> std::io::Result<std::process::Output> {
    use tokio::io::AsyncReadExt;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let (out, err) = tokio::join!(
        async {
            let mut b = Vec::new();
            if let Some(p) = stdout.as_mut() {
                p.read_to_end(&mut b).await?;
            }
            std::io::Result::Ok(b)
        },
        async {
            let mut b = Vec::new();
            if let Some(p) = stderr.as_mut() {
                p.read_to_end(&mut b).await?;
            }
            std::io::Result::Ok(b)
        }
    );
    let status = child.wait().await?;
    Ok(std::process::Output {
        status,
        stdout: out?,
        stderr: err?,
    })
}

/// Result of one conversational turn.
pub struct TurnResult {
    /// The engine's reply text.
    pub reply: String,
    /// Updated session state to persist for the next turn.
    pub session: CrewmateSession,
    /// Token/cost usage, only set when the engine reports it.
    pub usage: Option<UsageRecord>,
}

/// Run one turn of conversation for the given engine.
///
/// * `engine`     — engine kind + optional model/provider/base_url
/// * `session`    — prior session state (empty for first turn)
/// * `prompt`     — the full prompt text to send
/// * `crewmate_dir`    — `<store>/crew/<id>/` — scratch space (mcp config, codex output file, …)
/// * `crewmate_id`     — crewmate identifier, forwarded to the MCP server
/// * `hook_port`  — running hook server port
/// * `hook_token` — running hook server token
/// * `pane_key`   — PTY pane key for permission prompts (`crew:<id>:<task_id>`)
/// * `extra_env`  — crewmate-scoped secrets, forwarded as subprocess env vars
/// * `work_dir`   — working directory for the subprocess
/// * `yolo`       — when true, skip interactive permission prompts (auto-approve)
/// * `tools`      — told of every finished tool call, for engines whose stream carries them (omp RPC)
/// * `cancel`     — fires when the task is cancelled or the crew paused: the turn's process tree ends
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    engine: &EngineConfig,
    session: CrewmateSession,
    prompt: &str,
    crewmate_dir: &Path,
    crewmate_id: &str,
    hook_port: u16,
    hook_token: &str,
    pane_key: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
    yolo: bool,
    tools: Option<&ToolSink<'_>>,
    cancel: &TurnCancel,
) -> Result<TurnResult, String> {
    // Stamp created_at on first turn.
    let session = if session.created_at == 0 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        CrewmateSession {
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
        return run_fake(&bin, session, prompt, extra_env, work_dir, cancel).await;
    }

    match engine.kind.as_str() {
        "claude" => {
            claude::run(
                engine,
                session,
                prompt,
                crewmate_dir,
                crewmate_id,
                hook_port,
                hook_token,
                pane_key,
                extra_env,
                work_dir,
                yolo,
                cancel,
            )
            .await
        }
        "codex" => {
            codex::run(
                engine,
                session,
                prompt,
                crewmate_dir,
                extra_env,
                work_dir,
                cancel,
            )
            .await
        }
        "opencode" => opencode::run(engine, session, prompt, extra_env, work_dir, cancel).await,
        "omp" => {
            omp::run(
                engine,
                session,
                prompt,
                extra_env,
                work_dir,
                crewmate_dir,
                hook_port,
                hook_token,
                pane_key,
                yolo,
                tools,
                cancel,
            )
            .await
        }
        other => Err(format!("unknown engine kind '{other}'")),
    }
}

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// A finished tool call of a turn: `(tool name, its arguments, what it printed, whether it failed)`.
pub(crate) type ToolSink<'a> = dyn Fn(&str, &serde_json::Value, &str, bool) + Send + Sync + 'a;

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
    session: CrewmateSession,
    prompt: &str,
    extra_env: &BTreeMap<String, String>,
    work_dir: &str,
    cancel: &TurnCancel,
) -> Result<TurnResult, String> {
    let mut cmd = tokio::process::Command::new(bin);
    // Forward session_id via arg so the fake script can detect --resume.
    if let Some(ref sid) = session.session_id {
        cmd.arg("--resume").arg(sid);
    }
    cmd.arg(prompt);
    cmd.current_dir(work_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let out = TurnProc::spawn(&mut cmd)
        .map_err(|e| format!("fake engine: {e}"))?
        .output(cancel, "fake engine")
        .await?;
    if !out.status.success() {
        return Err(format!("fake engine exited {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let trimmed = text.trim();
    let v: serde_json::Value =
        serde_json::from_str(trimmed).unwrap_or(serde_json::json!({ "result": trimmed }));
    let reply = v["result"].as_str().unwrap_or(trimmed).to_string();
    let session_id = v["session_id"].as_str().map(str::to_string);
    let new_session = CrewmateSession {
        session_id,
        ..session
    };
    Ok(TurnResult {
        reply,
        session: new_session,
        usage: None,
    })
}
