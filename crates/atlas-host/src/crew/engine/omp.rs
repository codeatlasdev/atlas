//! omp engine — RPC mode.
//!
//! Protocol (per-turn):
//! 1. Spawn `omp --mode rpc [--auto-approve | --extension <path>] [--resume <session.jsonl>]`
//!    with piped stdin/stdout.  When `yolo` (approval == Auto) we pass `--auto-approve`.
//!    When `!yolo` (approval == Ask) we write a TypeScript extension to `<crewmate_dir>/omp-approval.ts`
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

use crate::crew::store::{CrewmateSession, UsageRecord};

use super::{TurnCtx, TurnResult, launch, program, turn_timeout};

/// Settings every crewmate's omp runs with, as a `--config` overlay. A crew server is a headless server: omp's sleep
/// prevention asks systemd-logind over D-Bus, which a server or container often lacks, and every start then waited
/// 25 s for the D-Bus timeout (seen on atlas-test-ship: first step 33 s in, the reply summary always timed out).
const CREW_OVERLAY: &str = "power:\n  sleepPrevention: \"off\"\n";

/// `--config <file>` for an omp started by a crewmate; the file sits in the crewmate's dir and is rewritten when it differs.
pub(crate) fn overlay_args(crewmate_dir: &std::path::Path) -> Vec<String> {
    let path = crewmate_dir.join("omp-crew.yml");
    // tree-access: hostd's own engine files in ctx.crewmate_dir (crew-runtime under isolation, which the crewmate cannot write)
    if std::fs::read_to_string(&path).ok().as_deref() != Some(CREW_OVERLAY) {
        // tree-access: hostd's own engine files in ctx.crewmate_dir (crew-runtime under isolation, which the crewmate cannot write)
        let _ = std::fs::write(&path, CREW_OVERLAY);
    }
    vec!["--config".to_string(), path.to_string_lossy().into_owned()]
}

