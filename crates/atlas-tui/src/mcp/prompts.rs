//! MCP prompts (omp surfaces them as `/atlas:<name>` slash commands).

use serde_json::{Map, Value, json};

struct PromptDef {
    name: &'static str,
    description: &'static str,
    /// (name, description, required)
    args: &'static [(&'static str, &'static str, bool)],
    render: fn(&Map<String, Value>) -> String,
}

fn arg<'a>(a: &'a Map<String, Value>, k: &str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or_default()
}

const PROMPTS: &[PromptDef] = &[
    PromptDef {
        name: "diagnose",
        description: "Find and fix why a service is unhealthy or failing",
        args: &[("service", "Service name from atlas.yaml", true)],
        render: |a| {
            let s = arg(a, "service");
            format!(
                "Service \"{s}\" of this Atlas project is unhealthy or failing. Use atlas_status, then atlas_logs(service=\"{s}\", level=\"error\") and atlas_logs_search for the first error, find the root cause in the code, fix it, then atlas_restart_service(\"{s}\") and atlas_wait_ready(service=\"{s}\"). Never start or stop the whole stack."
            )
        },
    },
    PromptDef {
        name: "add-service",
        description: "Add a service to atlas.yaml",
        args: &[
            ("name", "Service name", true),
            ("command", "Command that runs the service", true),
            ("port", "Port it listens on", false),
        ],
        render: |a| {
            let port = match arg(a, "port") {
                "" => String::new(),
                p => format!(", listening on port {p}"),
            };
            format!(
                "Add a service \"{}\" running \"{}\" to atlas.yaml{port}. Follow the atlas skill's Ports rule (random per-project block, never a default port), add depends_on if it needs infra, run `atlas config check`, then tell the user to start it with atlas_start_service or ask them to restart atlas dev.",
                arg(a, "name"),
                arg(a, "command")
            )
        },
    },
    PromptDef {
        name: "status",
        description: "Summarize the state of this Atlas project",
        args: &[],
        render: |_| {
            "Report the state of this Atlas project: call atlas_status and atlas_logs(level=\"error\", lines=50); summarize which services are running, failed or waiting, and the most recent errors with their likely cause.".into()
        },
    },
];

pub fn list() -> Value {
    PROMPTS
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "description": p.description,
                "arguments": p.args.iter().map(|(n, d, r)| json!({"name": n, "description": d, "required": r})).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// `prompts/get` result, or an error message (reported as INVALID_PARAMS).
pub fn get(name: &str, args: &Map<String, Value>) -> Result<Value, String> {
    let p = PROMPTS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("unknown prompt: {name}"))?;
    if let Some((n, ..)) = p
        .args
        .iter()
        .find(|(n, _, req)| *req && arg(args, n).is_empty())
    {
        return Err(format!("missing required argument: {n}"));
    }
    Ok(json!({
        "description": p.description,
        "messages": [{"role": "user", "content": {"type": "text", "text": (p.render)(args)}}],
    }))
}
