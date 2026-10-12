//! `atlas hook <harness> <event>` — the agent-side hook behind `atlas setup claude-code|codex`.
//!
//! The harness runs this as a command hook: the event JSON arrives on stdin, the
//! harness-specific JSON response goes to stdout. It never fails (any problem = no
//! output, exit 0, i.e. "no opinion"), never touches the network and only reads local
//! files, mirroring the OMP hook (`setup/atlas_hook.ts`):
//!
//! 1. `session-start`: inside an Atlas project (an `atlas.yaml` in the session's cwd or
//!    a parent) inject `atlas dev --status` as context, and tell the agent never to run
//!    `atlas dev` itself.
//! 2. `pre-tool-use`: deny `Bash` commands that would start the interactive `atlas dev` TUI.
//!
//! Wire formats (both verified against the primary docs and identical for these events):
//! * Claude Code: SessionStart `hookSpecificOutput.additionalContext`
//!   (<https://code.claude.com/docs/en/hooks#sessionstart-decision-control>), PreToolUse
//!   `hookSpecificOutput.permissionDecision = "deny"` + `permissionDecisionReason`
//!   (<https://code.claude.com/docs/en/hooks#pretooluse-decision-control>).
//! * Codex: same shapes (<https://developers.openai.com/codex/hooks#sessionstart>,
//!   <https://developers.openai.com/codex/hooks#pretooluse>).

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Args, ValueEnum};
use serde_json::{Value, json};

use super::AtlasCommand;

const MAX_STATUS_LINES: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Harness {
    #[value(name = "claude-code")]
    ClaudeCode,
    Codex,
}

impl Harness {
    /// The name used on the command line and inside registered hook commands.
    pub fn name(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude-code",
            Harness::Codex => "codex",
        }
    }

    /// Agent id in Atlas Desktop's registry.
    pub fn agent_id(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude",
            Harness::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Event {
    #[value(name = "session-start")]
    SessionStart,
    #[value(name = "pre-tool-use")]
    PreToolUse,
    #[value(name = "user-prompt-submit")]
    UserPromptSubmit,
    #[value(name = "post-tool-use")]
    PostToolUse,
    Stop,
    /// Claude Code: permission prompts and idle notices.
    Notification,
    /// Codex: an approval is pending.
    #[value(name = "permission-request")]
    PermissionRequest,
    /// Claude Code: the turn ended with an API error (auth, rate limit…).
    #[value(name = "stop-failure")]
    StopFailure,
    /// Claude Code: the session closed.
    #[value(name = "session-end")]
    SessionEnd,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Event::SessionStart => "session-start",
            Event::PreToolUse => "pre-tool-use",
            Event::UserPromptSubmit => "user-prompt-submit",
            Event::PostToolUse => "post-tool-use",
            Event::Stop => "stop",
            Event::Notification => "notification",
            Event::PermissionRequest => "permission-request",
            Event::StopFailure => "stop-failure",
            Event::SessionEnd => "session-end",
        }
    }

    /// `hookEventName` inside `hookSpecificOutput` (identical for both harnesses).
    fn json_name(self) -> &'static str {
        match self {
            Event::SessionStart => "SessionStart",
            Event::PreToolUse => "PreToolUse",
            Event::UserPromptSubmit => "UserPromptSubmit",
            Event::PostToolUse => "PostToolUse",
            Event::Stop => "Stop",
            Event::Notification => "Notification",
            Event::PermissionRequest => "PermissionRequest",
            Event::StopFailure => "StopFailure",
            Event::SessionEnd => "SessionEnd",
        }
    }
}

#[derive(Debug, Args)]
pub struct HookArgs {
    /// Agent that invoked the hook
    #[arg(value_enum)]
    pub harness: Harness,
    /// Hook event to handle (event JSON on stdin)
    #[arg(value_enum)]
    pub event: Event,
}

impl AtlasCommand for HookArgs {
    async fn run(self) -> Result<()> {
        let mut raw = String::new();
        // Unreadable stdin = no input = no opinion.
        let _ = std::io::stdin().take(1 << 20).read_to_string(&mut raw);
        report_status(self.harness, self.event, &raw);
        if let Some(out) = respond(self.harness, self.event, &raw) {
            println!("{out}");
        }
        Ok(())
    }
}

