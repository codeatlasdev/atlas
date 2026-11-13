//! MCP tool catalogue + implementations. Read tools work off files; control
//! tools go through the running atlas's control socket (`crate::control`).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::logs::{self, MinLevel};
use crate::control::{self, Caller, NOT_RUNNING};
use crate::runtime::service::ServiceState;
use crate::status;

const DEFAULT_WAIT_SECS: u64 = 60;
const MAX_WAIT_SECS: u64 = 600;
const WAIT_POLL: Duration = Duration::from_millis(500);
/// A control command is only reported once the heartbeat shows the service left
/// its pre-command state, so wait_ready right after restart can't race the old "running".
const SETTLE_GRACE: Duration = Duration::from_millis(1500);

/// Outcome of a tool: text for the agent, or an error text (`isError: true`).
pub type ToolResult = Result<String, String>;

/// One MCP tool. `control` is the control-socket command for tools that act on
/// the running atlas; read tools (`None`) are served from files in `call`.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    control: Option<&'static str>,
}

const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "atlas_status",
        description: "Is `atlas dev` running for this project, and what is the state of each service? Returns running flag, atlas pid, heartbeat age in seconds and per-service {name,state,port,health,exit_code,restarts,uptime_s,mem_mb,cpu,started_at,note,port_conflict,ready,probe_error,unhealthy_since,unhealthy_misses,unhealthy_lines} (exit_code = last exit of the process, null while running; uptime_s only while running; note = scheduler text such as `waiting for :3000 (12s)`; port_conflict = {pid,cmd,cwd} of the FOREIGN process that held the service's port at its last start, null otherwise: a dev server started outside atlas, not yours to kill: tell the user, whose options are to stop it themselves or change the service port; ready = result of the service's optional `ready:` probe, true|false, null when it has none; probe_error = why a `degraded` service is degraded (the failing ready probe's message, or the log line repeating); unhealthy_since/unhealthy_misses/unhealthy_lines = when a hung service stopped answering its liveness probe, how many probes it missed, and its last output at that moment), `tunnel` ({state,local_port,ssh_host}, or null when no tunnel is running), and `setup` = last pre_command run {state: running|ok|failed, command, started_at, finished_at?, reason?} or null. State is one of starting|running|degraded|unhealthy|failed|stopped|disabled|suspended: `unhealthy` = process alive but not answering its health probe (hung); `degraded` = liveness passes but the ready probe fails or the same error keeps repeating in the log. Works even when atlas is not running (then running=false). Call this first when debugging a dev environment.",
        schema: schema_no_args,
        control: None,
    },
    ToolDef {
        name: "atlas_logs",
        description: "Tail service logs from .logs/<service>.log (ANSI already stripped, lines look like `HH:MM:SS [stdout|stderr] text`). Without `service`, all services are merged by timestamp. Use `level` to see only problems. Works even when atlas is not running (shows the last run). Project action output is logged too: pass `action:<label>` (e.g. `action:Run checks`) or the file name `action-run-checks` as `service`; without `service` action logs are merged in. Each output line is prefixed [service:line_no]. `service: \"audit\"` shows who restarted/stopped/started what (agents and the developer): `HH:MM:SS caller client tool target -> result`.",
        schema: schema_logs,
        control: None,
    },
    ToolDef {
        name: "atlas_logs_search",
        description: "Search service logs. `query` is a case-insensitive substring, or a regex when wrapped in slashes like /ECONNREFUSED|timeout/. Returns the most recent matches as [service:line_no] text. Set include_previous=true to also search the previous run's logs (<service>.prev.log).",
        schema: schema_logs_search,
        control: None,
    },
    ToolDef {
        name: "atlas_config",
        description: "Summary of the project config (atlas.yaml merged with atlas.local.yaml, or the stack detected from the project when there is no atlas.yaml): project name, source (\"file\" | \"detected\"), services {name, port, depends_on, command, enabled} and actions {key, label, confirm?}. Use it to learn valid service names and action keys/labels before calling control tools.",
        schema: schema_no_args,
        control: None,
    },
    ToolDef {
        name: "atlas_restart_service",
        description: "Restart one service in the running `atlas dev` (stop, then start). Returns once the request is accepted; use atlas_wait_ready to wait for it to come back. Errors if atlas dev is not running. The project's `agents:` policy may deny it: then ask the developer to run it.",
        schema: schema_service,
        control: Some("restart_service"),
    },
    ToolDef {
        name: "atlas_stop_service",
        description: "Stop one service in the running `atlas dev` and mark it disabled (health checks skip it, it will not auto-restart). Undo with atlas_start_service. Errors if atlas dev is not running. The project's `agents:` policy may deny it: then ask the developer to run it.",
        schema: schema_service,
        control: Some("stop_service"),
    },
    ToolDef {
        name: "atlas_start_service",
        description: "Start a stopped/disabled/failed service in the running `atlas dev`. Errors if atlas dev is not running. The project's `agents:` policy may deny it: then ask the developer to run it.",
        schema: schema_service,
        control: Some("start_service"),
    },
    ToolDef {
        name: "atlas_run_action",
        description: "Run an atlas.yaml action (e.g. migrations, db push) in the running `atlas dev`, by key or label (see atlas_config). Actions with a `confirm` message are destructive: the call is REFUSED and the confirm text returned until you pass confirmed=true — only do that after the user agreed. Services listed in the action's pause_services are paused while it runs. Errors if atlas dev is not running. The project's `agents:` policy may deny it: then ask the developer to run it.",
        schema: schema_run_action,
        control: Some("run_action"),
    },
    ToolDef {
        name: "atlas_wait_ready",
        description: "Block until a service (or all services when omitted) is running and healthy, polling the live status. `running` already includes the service's `ready:` probe when it has one: a `degraded` (ready probe failing / repeated errors) or `unhealthy` (hung) service is still waited for. Fails fast if the service enters `failed`; on timeout returns the current states plus, for each blocking service, the failing probe's message (degraded) or since-when and last output lines (unhealthy). Typical use: atlas_restart_service then atlas_wait_ready. `suspended` and `disabled` services are not waited for when waiting on all.",
        schema: schema_wait_ready,
        control: None,
    },
    ToolDef {
        name: "atlas_heavy_status",
        description: "Read-only. The machine-wide queue of heavy jobs (builds, test suites, benches) shared by every project, worktree and agent on this machine: {slots, load, mem_available_pct, running[], queued[]}; each job has {label, command, cwd, project, pid, state, queued_at, started_at (unix ms), age_s, avg_s (average of past runs of the label), eta_s (seconds until it ends, or starts)}. Works without `atlas dev`. Agents MUST run heavy commands (cargo build/test, bun run check, e2e, benches) through `atlas run --heavy [--label X] -- <cmd…>`, never directly: it waits its turn (at most `slots` jobs run at once, first come first served), passes stdout/stderr through and returns the command's exit code. Check this tool to see who holds the slots and how long the wait will be.",
        schema: schema_no_args,
        control: None,
    },
];

