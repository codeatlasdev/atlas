//! Bot MCP stdio server (`atlas mcp --bot <id>`).
//!
//! Implements a minimal JSON-RPC 2.0 MCP server over stdin/stdout.
//! Exposes three tools to the agent running inside the bot:
//!
//! - **`ask_user`**  — post a question to the mobile/desktop UI and block until
//!   answered (up to 30 min).
//! - **`set_bot`**   — update this bot's metadata (name, role, triggers, …).
//! - **`approve`**   — answer a Claude permission-prompt (used with
//!   `--permission-prompt-tool atlas:approve`).
//!
//! Reads from env:
//!   `ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`, `ATLAS_BOT_ID`, `ATLAS_PANE_KEY`

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Serve the bot MCP protocol on stdin/stdout until EOF.
pub async fn serve(bot_id: &str) -> anyhow::Result<()> {
    // Install the ring crypto provider so rustls-no-provider works.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1800)) // ask_user can take up to 30 min
        .build()?;

    let env = Env::from_env(bot_id);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut out = tokio::io::BufWriter::new(stdout);

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // ignore malformed lines
        };

        // Notifications have no "id"; handle them but send no response.
        if msg.get("id").is_none() {
            handle_notification(&msg, &mut out).await?;
            continue;
        }

        let id = msg["id"].clone();
        let method = msg["method"].as_str().unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let result = dispatch(method, params, &client, &env).await;

        let response = match result {
            Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
            Err(e) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":e}}),
        };

        write_line(&mut out, &response).await?;
    }

    Ok(())
}

// ─── Env ─────────────────────────────────────────────────────────────────────

struct Env {
    hook_port: String,
    hook_token: String,
    bot_id: String,
    pane_key: String,
}

impl Env {
    fn from_env(bot_id: &str) -> Self {
        Self {
            hook_port: std::env::var("ATLAS_HOOK_PORT").unwrap_or_default(),
            hook_token: std::env::var("ATLAS_HOOK_TOKEN").unwrap_or_default(),
            bot_id: std::env::var("ATLAS_BOT_ID").unwrap_or_else(|_| bot_id.to_string()),
            pane_key: std::env::var("ATLAS_PANE_KEY").unwrap_or_default(),
        }
    }

    fn hook_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.hook_port, path)
    }
}

// ─── Dispatch ────────────────────────────────────────────────────────────────

async fn dispatch(
    method: &str,
    params: Value,
    client: &reqwest::Client,
    env: &Env,
) -> Result<Value, String> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "atlas-bot-mcp", "version": env!("CARGO_PKG_VERSION") }
        })),

        "tools/list" => Ok(json!({
            "tools": tools_list()
        })),

        "tools/call" => {
            let name = params["name"]
                .as_str()
                .ok_or_else(|| "missing tool name".to_string())?;
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            call_tool(name, args, client, env).await
        }

        // ping — no-op, part of MCP spec
        "ping" => Ok(json!({})),

        other => Err(format!("method not found: {other}")),
    }
}

async fn handle_notification(
    msg: &Value,
    _out: &mut tokio::io::BufWriter<tokio::io::Stdout>,
) -> anyhow::Result<()> {
    // notifications/initialized and others: nothing to send back
    let _ = msg; // suppress lint
    Ok(())
}

// ─── Tool dispatch ────────────────────────────────────────────────────────────

async fn call_tool(
    name: &str,
    args: Value,
    client: &reqwest::Client,
    env: &Env,
) -> Result<Value, String> {
    match name {
        "ask_user" => tool_ask_user(args, client, env).await,
        "set_bot" => tool_set_bot(args, client, env).await,
        "approve" => tool_approve(args, client, env).await,
        other => Err(format!("unknown tool: {other}")),
    }
}

// ─── ask_user ─────────────────────────────────────────────────────────────────

async fn tool_ask_user(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    let question = args["question"]
        .as_str()
        .ok_or_else(|| "ask_user: missing 'question'".to_string())?;

    let ask_id = crate::paths::new_id();
    let options = args.get("options").cloned().unwrap_or(Value::Array(vec![]));
    let allow_text = args
        .get("allow_text")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let body = json!({
        "question": question,
        "options": options,
        "allow_text": allow_text,
        "pane_key": env.pane_key,
        "ask_id": ask_id,
    });

    let url = env.hook_url(&format!("/bots/{}/ask", env.bot_id));
    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ask_user: HTTP error: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("ask_user: server returned {status}: {text}"));
    }

    // Parse the answer value from the response body.
    let answer_text = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            // {kind:"key",value:"..."} or {kind:"text",value:"..."}
            v.get("value")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
        .unwrap_or(text);

    Ok(json!({
        "content": [{ "type": "text", "text": answer_text }]
    }))
}

