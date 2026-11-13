//! The gate every control-socket request passes before it reaches the run loop: the
//! `agents:` policy for agent callers, and the audit trail for everyone.
//!
//! It lives on the atlas dev side (the socket server) rather than in `atlas mcp`, so any
//! client that identifies as an agent is covered, not just one MCP front end.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::audit::{self, AuditEntry, Outcome};
use super::protocol::{Caller, ControlCmd, ControlResponse, find_in};
use crate::config::{ActionConfig, AgentsPolicy, ProjectConfig, Verdict};

/// What the server snapshots from the config for the life of a run (like the services
/// and actions themselves, a policy edit applies on the next `atlas dev`).
#[derive(Debug, Clone)]
pub struct Guard {
    policy: AgentsPolicy,
    /// Actions, so a policy list naming an action's key or label matches either spelling.
    actions: Vec<ActionConfig>,
    log_dir: PathBuf,
}

/// The audited facts of one request, taken before the command is consumed.
#[derive(Debug)]
pub(super) struct Call {
    pub tool: &'static str,
    pub target: String,
    /// Other names of the target (an action's key and label).
    aliases: Vec<String>,
}

impl Guard {
    pub fn new(cfg: &ProjectConfig, root_dir: &Path) -> Self {
        Self {
            policy: cfg.agents.clone(),
            actions: cfg.actions.clone(),
            log_dir: crate::runtime::log_dir(root_dir),
        }
    }

    /// Start this run's audit file: the previous run's becomes `atlas-audit.prev.log`.
    pub fn start_run(&self) {
        audit::rotate_for_new_run(&self.log_dir);
    }

    /// The control tools an agent can use. Everything else on the socket (`info`,
    /// `shutdown`, `subscribe`, `add_service`) is atlas's own plumbing and has no call.
    pub(super) fn call_of(&self, cmd: &ControlCmd) -> Option<Call> {
        let service = |tool, name: &str| Call {
            tool,
            target: name.to_string(),
            aliases: Vec::new(),
        };
        match cmd {
            ControlCmd::StartService(n) => Some(service("start_service", n)),
            ControlCmd::StopService(n) => Some(service("stop_service", n)),
            ControlCmd::RestartService(n) => Some(service("restart_service", n)),
            ControlCmd::RunAction { target, .. } => Some(Call {
                tool: "run_action",
                target: target.clone(),
                // Resolve to the configured action so `allow: [Migrate]` also covers `m`.
                aliases: find_in(&self.actions, target)
                    .map(|a| vec![a.key.to_string(), a.label.clone()])
                    .unwrap_or_default(),
            }),
            ControlCmd::Info
            | ControlCmd::Shutdown
            | ControlCmd::Subscribe
            | ControlCmd::AddService { .. } => None,
        }
    }

    /// `Some(refusal)` when `caller` may not run `cmd`. Humans are never restricted.
    pub(super) fn check(
        &self,
        caller: &Caller,
        cmd: &ControlCmd,
        call: Option<&Call>,
    ) -> Option<ControlResponse> {
        if !caller.is_agent() {
            return None;
        }
        let Some(call) = call else {
            // Reading who runs atlas is harmless; stopping the run or adding services is not.
            if matches!(cmd, ControlCmd::Info | ControlCmd::Subscribe) {
                return None;
            }
            return Some(ControlResponse::err(
                "agents may not use this command: ask the developer to run it",
            ));
        };
        let aliases: Vec<&str> = call.aliases.iter().map(String::as_str).collect();
        match self.policy.decide(call.tool, &call.target, &aliases) {
            Verdict::Allowed => None,
            Verdict::Denied(msg) => Some(ControlResponse::err(msg)),
        }
    }

    /// Write the audit line for a finished request. `denied` = `reply` is the refusal from
    /// [`Guard::check`]; otherwise the run loop's `reply` decides allowed vs error.
    pub(super) fn audit(
        &self,
        caller: &Caller,
        call: &Call,
        args: &Value,
        denied: bool,
        reply: &ControlResponse,
    ) {
        let (result, detail) = match (denied, reply.ok) {
            (true, _) => (Outcome::Denied, reply.error.clone()),
            (false, true) => (Outcome::Allowed, None),
            (false, false) => (Outcome::Error, reply.error.clone()),
        };
        audit::record(
            &self.log_dir,
            &AuditEntry::new(caller, call.tool, &call.target, args, result, detail),
        );
    }
}
