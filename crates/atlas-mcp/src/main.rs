mod bridge;
mod protocol;
mod tools;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use bridge::DaemonBridge;
use protocol::{ContentItem, JsonRpcRequest, JsonRpcResponse};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("atlas_mcp=debug")
        .init();

    tracing::info!("atlas-mcp-server starting");

    let bridge = DaemonBridge::new();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_request(trimmed, &bridge).await;
        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        writer.write_all(out.as_bytes()).await?;
        writer.flush().await?;
    }

    Ok(())
}

async fn handle_request(raw: &str, bridge: &DaemonBridge) -> JsonRpcResponse {
    let req: JsonRpcRequest = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(e) => {
            return JsonRpcResponse::error(None, -32700, format!("parse error: {e}"));
        }
    };

    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => handle_initialize(id),
        "initialized" | "notifications/initialized" => {
            // Notifications don't get responses per JSON-RPC spec
            // But some clients expect acknowledgment, return empty
            JsonRpcResponse::success(id, json!({}))
        }
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(id, req.params, bridge).await,
        "ping" => JsonRpcResponse::success(id, json!({})),
        _ => JsonRpcResponse::error(id, -32601, format!("method not found: {}", req.method)),
    }
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "atlas",
                "version": "0.1.0"
            }
        }),
    )
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tool_defs = tools::all_tools();
    JsonRpcResponse::success(id, json!({ "tools": tool_defs }))
}

async fn handle_tools_call(
    id: Option<Value>,
    params: Option<Value>,
    bridge: &DaemonBridge,
) -> JsonRpcResponse {
    let params = params.unwrap_or(Value::Null);

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return JsonRpcResponse::error(id, -32602, "missing 'name' in params".to_string());
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(json!({}));

    let Some((daemon_method, daemon_params)) = tools::map_tool_to_daemon(&tool_name, arguments)
    else {
        return JsonRpcResponse::error(id, -32602, format!("unknown tool: {tool_name}"));
    };

    match bridge.call(daemon_method, daemon_params).await {
        Ok(result) => {
            let text = if result.is_string() {
                result.as_str().unwrap_or("").to_string()
            } else {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| "null".to_string())
            };

            let content = vec![ContentItem::text(text)];
            JsonRpcResponse::success(
                id,
                json!({
                    "content": content,
                    "isError": false
                }),
            )
        }
        Err(e) => {
            let content = vec![ContentItem::text(format!("Error: {e}"))];
            JsonRpcResponse::success(
                id,
                json!({
                    "content": content,
                    "isError": true
                }),
            )
        }
    }
}