fn schema_no_args() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn schema_service() -> Value {
    json!({
        "type": "object",
        "properties": {"service": {"type": "string", "description": "Service name exactly as in atlas.yaml (see atlas_config)."}},
        "required": ["service"],
        "additionalProperties": false
    })
}

fn schema_logs() -> Value {
    json!({
        "type": "object",
        "properties": {
            "service": {"type": "string", "description": "Only this service, `action:<label>` for an action's output, or `audit` for the audit trail of control calls. Omit for all merged."},
            "lines": {"type": "integer", "minimum": 1, "maximum": logs::MAX_TAIL, "default": logs::DEFAULT_TAIL, "description": "How many trailing lines to return."},
            "since_minutes": {"type": "integer", "minimum": 0, "description": "Only lines from the last N minutes (by the HH:MM:SS prefix, local time, max 1440)."},
            "level": {"type": "string", "enum": ["error", "warn"], "description": "error = errors only; warn = warnings and errors."}
        },
        "additionalProperties": false
    })
}

fn schema_logs_search() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Substring, or /regex/."},
            "service": {"type": "string", "description": "Only this service, or `audit` for the control audit trail. Omit to search all."},
            "limit": {"type": "integer", "minimum": 1, "maximum": logs::MAX_SEARCH_LIMIT, "default": logs::DEFAULT_SEARCH_LIMIT, "description": "Max matches returned (the most recent ones are kept)."},
            "include_previous": {"type": "boolean", "default": false, "description": "Also search the previous run's logs (<service>.prev.log); useful right after a crash/restart."}
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn schema_run_action() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "description": "Action key (single character) or label."},
            "confirmed": {"type": "boolean", "default": false, "description": "Set true only after the user approved the action's confirm text."}
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