/// What ended an omp turn: the RPC itself (a reply, or the stream failing), or hostd (cancel, timeout).
enum End<T> {
    Rpc(Result<T, String>),
    Stopped(&'static str),
}

type RpcOut = (String, Option<String>, Option<UsageRecord>);

impl End<RpcOut> {
    /// The process died under the RPC: the stream ended without a reply, or writing/reading it failed (the
    /// kernel can kill omp before hostd even writes the prompt: "Broken pipe"). Only then may the end be a
    /// memory kill (#296) to report as such. A cancel or a timeout is hostd's own doing, never an OOM.
    fn process_died(&self) -> bool {
        match self {
            End::Rpc(Ok((reply, ..))) => reply.is_empty(),
            End::Rpc(Err(_)) => true,
            End::Stopped(_) => false,
        }
    }
}

pub(super) async fn run(ctx: TurnCtx<'_>) -> Result<TurnResult, String> {
    let mut cmd = ctx.command(&program("omp"));
    cmd.arg("--mode")
        .arg("rpc")
        .args(overlay_args(ctx.crewmate_dir));

    if ctx.yolo {
        cmd.arg("--auto-approve");
    } else {
        // Write the approval extension and load it; it reaches hostd through the ATLAS_* env.
        let ext_path = ctx.crewmate_dir.join("omp-approval.ts");
        write_approval_extension(&ext_path, ctx.hook_port, ctx.hook_token, ctx.pane_key)?;
        cmd.arg("--extension").arg(&ext_path);
    }

    if let Some(session_file) = &ctx.session.session_file {
        cmd.arg("--resume").arg(session_file);
    }

    let launch::Rpc {
        mut writer,
        reader,
        guard,
        oom,
    } = launch::rpc(ctx.exec, cmd, "omp").await?;

    let end = tokio::select! {
        r = drive_rpc(&mut writer, reader, ctx.prompt, ctx.tools) => End::Rpc(r),
        () = ctx.cancel.fired() => End::Stopped("cancelled"),
        () = tokio::time::sleep(turn_timeout()) => End::Stopped("timed out after 2 h"),
    };
    let died = end.process_died();
    if died
        && let Some(oom) = oom
        && let Ok(Some(cap)) = tokio::task::spawn_blocking(oom).await
    {
        drop(guard);
        return Err(format!(
            "omp: {}",
            crate::crew::spawn::cgroup::oom_reason(cap)
        ));
    }
    let result = match end {
        End::Rpc(r) => r.map_err(|e| format!("omp: {e}")),
        End::Stopped(why) => Err(format!("omp: {why}")),
    };

    // Always end the whole tree: the process is not expected to exit on its own, and the tools it
    // started must not outlive the turn.
    drop(guard);

    let (reply, new_session_file, usage) = result?;

    Ok(TurnResult {
        reply,
        session: CrewmateSession {
            session_id: None,
            session_file: new_session_file.or(ctx.session.session_file),
            created_at: ctx.session.created_at,
        },
        usage,
    })
}

/// Send the prompt, collect the reply, obtain the session file path.
async fn drive_rpc(
    stdin: &mut (impl tokio::io::AsyncWrite + Unpin),
    stdout: impl tokio::io::AsyncRead + Unpin,
    prompt: &str,
    tools: Option<&super::ToolSink<'_>>,
) -> Result<(String, Option<String>, Option<UsageRecord>), String> {
    let mut lines = BufReader::new(stdout).lines();
    // Arguments of the tool calls that started, by call id: the end event carries the output only.
    let mut started: BTreeMap<String, (String, Value)> = BTreeMap::new();

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
            Some("tool_execution_start") => {
                let id = val["toolCallId"].as_str().unwrap_or_default().to_string();
                let name = val["toolName"].as_str().unwrap_or_default().to_string();
                started.insert(id, (name, val.get("args").cloned().unwrap_or(Value::Null)));
            }
            Some("tool_execution_end") => {
                let id = val["toolCallId"].as_str().unwrap_or_default();
                if let (Some(sink), Some((name, args))) = (tools, started.remove(id)) {
                    sink(
                        &name,
                        &args,
                        &tool_result_text(&val["result"]),
                        val["isError"].as_bool() == Some(true),
                    );
                }
            }
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
/// The text of a tool result: a string, or the text blocks of `{content: [{type: "text", text}]}`.
fn tool_result_text(result: &Value) -> String {
    match result {
        Value::String(s) => s.clone(),
        v => v["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
    }
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
        // tree-access: hostd's own engine files in ctx.crewmate_dir (crew-runtime under isolation, which the crewmate cannot write)
        std::fs::create_dir_all(parent).map_err(|e| format!("omp extension dir: {e}"))?;
    }
    // tree-access: hostd's own engine files in ctx.crewmate_dir (crew-runtime under isolation, which the crewmate cannot write)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // #385 CI (plain Linux): the kernel killed the fake omp before hostd wrote the prompt, the write failed with
    // "Broken pipe", and the turn reported the pipe error instead of the memory cap, because only an empty
    // reply was checked for an OOM. Every way the process can die under the RPC is checked now; hostd's own
    // stops never are (a cancel during a memory spike is still a cancel).
    #[test]
    fn every_rpc_death_is_checked_for_a_memory_kill_and_hostds_own_stops_never_are() {
        let out = |reply: &str| -> RpcOut { (reply.to_string(), None, None) };
        assert!(End::Rpc(Err("write prompt: Broken pipe (os error 32)".into())).process_died());
        assert!(End::Rpc(Err("flush: Broken pipe".into())).process_died());
        assert!(
            End::Rpc(Ok(out(""))).process_died(),
            "stream ended without a reply"
        );
        assert!(
            !End::Rpc(Ok(out("done"))).process_died(),
            "a reply is a reply"
        );
        assert!(!End::<RpcOut>::Stopped("cancelled").process_died());
        assert!(!End::<RpcOut>::Stopped("timed out after 2 h").process_died());
    }

    /// An omp RPC stream as the protocol sends it (same shapes as `apps/desktop/src/lib/rpc.test.ts`): a bash call
    /// whose result is text blocks, a read whose result is a plain string, and one failed call.
    const STREAM: &str = concat!(
        r#"{"type":"tool_execution_start","toolCallId":"c1","toolName":"bash","args":{"command":"cargo test"}}"#,
        "\n",
        r#"{"type":"tool_execution_end","toolCallId":"c1","toolName":"bash","result":{"content":[{"type":"text","text":"test result: ok. 3 passed"}]},"isError":false}"#,
        "\n",
        r#"{"type":"tool_execution_start","toolCallId":"c2","toolName":"read","args":{"path":"a.rs"}}"#,
        "\n",
        r#"{"type":"tool_execution_end","toolCallId":"c2","toolName":"read","result":"fn main() {}","isError":false}"#,
        "\n",
        r#"{"type":"tool_execution_start","toolCallId":"c3","toolName":"bash","args":{"command":"cargo clippy"}}"#,
        "\n",
        r#"{"type":"tool_execution_end","toolCallId":"c3","toolName":"bash","result":"error: boom","isError":true}"#,
        "\n",
        r#"{"type":"agent_end","messages":[{"role":"assistant","content":"Done. All tests pass."}]}"#,
        "\n",
        r#"{"type":"prompt_result","id":"r1"}"#,
        "\n",
        r#"{"type":"response","id":"g","data":{"sessionFile":"/s.jsonl"}}"#,
        "\n",
    );

    #[tokio::test]
    async fn the_rpc_stream_reports_every_finished_tool_with_its_output() {
        let seen: Mutex<Vec<(String, Value, String, bool)>> = Mutex::new(vec![]);
        let sink = |tool: &str, args: &Value, out: &str, err: bool| {
            seen.lock()
                .unwrap()
                .push((tool.into(), args.clone(), out.into(), err));
        };
        let mut stdin = tokio::io::sink();
        let (reply, session_file, _) = drive_rpc(&mut stdin, STREAM.as_bytes(), "go", Some(&sink))
            .await
            .unwrap();
        assert_eq!(reply, "Done. All tests pass.");
        assert_eq!(session_file.as_deref(), Some("/s.jsonl"));
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].0, "bash");
        assert_eq!(seen[0].1["command"], "cargo test");
        assert_eq!(seen[0].2, "test result: ok. 3 passed");
        assert!(!seen[0].3);
        assert_eq!(seen[1].2, "fn main() {}");
        assert!(seen[2].3, "an isError result is reported as one");
    }

