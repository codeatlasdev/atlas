//! MCP tool catalogue + implementations. Read tools work off files; control
//! tools go through the running atlas's control socket (`crate::control`).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::logs::{self, MinLevel};
use crate::control::{self, NOT_RUNNING};
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
        description: "Is `atlas dev` running for this project, and what is the state of each service? Returns running flag, atlas pid, heartbeat age in seconds and per-service {name,state,port,health,exit_code,restarts,uptime_s,mem_mb,cpu,started_at,note} (exit_code = last exit of the process, null while running; uptime_s only while running; note = scheduler text such as `waiting for :3000 (12s)`), `tunnel` ({state,local_port,ssh_host}, or null when no tunnel is running), and `setup` = last pre_command run {state: running|ok|failed, command, started_at, finished_at?, reason?} or null. State is one of starting|running|failed|stopped|disabled|suspended. Works even when atlas is not running (then running=false). Call this first when debugging a dev environment.",
        schema: schema_no_args,
        control: None,
    },
    ToolDef {
        name: "atlas_logs",
        description: "Tail service logs from .logs/<service>.log (ANSI already stripped, lines look like `HH:MM:SS [stdout|stderr] text`). Without `service`, all services are merged by timestamp. Use `level` to see only problems. Works even when atlas is not running (shows the last run). Project action output is logged too: pass `action:<label>` (e.g. `action:Run checks`) or the file name `action-run-checks` as `service`; without `service` action logs are merged in. Each output line is prefixed [service:line_no].",
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
        description: "Summary of atlas.yaml (merged with atlas.local.yaml): project name, services {name, port, depends_on, command, enabled} and actions {key, label, confirm?}. Use it to learn valid service names and action keys/labels before calling control tools.",
        schema: schema_no_args,
        control: None,
    },
    ToolDef {
        name: "atlas_restart_service",
        description: "Restart one service in the running `atlas dev` (stop, then start). Returns once the request is accepted; use atlas_wait_ready to wait for it to come back. Errors if atlas dev is not running.",
        schema: schema_service,
        control: Some("restart_service"),
    },
    ToolDef {
        name: "atlas_stop_service",
        description: "Stop one service in the running `atlas dev` and mark it disabled (health checks skip it, it will not auto-restart). Undo with atlas_start_service. Errors if atlas dev is not running.",
        schema: schema_service,
        control: Some("stop_service"),
    },
    ToolDef {
        name: "atlas_start_service",
        description: "Start a stopped/disabled/failed service in the running `atlas dev`. Errors if atlas dev is not running.",
        schema: schema_service,
        control: Some("start_service"),
    },
    ToolDef {
        name: "atlas_run_action",
        description: "Run an atlas.yaml action (e.g. migrations, db push) in the running `atlas dev`, by key or label (see atlas_config). Actions with a `confirm` message are destructive: the call is REFUSED and the confirm text returned until you pass confirmed=true — only do that after the user agreed. Services listed in the action's pause_services are paused while it runs. Errors if atlas dev is not running.",
        schema: schema_run_action,
        control: Some("run_action"),
    },
    ToolDef {
        name: "atlas_wait_ready",
        description: "Block until a service (or all services when omitted) is running and healthy, polling the live status. Fails fast if the service enters `failed`, and on timeout returns the current states. Typical use: atlas_restart_service then atlas_wait_ready. `suspended` and `disabled` services are not waited for when waiting on all.",
        schema: schema_wait_ready,
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
            "service": {"type": "string", "description": "Only this service, or `action:<label>` for an action's output. Omit for all merged."},
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
            "service": {"type": "string", "description": "Only this service. Omit to search all."},
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

pub async fn call(root: &Path, name: &str, args: &Value) -> ToolResult {
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return Err(format!("unknown tool: {name}"));
    };
    if let Some(cmd) = tool.control {
        return control_tool(root, cmd, args).await;
    }
    match name {
        "atlas_status" => status_tool(root),
        "atlas_logs" => logs_tool(root, args),
        "atlas_logs_search" => search_tool(root, args),
        "atlas_config" => config_tool(root),
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

pub(crate) fn logs_tool(root: &Path, args: &Value) -> ToolResult {
    let lines = opt_u64(args, "lines")?.map_or(logs::DEFAULT_TAIL, |n| {
        (n as usize).clamp(1, logs::MAX_TAIL)
    });
    let level = opt_str(args, "level")?.map(MinLevel::parse).transpose()?;
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

pub(crate) fn config_tool(root: &Path) -> ToolResult {
    let (cfg, _) = crate::config::load_or_detect(root).map_err(|e| format!("{e:#}"))?;
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
    let v = json!({ "project": cfg.name, "services": services, "actions": actions });
    Ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

async fn control_tool(root: &Path, cmd: &str, args: &Value) -> ToolResult {
    let resp = control::call(root, cmd, args.clone()).await?;
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
            return Err(format!(
                "timed out after {timeout}s waiting for readiness\n{}",
                states_line(&snap.services)
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
                "atlas_wait_ready"
            ]
        );
        for t in c.as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(t["description"].as_str().unwrap().len() > 40);
        }
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
}