fn schema_wait_ready() -> Value {
    json!({
        "type": "object",
        "properties": {
            "service": {"type": "string", "description": "Service to wait for. Omit to wait for every service."},
            "timeout_s": {"type": "integer", "minimum": 1, "maximum": MAX_WAIT_SECS, "default": DEFAULT_WAIT_SECS, "description": "Give up after this many seconds (default 60, max 600)."}
        },
        "additionalProperties": false
    })
}

pub fn is_tool(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

pub fn catalogue() -> Value {
    TOOLS
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "inputSchema": (t.schema)()}))
        .collect()
}

/// Run a tool as the developer: what hostd does for Atlas Desktop on a remote host. The MCP
/// server never uses this; it goes through [`call_with`] with its client as an agent.
pub async fn call(root: &Path, name: &str, args: &Value) -> ToolResult {
    call_with(root, name, args, &Caller::Human).await
}

/// Run a tool. `caller` is the MCP client behind this connection: control tools carry it to
/// atlas dev, which applies the project's `agents:` policy and audits the call.
pub async fn call_with(root: &Path, name: &str, args: &Value, caller: &Caller) -> ToolResult {
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return Err(format!("unknown tool: {name}"));
    };
    if let Some(cmd) = tool.control {
        return control_tool(root, cmd, args, caller).await;
    }
    match name {
        "atlas_status" => status_tool(root),
        "atlas_logs" => logs_tool(root, args),
        "atlas_logs_search" => search_tool(root, args),
        "atlas_config" => config_tool(root),
        "atlas_heavy_status" => heavy_status_tool(),
        _ => wait_ready(root, args).await,
    }
}

fn opt_str<'a>(args: &'a Value, k: &str) -> Result<Option<&'a str>, String> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(format!("'{k}' must be a string")),
    }
}

fn opt_u64(args: &Value, k: &str) -> Result<Option<u64>, String> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("'{k}' must be a non-negative integer")),
    }
}

fn status_json(root: &Path) -> Value {
    let mut v = match status::live_snapshot(root) {
        Ok(s) => json!({
            "running": true,
            "pid": s.pid,
            "heartbeat_age_s": s.age_secs,
            "services": s.services.iter().map(|r| json!({
                "name": r.name, "state": r.state, "port": r.port, "health": r.health,
                "exit_code": r.exit_code, "restarts": r.restarts, "uptime_s": r.uptime_s, "mem_mb": r.mem_mb, "cpu": r.cpu,
                "started_at": r.started_at, "note": r.note, "off": r.off,
                "port_conflict": r.port_conflict,
                "ready": r.ready, "probe_error": r.probe_error,
                "unhealthy_since": r.unhealthy_since, "unhealthy_misses": r.unhealthy_misses,
                "unhealthy_lines": r.unhealthy_lines,
            })).collect::<Vec<_>>(),
            "tunnel": s.tunnel.as_ref().map(|t| json!({
                "state": t.state, "local_port": t.local_port, "ssh_host": t.ssh_host,
            })),
        }),
        Err(reason) => json!({
            "running": false,
            "reason": reason,
            "hint": NOT_RUNNING,
        }),
    };
    let setup = std::fs::read_to_string(
        crate::runtime::log_dir(root).join(crate::runtime::deps::SETUP_STATUS),
    )
    .ok()
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .unwrap_or(Value::Null);
    v["setup"] = setup;
    v
}

pub(crate) fn status_tool(root: &Path) -> ToolResult {
    Ok(serde_json::to_string_pretty(&status_json(root)).unwrap_or_default())
}

/// `service: "audit"` (or the file's own name) selects the control audit trail.
fn is_audit(service: &str) -> bool {
    service == "audit" || service == crate::control::audit::AUDIT_LOG
}

/// The newest `keep` audit entries as readable lines (oldest first), only those matching
/// `matcher` when given.
fn audit_text(
    root: &Path,
    keep: usize,
    include_previous: bool,
    matcher: Option<&logs::Matcher>,
) -> String {
    let entries =
        crate::control::audit::read_recent(&root.join(".logs"), usize::MAX, include_previous);
    let rendered: Vec<String> = entries
        .iter()
        .map(crate::control::audit::AuditEntry::render)
        .filter(|l| matcher.is_none_or(|m| m.is_match(l)))
        .collect();
    let out = &rendered[rendered.len().saturating_sub(keep)..];
    if out.is_empty() {
        return "(no matching log lines)".into();
    }
    out.join("\n")
}