// ─── set_bot ──────────────────────────────────────────────────────────────────

async fn tool_set_bot(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    let url = env.hook_url(&format!("/bots/{}/set_bot", env.bot_id));
    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&args)
        .send()
        .await
        .map_err(|e| format!("set_bot: HTTP error: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("set_bot: server returned {status}: {body}"));
    }

    Ok(json!({
        "content": [{ "type": "text", "text": "{\"ok\":true}" }]
    }))
}

// ─── approve ──────────────────────────────────────────────────────────────────

async fn tool_approve(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    let url = format!(
        "{}?pane_key={}",
        env.hook_url("/hook/claude/decision"),
        urlencoded(&env.pane_key)
    );

    let body = json!({
        "pane_key": env.pane_key,
        "payload": args,
    });

    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("approve: HTTP error: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("approve: server returned {status}: {text}"));
    }

    // Parse {"decision": "allow"|"deny"|null}
    let decision: Option<String> = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("decision").cloned())
        .filter(|d| !d.is_null())
        .and_then(|d| d.as_str().map(|s| s.to_string()));

    let result = match decision.as_deref() {
        Some("allow") | None => json!({ "behavior": "allow" }),
        _ => json!({
            "behavior": "deny",
            "message": "Permission denied by the user."
        }),
    };

    Ok(json!({
        "content": [{ "type": "text", "text": result.to_string() }]
    }))
}

// ─── Tool schemas ─────────────────────────────────────────────────────────────

fn tools_list() -> Value {
    json!([
        {
            "name": "ask_user",
            "description": "Ask the user a question and block until they answer (up to 30 min).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to present to the user."
                    },
                    "options": {
                        "type": "array",
                        "description": "Optional predefined answer choices.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "key":      { "type": "string" },
                                "title":    { "type": "string" },
                                "subtitle": { "type": "string" }
                            },
                            "required": ["key", "title"]
                        }
                    },
                    "allow_text": {
                        "type": "boolean",
                        "description": "Whether a free-text answer is accepted in addition to option keys."
                    }
                },
                "required": ["question"]
            }
        },
        {
            "name": "set_bot",
            "description": "Update this bot's metadata (name, role, description, triggers, project).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name":        { "type": "string" },
                    "role":        { "type": "string" },
                    "description": { "type": "string" },
                    "triggers":    { "type": "array" },
                    "project":     { "type": "object" }
                }
            }
        },
        {
            "name": "approve",
            "description": "Answer a Claude permission-prompt (use with --permission-prompt-tool atlas:approve).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name":   { "type": "string" },
                    "tool_input":  { "type": "object" },
                    "description": { "type": "string" }
                },
                "required": ["tool_name", "tool_input"]
            }
        }
    ])
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Percent-encode a string for use in a URL query parameter value.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
            }
        }
    }
    out
}

async fn write_line(
    out: &mut tokio::io::BufWriter<tokio::io::Stdout>,
    v: &Value,
) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(v)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoded_safe_chars() {
        assert_eq!(urlencoded("abc-123"), "abc-123");
    }

    #[test]
    fn urlencoded_encodes_special() {
        let encoded = urlencoded("foo bar/baz");
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('/'));
        assert!(encoded.contains('%'));
    }

    #[test]
    fn tools_list_has_three_tools() {
        let list = tools_list();
        assert_eq!(list.as_array().unwrap().len(), 3);
        let names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"ask_user"));
        assert!(names.contains(&"set_bot"));
        assert!(names.contains(&"approve"));
    }

    #[tokio::test]
    async fn dispatch_initialize() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let env = Env {
            hook_port: "9999".into(),
            hook_token: "tok".into(),
            bot_id: "bot1".into(),
            pane_key: "pane1".into(),
        };
        let result = dispatch("initialize", Value::Null, &client, &env)
            .await
            .unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn dispatch_tools_list() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let env = Env {
            hook_port: "9999".into(),
            hook_token: "tok".into(),
            bot_id: "bot1".into(),
            pane_key: "pane1".into(),
        };
        let result = dispatch("tools/list", Value::Null, &client, &env)
            .await
            .unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
    }

    #[tokio::test]
    async fn dispatch_unknown_method() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let env = Env {
            hook_port: "9999".into(),
            hook_token: "tok".into(),
            bot_id: "bot1".into(),
            pane_key: "pane1".into(),
        };
        let result = dispatch("nonexistent/method", Value::Null, &client, &env).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("method not found"));
    }
}