/// The response for one hook invocation, or `None` for "no output". Pure over the
/// filesystem state it reads (`atlas.yaml`, `package.json`, the status file).
pub fn respond(harness: Harness, event: Event, stdin: &str) -> Option<String> {
    let input: Value = serde_json::from_str(stdin).ok()?;
    let cwd = input
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let body = match event {
        Event::SessionStart => session_context(&cwd)?,
        Event::PreToolUse => pre_tool_use_denial(&input, &cwd)?,
        // Status-only events: no opinion for the harness.
        _ => return None,
    };
    Some(wire(harness, event, &body))
}

/// Status event for Atlas Desktop's hook server, in the vocabulary its
/// `normalize` understands. `None` = not a status change worth sending.
pub fn status_payload(event: Event, input: &Value) -> Option<Value> {
    let s = |k: &str| input.get(k).and_then(Value::as_str);
    let name = match event {
        Event::SessionStart => "session_start",
        Event::UserPromptSubmit => "user_prompt_submit",
        Event::PreToolUse => "pre_tool_use",
        Event::PostToolUse => "post_tool_use",
        Event::Stop | Event::StopFailure => "stop",
        Event::SessionEnd => "session_end",
        Event::PermissionRequest => "permission_request",
        // Claude sends Notification for permission prompts *and* idle reminders;
        // only the former means "blocked".
        Event::Notification => {
            let m = s("message").unwrap_or("").to_ascii_lowercase();
            if !m.contains("permission") && !m.contains("approv") {
                return None;
            }
            "permission_request"
        }
    };
    let mut p = serde_json::json!({ "event": name });
    if let Some(v) = s("session_id") {
        p["session_id"] = v.into();
    }
    if let Some(v) = s("transcript_path") {
        p["session_file"] = v.into();
    }
    if let Some(v) = s("prompt") {
        p["prompt"] = v.into();
    }
    if let Some(v) = s("tool_name") {
        p["tool_name"] = v.into();
    }
    if let Some(v) = s("last_assistant_message")
        .or(s("error_details"))
        .or(s("error"))
    {
        p["text"] = v.into();
    }
    Some(p)
}

/// POST the event to Atlas Desktop when the agent runs in one of its terminals
/// (`ATLAS_HOOK_ENDPOINT` set). Loopback only, 1 s timeout, never fails the hook.
fn report_status(harness: Harness, event: Event, stdin: &str) {
    let Ok(endpoint) = std::env::var("ATLAS_HOOK_ENDPOINT") else {
        return;
    };
    let Ok(input) = serde_json::from_str::<Value>(stdin) else {
        return;
    };
    let Some(payload) = status_payload(event, &input) else {
        return;
    };
    let Ok(file) = std::fs::read_to_string(endpoint) else {
        return;
    };
    let get = |k: &str| {
        file.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(str::trim)
            .map(str::to_string)
    };
    let (Some(port), Some(token)) = (get("ATLAS_HOOK_PORT"), get("ATLAS_HOOK_TOKEN")) else {
        return;
    };
    let env = |k: &str| std::env::var(k).ok();
    let body = serde_json::json!({
        "pane_key": env("ATLAS_PANE_KEY"),
        "launch_token": env("ATLAS_LAUNCH_TOKEN"),
        "tab_id": env("ATLAS_TAB_ID"),
        "worktree_id": env("ATLAS_WORKTREE_ID"),
        "agent": harness.agent_id(),
        "payload": payload,
    })
    .to_string();
    let req = format!(
        "POST /hook/{} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nX-Atlas-Hook-Token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        harness.agent_id(),
        body.len()
    );
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return;
    };
    let timeout = std::time::Duration::from_secs(1);
    if let Ok(mut s) = std::net::TcpStream::connect_timeout(&addr, timeout) {
        use std::io::Write;
        let _ = s.set_read_timeout(Some(timeout));
        let _ = s.write_all(req.as_bytes());
        let _ = s.read(&mut [0u8; 64]);
    }
}

