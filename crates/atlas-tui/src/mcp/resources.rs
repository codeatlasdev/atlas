//! MCP resources: live status, config summary and per-service logs, served
//! from the same functions as the matching tools.

use std::path::Path;

use serde_json::{Value, json};

use super::tools;

const LOGS_PREFIX: &str = "atlas://logs/";

pub fn list() -> Value {
    json!([
        {"uri": "atlas://status", "name": "status", "mimeType": "application/json",
         "description": "Live status of atlas dev (same JSON as atlas_status)"},
        {"uri": "atlas://config", "name": "config", "mimeType": "application/json",
         "description": "atlas.yaml summary (same as atlas_config)"},
    ])
}

pub fn templates() -> Value {
    json!([
        {"uriTemplate": "atlas://logs/{service}", "name": "service log", "mimeType": "text/plain",
         "description": "Last 200 lines of .logs/<service>.log"},
    ])
}

/// `resources/read` result, or an error message (reported as INVALID_PARAMS).
pub fn read(root: &Path, uri: &str) -> Result<Value, String> {
    let (mime, text) = match uri {
        "atlas://status" => ("application/json", tools::status_tool(root)?),
        "atlas://config" => ("application/json", tools::config_tool(root)?),
        _ => match uri.strip_prefix(LOGS_PREFIX).filter(|s| !s.is_empty()) {
            Some(svc) => (
                "text/plain",
                tools::logs_tool(root, &json!({"service": svc, "lines": 200}))?,
            ),
            None => return Err(format!("unknown resource: {uri}")),
        },
    };
    Ok(json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]}))
}