    // The same stream into the crew's tape: the passing test run proves the claim, the failed call does not.
    #[tokio::test]
    async fn a_claim_is_proven_by_an_rpc_stream_and_not_by_a_failed_call() {
        use crate::crew::runner::{Tape, guard_strip_unproven};
        let tape = Tape::default();
        let sink = |tool: &str, args: &Value, out: &str, err: bool| {
            tape.record_finished("t1", tool, args, out, err)
        };
        let mut stdin = tokio::io::sink();
        drive_rpc(&mut stdin, STREAM.as_bytes(), "go", Some(&sink))
            .await
            .unwrap();
        let ran = tape.of("t1");
        assert_eq!(ran.len(), 2, "the read is no command: {ran:?}");
        let (kept, removed) = guard_strip_unproven("All tests pass.", &ran);
        assert_eq!(kept, "All tests pass.");
        assert!(removed.is_empty());
        // Only the failed run: nothing proves it.
        let (kept, removed) = guard_strip_unproven("All tests pass.", &ran[1..]);
        assert!(kept.is_empty() && removed.len() == 1);
    }
}

#[cfg(all(test, unix))]
mod kill_tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::crew::engine::TurnCancel;
    use crate::crew::store::EngineConfig;

    fn gone(pid: u32) -> bool {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => stat
                .rsplit(')')
                .next()
                .is_some_and(|r| r.trim_start().starts_with('Z')),
            Err(_) => !atlas_core::sys::pid_alive(pid),
        }
    }

    // #290: omp killed only its direct child, so the tool process omp had started (here a `bash` call's
    // `sleep`, a grandchild) outlived the turn. A fake omp speaks the RPC (a tool call starts, and
    // never ends), starts that tool process, and the turn is cancelled: nothing may be left.
    #[tokio::test]
    async fn cancelling_an_omp_turn_also_kills_the_tool_process_omp_started() {
        let tmp = tempfile::tempdir().unwrap();
        let pidfile = tmp.path().join("tool.pid");
        let bin = tmp.path().join("omp");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nread -r _prompt\n\
                 printf '{{\"type\":\"tool_execution_start\",\"toolCallId\":\"c1\",\"toolName\":\"bash\",\"args\":{{}}}}\\n'\n\
                 (sleep 60 & echo $! > \"{}\"; wait) &\nwait\n",
                pidfile.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let _lock = crate::crew::runner::TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
            Some(("omp".into(), bin.to_string_lossy().into_owned()));
        let cancel = TurnCancel::new();
        let turn = {
            let (dir, cancel) = (tmp.path().to_path_buf(), cancel.clone());
            tokio::spawn(async move {
                let engine = EngineConfig {
                    kind: "omp".into(),
                    provider: None,
                    base_url: None,
                    model: None,
                };
                let env = BTreeMap::new();
                run(TurnCtx {
                    engine: &engine,
                    session: CrewmateSession::default(),
                    prompt: "go",
                    crewmate_dir: &dir,
                    crewmate_id: "c1",
                    hook_port: 0,
                    hook_token: "",
                    pane_key: "",
                    extra_env: &env,
                    work_dir: dir.to_str().unwrap(),
                    yolo: true,
                    tools: None,
                    cancel: &cancel,
                    exec: &crate::crew::engine::Exec::local(),
                })
                .await
            })
        };
        let start = Instant::now();
        let tool = loop {
            if let Ok(s) = std::fs::read_to_string(&pidfile)
                && let Ok(pid) = s.trim().parse::<u32>()
            {
                break pid;
            }
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "omp never ran its tool"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert!(!gone(tool));

        cancel.fire();
        let err = turn
            .await
            .unwrap()
            .err()
            .expect("a cancelled turn is an error");
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
        assert!(err.contains("cancel"), "{err}");
        let start = Instant::now();
        while !gone(tool) && start.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            gone(tool),
            "the tool process {tool} outlived the cancelled omp turn"
        );
    }
}