/// Harness-specific JSON for a context message (session-start) or a denial (pre-tool-use).
fn wire(harness: Harness, event: Event, text: &str) -> String {
    // Claude Code and Codex accept the same `hookSpecificOutput` shape for these two
    // events, so there is one encoding; the match keeps each harness an explicit decision
    // if their schemas ever diverge.
    let out = match (harness, event) {
        (Harness::ClaudeCode | Harness::Codex, Event::SessionStart) => json!({
            "hookSpecificOutput": {
                "hookEventName": event.json_name(),
                "additionalContext": text,
            }
        }),
        (Harness::ClaudeCode | Harness::Codex, Event::PreToolUse) => json!({
            "hookSpecificOutput": {
                "hookEventName": event.json_name(),
                "permissionDecision": "deny",
                "permissionDecisionReason": text,
            }
        }),
        // Unreachable: `respond` returns before `wire` for status-only events.
        (_, _) => return String::new(),
    };
    out.to_string()
}

fn find_root(start: &Path) -> Option<PathBuf> {
    atlas_tui::config::find_root(start)
}

/// Session-start context for an Atlas project, `None` outside one.
fn session_context(cwd: &Path) -> Option<String> {
    let root = find_root(cwd)?;
    Some(match atlas_tui::status::live_status(&root) {
        Ok(table) => {
            let lines: Vec<&str> = table.trim().lines().collect();
            let shown = if lines.len() > MAX_STATUS_LINES {
                let mut v: Vec<String> = lines[..MAX_STATUS_LINES]
                    .iter()
                    .map(|l| (*l).to_string())
                    .collect();
                v.push(format!("… ({} more lines)", lines.len() - MAX_STATUS_LINES));
                v.join("\n")
            } else {
                lines.join("\n")
            };
            format!(
                "atlas dev is RUNNING for {}:\n{shown}\nUse the atlas MCP tools (atlas_status, atlas_logs, atlas_restart, …) to inspect or control it. Never run `atlas dev` yourself.",
                root.display()
            )
        }
        Err(_) => format!(
            "atlas dev is NOT running for {}. If you need the stack, ask the user to start it (`atlas dev`). Never run `atlas dev` yourself; the atlas MCP tools and `atlas dev --status` are safe.",
            root.display()
        ),
    })
}

/// Denial reason when this PreToolUse input is a `Bash` call that starts the TUI.
fn pre_tool_use_denial(input: &Value, cwd: &Path) -> Option<String> {
    if input.get("tool_name").and_then(Value::as_str) != Some("Bash") {
        return None;
    }
    let tool_input = input.get("tool_input")?;
    let command = tool_input.get("command").and_then(Value::as_str)?;
    let run_dir = tool_input
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| cwd.to_path_buf(), PathBuf::from);
    block_reason(command, &run_dir)
}

const REASON_ATLAS_DEV: &str = "atlas dev is an interactive TUI and must be started by the user. Use the atlas MCP tools (atlas_status, atlas_logs, atlas_restart, …) or `atlas dev --status`. If it is not running, ask the user to start it.";
const REASON_PKG_DEV: &str = "This project's dev script runs `atlas dev` (interactive TUI) — the user starts it, not you. Use the atlas MCP tools or `atlas dev --status` to inspect it, and ask the user to start it if it is down.";

/// Block reason when `command` (run from `cwd`) would start the interactive stack.
/// Only commands inside an Atlas project (atlas.yaml in `cwd` or a parent) are judged.
pub fn block_reason(command: &str, cwd: &Path) -> Option<String> {
    let root = find_root(cwd)?;
    for raw in split_commands(command) {
        let segment = strip_env_assignments(raw.trim());
        if let Some(args) = atlas_dev_args(segment)
            && !has_safe_flag(args)
        {
            return Some(REASON_ATLAS_DEV.to_string());
        }
        if is_package_dev(segment) && dev_script_is_atlas_dev(&root) {
            return Some(REASON_PKG_DEV.to_string());
        }
    }
    None
}

