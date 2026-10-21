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
//!
//! * `guard`  — session context and the `atlas dev` denial rules
//! * `status` — event reporting to Atlas Desktop

mod guard;
mod status;

use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, ValueEnum};
use serde_json::{Value, json};

use super::AtlasCommand;
use guard::{pre_tool_use_denial, session_context};
use status::report_status;

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
        if self.event == Event::PermissionRequest {
            if let Some(allow) = status::ask_decision(self.harness, &raw) {
                println!("{}", permission_decision(allow));
            }
            return Ok(());
        }
        if let Some(out) = respond(self.harness, self.event, &raw) {
            println!("{out}");
        }
        Ok(())
    }
}

/// `PermissionRequest` answer; Claude Code and Codex share this shape
/// (<https://code.claude.com/docs/en/hooks#permissionrequest-decision-control>,
/// codex `permission-request.command.output.schema.json`).
pub fn permission_decision(allow: bool) -> String {
    let decision = if allow {
        json!({"behavior": "allow"})
    } else {
        json!({"behavior": "deny", "message": "Denied from Atlas Mobile"})
    };
    json!({
        "hookSpecificOutput": {
            "hookEventName": Event::PermissionRequest.json_name(),
            "decision": decision,
        }
    })
    .to_string()
}

/// The response for one hook invocation, or `None` for "no output". Pure over the
/// filesystem state it reads (`atlas.yaml`, `package.json`, the status file).
pub fn respond(harness: Harness, event: Event, stdin: &str) -> Option<String> {
    // A hook payload is an object; anything else (`[1,2]`, `3`) is noise, not a cwd-less
    // request. Without this the fallback to the current dir answered for whatever atlas.yaml
    // sat there (it failed `malformed_stdin_means_no_output` in a checkout with one).
    let input = serde_json::from_str::<Value>(stdin)
        .ok()
        .filter(Value::is_object)?;
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

#[cfg(test)]
mod decision_tests {
    use super::*;

    /// Both harnesses parse exactly this shape (codex fails closed on unknown fields).
    #[test]
    fn permission_decision_matches_the_harness_schema() {
        let allow: Value = serde_json::from_str(&permission_decision(true)).unwrap();
        assert_eq!(
            allow,
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
        );
        let deny: Value = serde_json::from_str(&permission_decision(false)).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        let keys: Vec<_> = deny["hookSpecificOutput"]["decision"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["behavior", "message"]);
    }
}
