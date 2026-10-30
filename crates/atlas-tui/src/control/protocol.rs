//! Wire types and command resolution: what a request line means, before any
//! socket is involved.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::config::ProjectConfig;

#[derive(Debug, Deserialize)]
pub(super) struct WireRequest {
    pub(super) cmd: String,
    #[serde(default)]
    pub(super) args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ControlResponse {
    pub fn ok(data: Value) -> Self {
        Self {
            ok: true,
            error: None,
            data: Some(data),
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            data: None,
        }
    }
}

/// A parsed, validated control command.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlCmd {
    StartService(String),
    StopService(String),
    RestartService(String),
    /// Run the action whose key or label matches `target`.
    RunAction {
        target: String,
        confirmed: bool,
    },
    /// Who owns this run: `{mode, pid, project, clients, version}`.
    Info,
    /// Stop every service and exit (`atlas down`, "Stop everything" while attached).
    Shutdown,
    /// Snapshot reply, then a stream of events (see `runner.rs`).
    Subscribe,
    /// Register and start an ad-hoc service (`atlas -- <cmd>`): runner only, never persisted.
    AddService {
        command: String,
    },
}

impl ControlCmd {
    pub fn parse(cmd: &str, args: &Value) -> Result<Self, String> {
        let str_arg = |k: &str| {
            args.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let service = || -> Result<String, String> {
            str_arg("service").ok_or_else(|| "missing required arg: service".to_string())
        };
        match cmd {
            "start_service" => Ok(Self::StartService(service()?)),
            "stop_service" => Ok(Self::StopService(service()?)),
            "restart_service" => Ok(Self::RestartService(service()?)),
            "run_action" => Ok(Self::RunAction {
                target: str_arg("action").ok_or("missing required arg: action (key or label)")?,
                confirmed: args
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            "info" => Ok(Self::Info),
            "shutdown" => Ok(Self::Shutdown),
            "subscribe" => Ok(Self::Subscribe),
            "add_service" => Ok(Self::AddService {
                command: str_arg("command").ok_or("missing required arg: command")?,
            }),
            other => Err(format!("unknown cmd: {other}")),
        }
    }
}

/// What the run loop must do for a [`ControlCmd`], after config-level validation.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    StartService(String),
    StopService(String),
    RestartService(String),
    /// Action key plus its label (for the reply).
    RunAction {
        key: char,
        label: String,
    },
}

/// Validate `cmd` against the loaded config: unknown service/action and the
/// `confirm` gate are rejected here with agent-readable errors.
pub fn resolve(cmd: ControlCmd, cfg: &ProjectConfig) -> Result<Resolved, String> {
    let known = |name: &str| -> Result<(), String> {
        if cfg.services.contains_key(name) {
            return Ok(());
        }
        let mut names: Vec<&str> = cfg.services.keys().map(String::as_str).collect();
        names.sort_unstable();
        Err(format!(
            "unknown service '{name}'. Known services: {}",
            names.join(", ")
        ))
    };
    match cmd {
        ControlCmd::Info
        | ControlCmd::Shutdown
        | ControlCmd::Subscribe
        | ControlCmd::AddService { .. } => Err("not a service command".to_string()),
        ControlCmd::StartService(n) => known(&n).map(|()| Resolved::StartService(n)),
        ControlCmd::StopService(n) => known(&n).map(|()| Resolved::StopService(n)),
        ControlCmd::RestartService(n) => known(&n).map(|()| Resolved::RestartService(n)),
        ControlCmd::RunAction { target, confirmed } => {
            let action = find_action(cfg, &target).ok_or_else(|| {
                let all: Vec<String> = cfg
                    .actions
                    .iter()
                    .map(|a| format!("{} ({})", a.key, a.label))
                    .collect();
                format!(
                    "unknown action '{target}'. Known actions: {}",
                    all.join(", ")
                )
            })?;
            if let Some(text) = action.confirm.as_deref().filter(|c| !c.is_empty())
                && !confirmed
            {
                return Err(format!(
                    "action '{}' requires confirmation: \"{text}\". Ask the user, then retry with confirmed: true.",
                    action.label
                ));
            }
            Ok(Resolved::RunAction {
                key: action.key,
                label: action.label.clone(),
            })
        }
    }
}

/// Match by exact key char first, then case-insensitive label.
pub fn find_action<'a>(
    cfg: &'a ProjectConfig,
    target: &str,
) -> Option<&'a crate::config::ActionConfig> {
    let mut chars = target.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && let Some(a) = cfg.actions.iter().find(|a| a.key == c)
    {
        return Some(a);
    }
    cfg.actions
        .iter()
        .find(|a| a.label.eq_ignore_ascii_case(target))
}

/// One request handed to the run loop. The loop MUST answer through `reply`.
#[derive(Debug)]
pub struct ControlRequest {
    pub cmd: ControlCmd,
    pub reply: oneshot::Sender<ControlResponse>,
    /// `Subscribe` only: where the loop pushes event lines after the snapshot.
    /// Bounded: a client that stops reading is dropped by the hub, not buffered forever.
    pub stream: Option<mpsc::Sender<String>>,
}