pub(crate) fn logs_tool(root: &Path, args: &Value) -> ToolResult {
    let lines = opt_u64(args, "lines")?.map_or(logs::DEFAULT_TAIL, |n| {
        (n as usize).clamp(1, logs::MAX_TAIL)
    });
    let level = opt_str(args, "level")?.map(MinLevel::parse).transpose()?;
    if opt_str(args, "service")?.is_some_and(is_audit) {
        return Ok(audit_text(root, lines, false, None));
    }
    let opts = logs::TailOpts {
        service: opt_str(args, "service")?,
        lines,
        since_minutes: opt_u64(args, "since_minutes")?,
        level,
        now_secs: crate::runtime::clock::secs_of_day_now(),
    };
    let out = logs::tail(&root.join(".logs"), &opts)?;
    if out.is_empty() {
        return Ok("(no matching log lines)".into());
    }
    Ok(out
        .iter()
        .map(|l| l.render())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn search_tool(root: &Path, args: &Value) -> ToolResult {
    let query = opt_str(args, "query")?.ok_or("missing required arg: query")?;
    let matcher = logs::Matcher::parse(query)?;
    if opt_str(args, "service")?.is_some_and(is_audit) {
        let limit = opt_u64(args, "limit")?.map_or(logs::DEFAULT_SEARCH_LIMIT, |n| {
            (n as usize).clamp(1, logs::MAX_SEARCH_LIMIT)
        });
        let previous = args
            .get("include_previous")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(audit_text(root, limit, previous, Some(&matcher)));
    }
    let limit = opt_u64(args, "limit")?.map_or(logs::DEFAULT_SEARCH_LIMIT, |n| {
        (n as usize).clamp(1, logs::MAX_SEARCH_LIMIT)
    });
    let include_previous = args
        .get("include_previous")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let hits = logs::search(
        &root.join(".logs"),
        &matcher,
        opt_str(args, "service")?,
        limit,
        include_previous,
    )?;
    if hits.is_empty() {
        return Ok("(no matches)".into());
    }
    Ok(hits
        .iter()
        .map(|l| l.render())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The heavy-job queue (`atlas heavy status --json`): machine-wide, so it needs no project.
fn heavy_status_tool() -> ToolResult {
    let st = atlas_core::heavy::Dir::default_dir().status(atlas_core::heavy::slots());
    serde_json::to_string_pretty(&st).map_err(|e| e.to_string())
}

pub(crate) fn config_tool(root: &Path) -> ToolResult {
    let (cfg, source) = crate::config::load_or_detect(root).map_err(|e| format!("{e:#}"))?;
    let mut names: Vec<&String> = cfg.services.keys().collect();
    names.sort();
    let services: Vec<Value> = names
        .into_iter()
        .map(|n| {
            let s = &cfg.services[n];
            json!({
                "name": n,
                "port": s.port,
                "depends_on": s
                    .deps()
                    .into_iter()
                    .map(|(d, c)| json!({"service": d, "condition": c.as_str()}))
                    .collect::<Vec<_>>(),
                "command": s.command,
                "enabled": s.is_enabled(),
            })
        })
        .collect();
    let actions: Vec<Value> = cfg
        .actions
        .iter()
        .map(|a| {
            json!({
                "key": a.key.to_string(),
                "label": a.label,
                "confirm": a.confirm,
                "pause_services": a.pause_services,
                "session": a.session,
            })
        })
        .collect();
    // `source`: "file" (atlas.yaml) or "detected" (no file; inferred from the project, `atlas init` saves it).
    let source = match source {
        crate::config::ConfigSource::File(_) => "file",
        crate::config::ConfigSource::Detected => "detected",
    };
    let v =
        json!({ "project": cfg.name, "source": source, "services": services, "actions": actions });
    Ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

async fn control_tool(root: &Path, cmd: &str, args: &Value, caller: &Caller) -> ToolResult {
    let resp = control::call_as(root, cmd, args.clone(), caller).await?;
    if resp.ok {
        Ok(serde_json::to_string_pretty(&resp.data.unwrap_or(Value::Null)).unwrap_or_default())
    } else {
        Err(resp.error.unwrap_or_else(|| "request failed".into()))
    }
}

/// Pure readiness verdict over a status snapshot.
#[derive(Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Failed(String),
    Waiting,
}

pub fn readiness(rows: &[status::ServiceRow], service: Option<&str>) -> Result<Readiness, String> {
    if let Some(name) = service
        && !rows.iter().any(|r| r.name == name)
    {
        return Err(format!(
            "unknown service '{name}'. Known services: {}",
            rows.iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let relevant = rows.iter().filter(|r| match service {
        Some(n) => r.name == n,
        // Waiting on everything ignores services that are intentionally not running.
        None => !matches!(
            r.state.parse(),
            Ok(ServiceState::Disabled | ServiceState::Suspended)
        ),
    });
    let mut waiting = false;
    for r in relevant {
        match r.state.parse() {
            Ok(ServiceState::Running) => {}
            Ok(ServiceState::Failed) => return Ok(Readiness::Failed(r.name.clone())),
            _ => waiting = true,
        }
    }
    Ok(if waiting {
        Readiness::Waiting
    } else {
        Readiness::Ready
    })
}

async fn wait_ready(root: &Path, args: &Value) -> ToolResult {
    let service = opt_str(args, "service")?;
    let timeout = opt_u64(args, "timeout_s")?
        .unwrap_or(DEFAULT_WAIT_SECS)
        .clamp(1, MAX_WAIT_SECS);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let started = tokio::time::Instant::now();
    loop {
        let snap = status::live_snapshot(root).map_err(|r| format!("{NOT_RUNNING} ({r})"))?;
        match readiness(&snap.services, service)? {
            // The 1s status heartbeat may still show the pre-restart "running";
            // don't trust a first-instant Ready — give the manager one write cycle.
            Readiness::Ready if started.elapsed() >= SETTLE_GRACE => {
                return Ok(format!(
                    "ready after {:.1}s\n{}",
                    started.elapsed().as_secs_f32(),
                    states_line(&snap.services)
                ));
            }
            Readiness::Failed(name) => {
                let mut msg = format!("service '{name}' failed\n{}", states_line(&snap.services));
                let tail = logs_tool(
                    root,
                    &json!({"service": name, "lines": 10, "level": "error"}),
                );
                if let Ok(lines) = tail {
                    msg.push_str(&format!("\n--- last errors ({name}) ---\n{lines}"));
                }
                return Err(msg);
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            let why = blockers(&snap.services, service);
            return Err(format!(
                "timed out after {timeout}s waiting for readiness\n{}{}",
                states_line(&snap.services),
                if why.is_empty() {
                    String::new()
                } else {
                    format!("\n{why}")
                }
            ));
        }
        tokio::time::sleep(WAIT_POLL).await;
    }
}

fn states_line(rows: &[status::ServiceRow]) -> String {
    rows.iter()
        .map(|r| format!("{}={}", r.name, r.state))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Why the services still being waited on are not ready: the failing probe's own message
/// for `degraded`, since-when plus the captured log tail for `unhealthy`. Empty when nothing
/// carries a reason.
pub fn blockers(rows: &[status::ServiceRow], service: Option<&str>) -> String {
    let mut out = Vec::new();
    for r in rows.iter().filter(|r| service.is_none_or(|n| r.name == n)) {
        match r.state.parse() {
            Ok(ServiceState::Degraded) => {
                let why = r
                    .probe_error
                    .as_deref()
                    .or(r.note.as_deref())
                    .unwrap_or("degraded");
                out.push(format!("{}: {why}", r.name));
            }
            Ok(ServiceState::Unhealthy) => {
                let secs = r
                    .unhealthy_since
                    .map_or(0, |t| crate::runtime::clock::now_secs().saturating_sub(t));
                let mut s = format!(
                    "{}: unhealthy for {secs}s ({} missed probes)",
                    r.name,
                    r.unhealthy_misses.unwrap_or(0)
                );
                if !r.unhealthy_lines.is_empty() {
                    s.push_str("; last output:");
                    for l in &r.unhealthy_lines {
                        s.push_str(&format!("\n    {l}"));
                    }
                }
                out.push(s);
            }
            _ => {}
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, state: &str) -> status::ServiceRow {
        status::ServiceRow {
            name: name.into(),
            state: state.into(),
            port: None,
            health: None,
            exit_code: None,
            restarts: 0,
            uptime_s: None,
            mem_mb: None,
            cpu: None,
            started_at: None,
            note: None,
            off: false,
            port_conflict: None,
            ready: None,
            probe_error: None,
            unhealthy_since: None,
            unhealthy_misses: None,
            unhealthy_lines: Vec::new(),
        }
    }

    #[test]
    fn readiness_all_running_is_ready_ignoring_disabled() {
        let rows = [
            row("a", "running"),
            row("b", "disabled"),
            row("c", "suspended"),
        ];
        assert_eq!(readiness(&rows, None).unwrap(), Readiness::Ready);
    }

    #[test]
    fn readiness_starting_waits_and_failed_fails_fast() {
        let rows = [row("a", "starting"), row("b", "failed")];
        assert_eq!(readiness(&rows, Some("a")).unwrap(), Readiness::Waiting);
        assert_eq!(
            readiness(&rows, None).unwrap(),
            Readiness::Failed("b".into())
        );
    }

    #[test]
    fn readiness_single_service_ignores_others_and_rejects_unknown() {
        let rows = [row("a", "running"), row("b", "failed")];
        assert_eq!(readiness(&rows, Some("a")).unwrap(), Readiness::Ready);
        assert!(readiness(&rows, Some("zzz")).unwrap_err().contains("a, b"));
    }

    #[test]
    fn readiness_targeted_disabled_service_keeps_waiting() {
        let rows = [row("a", "disabled")];
        assert_eq!(readiness(&rows, Some("a")).unwrap(), Readiness::Waiting);
    }

    #[test]
    fn catalogue_has_all_tools_with_object_schemas() {
        let c = catalogue();
        let names: Vec<&str> = c
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "atlas_status",
                "atlas_logs",
                "atlas_logs_search",
                "atlas_config",
                "atlas_restart_service",
                "atlas_stop_service",
                "atlas_start_service",
                "atlas_run_action",
                "atlas_wait_ready",
                "atlas_heavy_status"
            ]
        );
        for t in c.as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(t["description"].as_str().unwrap().len() > 40);
        }
    }

    // Regression guard for the class "the developer's own path got the agent policy": hostd runs
    // `tools::call` for Atlas Desktop on remote hosts, and that is the developer, never an agent.
    #[tokio::test]
    async fn call_is_the_developer_and_call_with_an_agent_is_denied_by_the_same_policy() {
        let d = tempfile::TempDir::new().unwrap();
        let cfg: crate::config::ProjectConfig = serde_norway::from_str(
            "name: p\nservices:\n  api: { command: x }\nagents: { control: deny }\n",
        )
        .unwrap();
        let (_server, mut rx) =
            control::ControlServer::bind(d.path(), control::Guard::new(&cfg, d.path())).unwrap();
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req
                    .reply
                    .send(control::ControlResponse::ok(json!({"queued": true})));
            }
        });
        let args = json!({"service": "api"});
        let human = call(d.path(), "atlas_restart_service", &args).await;
        assert!(human.is_ok(), "{human:?}");
        let agent = call_with(
            d.path(),
            "atlas_restart_service",
            &args,
            &Caller::agent(Some("omp".into()), None),
        )
        .await
        .unwrap_err();
        assert!(
            agent.contains("denies agents restart_service on api"),
            "{agent}"
        );
    }

    #[tokio::test]
    async fn control_tools_report_not_running_without_socket() {
        let d = tempfile::TempDir::new().unwrap();
        for (tool, args) in [
            ("atlas_restart_service", json!({"service": "api"})),
            ("atlas_stop_service", json!({"service": "api"})),
            ("atlas_start_service", json!({"service": "api"})),
            ("atlas_run_action", json!({"action": "m"})),
        ] {
            let e = call(d.path(), tool, &args).await.unwrap_err();
            assert_eq!(e, NOT_RUNNING, "{tool}");
        }
        let e = call(d.path(), "atlas_wait_ready", &json!({}))
            .await
            .unwrap_err();
        assert!(e.starts_with(NOT_RUNNING), "{e}");
    }

    #[tokio::test]
    async fn status_without_atlas_reports_not_running_not_error() {
        let d = tempfile::TempDir::new().unwrap();
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["running"], false);
        assert!(v["hint"].as_str().unwrap().contains("not running"));
    }

    #[tokio::test]
    async fn logs_tool_validates_args() {
        let d = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(d.path().join(".logs")).unwrap();
        std::fs::write(d.path().join(".logs/api.log"), "10:00:00 [stdout] hi\n").unwrap();
        assert!(
            call(d.path(), "atlas_logs", &json!({"level": "loud"}))
                .await
                .is_err()
        );
        assert!(
            call(d.path(), "atlas_logs", &json!({"lines": "x"}))
                .await
                .is_err()
        );
        let ok = call(d.path(), "atlas_logs", &json!({"service": "api"}))
            .await
            .unwrap();
        assert_eq!(ok, "[api:1] 10:00:00 [stdout] hi");
        assert!(
            call(d.path(), "atlas_logs_search", &json!({}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn config_tool_lists_services_and_actions() {
        let d = tempfile::TempDir::new().unwrap();
        std::fs::write(
            d.path().join("atlas.yaml"),
            "name: demo\nservices:\n  api: { command: 'x', port: 3000 }\nactions:\n  - { key: x, label: Migrate, command: 'true', confirm: 'sure?' }\n",
        )
        .unwrap();
        let out = call(d.path(), "atlas_config", &json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["project"], "demo");
        assert_eq!(v["services"][0]["port"], 3000);
        assert_eq!(v["actions"][0]["confirm"], "sure?");
    }

    #[tokio::test]
    async fn status_includes_setup_state_when_present() {
        let d = tempfile::TempDir::new().unwrap();
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        assert!(serde_json::from_str::<Value>(&out).unwrap()["setup"].is_null());
        std::fs::create_dir_all(d.path().join(".logs")).unwrap();
        std::fs::write(
            d.path().join(".logs/setup.json"),
            r#"{"state":"failed","command":"x","started_at":1,"finished_at":1,"reason":"exit 3: boom"}"#,
        )
        .unwrap();
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["setup"]["reason"], "exit 3: boom");
    }

    // ── readiness / degraded / unhealthy ────────────────────────────────

    /// A real manager writes the status file; the MCP tools read it back. No hand-written
    /// JSON on either side of the contract.
    fn real_status(
        dir: &Path,
        ready: Option<crate::config::ReadyProbe>,
        set: impl FnOnce(&mut crate::runtime::ServiceManager),
    ) {
        let cfg = crate::config::ProjectConfig {
            name: "t".into(),
            services: std::collections::HashMap::from([(
                "api".to_string(),
                crate::config::ServiceDef {
                    command: Some("sleep 30".into()),
                    port: Some(3000),
                    ready,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut m = crate::runtime::ServiceManager::new(dir.to_path_buf(), &cfg, tx);
        set(&mut m);
        m.write_status_file();
    }

    fn http_ready() -> Option<crate::config::ReadyProbe> {
        Some(crate::config::ReadyProbe {
            http: Some("/ready".into()),
            status: None,
            command: None,
            timeout: None,
        })
    }

    /// Incident #136 through the real producer: degraded + probe error reach `atlas_status`.
    #[tokio::test]
    async fn atlas_status_reports_degraded_with_the_probe_error_and_ready_column() {
        let d = tempfile::TempDir::new().unwrap();
        real_status(d.path(), http_ready(), |m| {
            let svc = m.services.get_mut("api").unwrap();
            svc.probes.ready_error = Some("GET /ready → 500".into());
            svc.probes.degraded = Some("ready probe failed: GET /ready → 500".into());
            m.set_state_if_changed("api", ServiceState::Degraded);
        });
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let api = &v["services"][0];
        assert_eq!(api["state"], "degraded");
        assert_eq!(api["ready"], false);
        assert!(
            api["probe_error"]
                .as_str()
                .unwrap()
                .contains("GET /ready → 500"),
            "{api}"
        );
        assert!(api["unhealthy_since"].is_null());
    }

    #[tokio::test]
    async fn atlas_status_reports_unhealthy_with_since_and_log_lines() {
        let d = tempfile::TempDir::new().unwrap();
        real_status(d.path(), None, |m| {
            let svc = m.services.get_mut("api").unwrap();
            svc.probes.unhealthy = Some(crate::runtime::manager::test_unhealthy(
                3,
                &["booting", "GET /slow 9000ms"],
            ));
            m.set_state_if_changed("api", ServiceState::Unhealthy);
        });
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let api = &v["services"][0];
        assert_eq!(api["state"], "unhealthy");
        assert_eq!(api["unhealthy_misses"], 3);
        assert!(api["unhealthy_since"].as_u64().unwrap() > 0);
        assert_eq!(api["unhealthy_lines"][1], "GET /slow 9000ms");
        assert!(
            api["ready"].is_null(),
            "no ready probe configured → null, not false"
        );
    }

    /// A service without `ready` serializes the new keys as null: consumers can rely on them.
    #[tokio::test]
    async fn atlas_status_without_a_ready_probe_has_null_probe_keys() {
        let d = tempfile::TempDir::new().unwrap();
        real_status(d.path(), None, |m| {
            m.set_state_if_changed("api", ServiceState::Running)
        });
        let out = call(d.path(), "atlas_status", &json!({})).await.unwrap();
        let api = &serde_json::from_str::<Value>(&out).unwrap()["services"][0];
        assert_eq!(api["state"], "running");
        for k in [
            "ready",
            "probe_error",
            "unhealthy_since",
            "unhealthy_misses",
        ] {
            assert!(api[k].is_null(), "{k}: {api}");
        }
    }

    #[test]
    fn readiness_waits_on_degraded_and_unhealthy_instead_of_declaring_ready() {
        for state in ["degraded", "unhealthy"] {
            let rows = [row("api", state), row("db", "running")];
            assert_eq!(
                readiness(&rows, None).unwrap(),
                Readiness::Waiting,
                "{state}"
            );
            assert_eq!(
                readiness(&rows, Some("api")).unwrap(),
                Readiness::Waiting,
                "{state}"
            );
            assert_eq!(readiness(&rows, Some("db")).unwrap(), Readiness::Ready);
        }
    }

    #[test]
    fn blockers_name_the_failing_probe_and_the_hang_context() {
        let mut api = row("api", "degraded");
        api.probe_error = Some("ready probe failed: GET /ready → 500".into());
        let mut web = row("web", "unhealthy");
        web.unhealthy_since = Some(crate::runtime::clock::now_secs() - 12);
        web.unhealthy_misses = Some(3);
        web.unhealthy_lines = vec!["render queue: 412 pending".into()];
        let rows = [api, web, row("db", "running")];
        let out = blockers(&rows, None);
        assert!(
            out.contains("api: ready probe failed: GET /ready → 500"),
            "{out}"
        );
        assert!(
            out.contains("web: unhealthy for 12s") && out.contains("3 missed probes"),
            "{out}"
        );
        assert!(out.contains("render queue: 412 pending"), "{out}");
        assert!(
            !out.contains("db"),
            "healthy services are not blockers: {out}"
        );
        assert!(
            blockers(&rows, Some("web")).contains("web:")
                && !blockers(&rows, Some("web")).contains("api:")
        );
    }

    /// `atlas_wait_ready` against a real status file that stays degraded: it must not
    /// succeed, and on timeout it returns the failing probe's message.
    #[tokio::test]
    async fn wait_ready_times_out_on_degraded_with_the_probe_message() {
        let d = tempfile::TempDir::new().unwrap();
        real_status(d.path(), http_ready(), |m| {
            let svc = m.services.get_mut("api").unwrap();
            svc.probes.ready_error = Some("GET /ready → 500".into());
            svc.probes.degraded = Some("ready probe failed: GET /ready → 500".into());
            m.set_state_if_changed("api", ServiceState::Degraded);
        });
        let err = call(
            d.path(),
            "atlas_wait_ready",
            &json!({"service": "api", "timeout_s": 1}),
        )
        .await
        .unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(err.contains("api=degraded"), "{err}");
        assert!(
            err.contains("ready probe failed: GET /ready → 500"),
            "{err}"
        );
    }

    /// The same wait succeeds once the service is `running` (its `ready` passes).
    #[tokio::test]
    async fn wait_ready_succeeds_when_ready_passes() {
        let d = tempfile::TempDir::new().unwrap();
        real_status(d.path(), http_ready(), |m| {
            m.set_state_if_changed("api", ServiceState::Running);
        });
        let ok = call(
            d.path(),
            "atlas_wait_ready",
            &json!({"service": "api", "timeout_s": 5}),
        )
        .await
        .unwrap();
        assert!(ok.starts_with("ready after"), "{ok}");
    }
}