/// Split on `&&`, `||`, `;` and newlines (a single `|` or `&` does not separate).
fn split_commands(command: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, bytes) = (0, command.as_bytes());
    let mut i = 0;
    while i < bytes.len() {
        let sep = match bytes[i] {
            b';' | b'\n' => 1,
            b'&' | b'|' if bytes.get(i + 1) == Some(&bytes[i]) => 2,
            _ => 0,
        };
        if sep > 0 {
            parts.push(&command[start..i]);
            i += sep;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(&command[start..]);
    parts
}

/// Drop leading `VAR=value ` assignments (`\w+=\S*` each).
fn strip_env_assignments(mut s: &str) -> &str {
    loop {
        let Some(end) = s.find(char::is_whitespace) else {
            return s;
        };
        let word = &s[..end];
        let Some((name, _)) = word.split_once('=') else {
            return s;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return s;
        }
        s = s[end..].trim_start();
    }
}

/// `atlas dev <args>` / `/path/to/atlas dev <args>` / `bunx|npx atlas dev <args>`:
/// the argument string after `dev`. `None` when the segment is not that command, or when
/// the arguments contain a lone `&` or `|` (a pipeline/background: not judged, as before).
fn atlas_dev_args(segment: &str) -> Option<&str> {
    let first = segment.split_whitespace().next()?;
    // `segment` is trimmed, so `first` is its literal prefix.
    let after_first = &segment[first.len()..];
    let after = if first == "atlas" || first.ends_with("/atlas") {
        after_first
    } else if matches!(first, "bunx" | "npx") {
        after_first.trim_start().strip_prefix("atlas")?
    } else {
        return None;
    };
    // `\s+dev` then end-of-string or whitespace + args free of `;&|`.
    let after = after.strip_prefix(char::is_whitespace)?.trim_start();
    let args = after.strip_prefix("dev")?;
    if args.is_empty() {
        return Some("");
    }
    (args.starts_with(char::is_whitespace) && !args.contains([';', '&', '|'])).then_some(args)
}

/// `--status`, `--ci`, `--headless` (also `--flag=…`), `-h`, `--help` never start the TUI.
fn has_safe_flag(args: &str) -> bool {
    args.split_whitespace().any(|t| {
        matches!(t, "--status" | "--ci" | "--headless" | "-h" | "--help")
            || ["--status=", "--ci=", "--headless="]
                .iter()
                .any(|p| t.starts_with(p))
    })
}

/// `bun|npm|pnpm|yarn [run] dev [...]`.
fn is_package_dev(segment: &str) -> bool {
    let mut it = segment.split_whitespace();
    if !matches!(it.next(), Some("bun" | "npm" | "pnpm" | "yarn")) {
        return false;
    }
    match it.next() {
        Some("dev") => true,
        Some("run") => it.next() == Some("dev"),
        _ => false,
    }
}

/// `package.json` `scripts.dev` is `atlas dev …`.
fn dev_script_is_atlas_dev(root: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join("package.json")) else {
        return false;
    };
    let Ok(pkg) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    pkg.get("scripts")
        .and_then(|s| s.get("dev"))
        .and_then(Value::as_str)
        .is_some_and(|dev| {
            let mut it = dev.split_whitespace();
            it.next() == Some("atlas") && it.next() == Some("dev")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        proj: PathBuf,
        plain: PathBuf,
    }

    fn fixture(dev_script: Option<&str>) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let (proj, plain) = (root.join("proj"), root.join("plain"));
        std::fs::create_dir_all(proj.join("a/b")).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(proj.join("atlas.yaml"), "name: demo\nservices: {}\n").unwrap();
        if let Some(d) = dev_script {
            std::fs::write(
                proj.join("package.json"),
                json!({ "scripts": { "dev": d } }).to_string(),
            )
            .unwrap();
        }
        Fixture {
            _tmp: tmp,
            proj,
            plain,
        }
    }

    fn pre(harness: Harness, command: &str, cwd: &Path) -> Option<Value> {
        let input = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": command },
            "cwd": cwd,
        });
        respond(harness, Event::PreToolUse, &input.to_string())
            .map(|s| serde_json::from_str(&s).unwrap())
    }

    #[test]
    fn denies_interactive_atlas_dev_and_allows_the_rest_for_both_harnesses() {
        let f = fixture(Some("atlas dev"));
        let cases: &[(&str, bool, bool)] = &[
            // (command, in project, expect deny)
            ("atlas dev", true, true),
            ("cd x && atlas dev", true, true),
            ("FOO=1 BAR=2 atlas dev -d .", true, true),
            ("/Users/a/.atlas/bin/atlas dev", true, true),
            ("npx atlas dev", true, true),
            ("bunx atlas dev -d .", true, true),
            ("bun run dev", true, true),
            ("bun dev", true, true),
            ("pnpm dev", true, true),
            ("npm run dev", true, true),
            ("yarn dev", true, true),
            ("echo hi; atlas dev", true, true),
            ("echo hi\natlas dev", true, true),
            ("false || atlas dev", true, true),
            ("atlas dev --status", true, false),
            ("atlas dev --ci", true, false),
            ("atlas dev --ci --timeout 5", true, false),
            ("atlas dev --headless", true, false),
            ("atlas dev --status=1", true, false),
            ("atlas dev -h", true, false),
            ("atlas dev --help", true, false),
            ("npx atlas dev --status", true, false),
            ("echo atlas dev is cool", true, false),
            ("atlas devx", true, false),
            ("atlas development", true, false),
            ("myatlas dev", true, false),
            ("atlas mcp", true, false),
            ("atlas dev | tee log", true, false),
            ("bun run build", true, false),
            ("bun run devx", true, false),
            ("npm test", true, false),
            ("ls", true, false),
            ("", true, false),
            // Outside an atlas project nothing is judged.
            ("atlas dev", false, false),
            ("bun run dev", false, false),
        ];
        for harness in [Harness::ClaudeCode, Harness::Codex] {
            for &(cmd, in_proj, deny) in cases {
                let cwd = if in_proj {
                    f.proj.join("a/b")
                } else {
                    f.plain.clone()
                };
                let got = pre(harness, cmd, &cwd);
                assert_eq!(got.is_some(), deny, "{harness:?} {cmd:?} in_proj={in_proj}");
                if let Some(v) = got {
                    let h = &v["hookSpecificOutput"];
                    assert_eq!(h["hookEventName"], "PreToolUse");
                    assert_eq!(h["permissionDecision"], "deny");
                    assert!(
                        h["permissionDecisionReason"]
                            .as_str()
                            .unwrap()
                            .contains("atlas")
                    );
                    assert_eq!(v.as_object().unwrap().len(), 1, "only hookSpecificOutput");
                }
            }
        }
    }

    #[test]
    fn package_dev_is_only_blocked_when_the_script_is_atlas_dev() {
        let plain_script = fixture(Some("vite"));
        assert!(pre(Harness::Codex, "bun run dev", &plain_script.proj).is_none());
        let none = fixture(None);
        assert!(pre(Harness::Codex, "pnpm dev", &none.proj).is_none());
        let ws = fixture(Some("  atlas   dev --timeout 9"));
        assert!(pre(Harness::ClaudeCode, "npm run dev", &ws.proj).is_some());
        // Reason differs: package-script vs direct.
        let v = pre(Harness::ClaudeCode, "npm run dev", &ws.proj).unwrap();
        assert!(
            v["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
                .contains("dev script")
        );
    }

    #[test]
    fn other_tools_and_missing_fields_are_ignored() {
        let f = fixture(Some("atlas dev"));
        let run = |v: Value| respond(Harness::ClaudeCode, Event::PreToolUse, &v.to_string());
        assert!(
            run(json!({"tool_name":"Edit","tool_input":{"command":"atlas dev"},"cwd":f.proj}))
                .is_none()
        );
        assert!(run(json!({"tool_name":"Bash","cwd":f.proj})).is_none());
        assert!(run(json!({"tool_name":"Bash","tool_input":null,"cwd":f.proj})).is_none());
        assert!(
            run(json!({"tool_name":"Bash","tool_input":{"command":42},"cwd":f.proj})).is_none()
        );
        assert!(run(json!({})).is_none());
        assert!(run(json!(null)).is_none());
        // tool_input.cwd wins over the session cwd (command runs elsewhere).
        let elsewhere = run(
            json!({"tool_name":"Bash","tool_input":{"command":"atlas dev","cwd":f.plain},"cwd":f.proj}),
        );
        assert!(elsewhere.is_none());
    }

    #[test]
    fn malformed_stdin_means_no_output() {
        for bad in ["", "   ", "not json", "{", "[1,2]"] {
            for e in [Event::SessionStart, Event::PreToolUse] {
                assert!(
                    respond(Harness::ClaudeCode, e, bad).is_none(),
                    "{bad:?} {e:?}"
                );
            }
        }
    }

    #[test]
    fn session_start_not_running_in_project_and_silent_outside() {
        let f = fixture(None);
        let input = |cwd: &Path| {
            json!({"hook_event_name":"SessionStart","source":"startup","cwd":cwd}).to_string()
        };
        for harness in [Harness::ClaudeCode, Harness::Codex] {
            let out = respond(harness, Event::SessionStart, &input(&f.proj.join("a"))).unwrap();
            let v: Value = serde_json::from_str(&out).unwrap();
            let h = &v["hookSpecificOutput"];
            assert_eq!(h["hookEventName"], "SessionStart");
            let ctx = h["additionalContext"].as_str().unwrap();
            assert!(ctx.contains("NOT running"), "{ctx}");
            assert!(ctx.contains(f.proj.to_str().unwrap()), "{ctx}");
            assert!(ctx.contains("Never run `atlas dev` yourself"));
            assert_eq!(v.as_object().unwrap().len(), 1);
            assert!(respond(harness, Event::SessionStart, &input(&f.plain)).is_none());
        }
    }

    #[test]
    fn session_start_reports_running_stack_and_truncates() {
        let f = fixture(None);
        let services: serde_json::Map<String, Value> = (0..40)
            .map(|i| {
                (
                    format!("svc{i:02}"),
                    json!({"state":"running","port":3000 + i,"health":null}),
                )
            })
            .collect();
        std::fs::create_dir_all(f.proj.join(".logs")).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Our own pid is alive, so the status file counts as a live run.
        std::fs::write(
            f.proj.join(".logs/.tui-status.json"),
            json!({"pid": std::process::id(), "updated_at": now, "services": services}).to_string(),
        )
        .unwrap();
        let out = respond(
            Harness::Codex,
            Event::SessionStart,
            &json!({"cwd": f.proj}).to_string(),
        )
        .unwrap();
        let ctx =
            serde_json::from_str::<Value>(&out).unwrap()["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .to_string();
        assert!(ctx.contains("atlas dev is RUNNING"), "{ctx}");
        assert!(ctx.contains("svc00"), "{ctx}");
        assert!(ctx.contains("more lines)"), "{ctx}");
        assert!(!ctx.contains("svc39"), "{ctx}");
    }

    #[test]
    fn split_and_assignment_helpers() {
        assert_eq!(
            split_commands("a && b || c; d\ne"),
            ["a ", " b ", " c", " d", "e"]
        );
        assert_eq!(split_commands("a | b & c"), ["a | b & c"]);
        assert_eq!(strip_env_assignments("A=1 B=x/y cmd arg"), "cmd arg");
        assert_eq!(strip_env_assignments("cmd A=1"), "cmd A=1");
        assert_eq!(strip_env_assignments("=1 cmd"), "=1 cmd");
        assert_eq!(strip_env_assignments("A-B=1 cmd"), "A-B=1 cmd");
    }

    #[test]
    fn status_payloads_map_events_and_skip_idle_notifications() {
        let p = status_payload(
            Event::UserPromptSubmit,
            &json!({"session_id": "s", "transcript_path": "/t.jsonl", "prompt": "hi"}),
        )
        .unwrap();
        assert_eq!(p["event"], "user_prompt_submit");
        assert_eq!(
            (p["session_file"].as_str(), p["prompt"].as_str()),
            (Some("/t.jsonl"), Some("hi"))
        );
        assert_eq!(
            status_payload(Event::Stop, &json!({})).unwrap()["event"],
            "stop"
        );
        let ask = json!({"message": "Claude needs your permission to use Bash"});
        assert_eq!(
            status_payload(Event::Notification, &ask).unwrap()["event"],
            "permission_request"
        );
        let idle = json!({"message": "Claude is waiting for your input"});
        assert!(status_payload(Event::Notification, &idle).is_none());
        // A failed turn still ends the turn, with the error as the last message.
        let fail = status_payload(
            Event::StopFailure,
            &json!({"error": "authentication_failed"}),
        )
        .unwrap();
        assert_eq!(
            (fail["event"].as_str(), fail["text"].as_str()),
            (Some("stop"), Some("authentication_failed"))
        );
        // Status-only events never answer the harness.
        assert!(respond(Harness::ClaudeCode, Event::Stop, r#"{"cwd":"/"}"#).is_none());
    }
}
