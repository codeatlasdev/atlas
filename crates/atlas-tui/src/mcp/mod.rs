//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited), hand-rolled:
//! initialize, ping, tools/*, resources/*, prompts/*, notifications/initialized.
//! stdout carries protocol messages only; diagnostics go to stderr.

pub mod logs;
pub mod prompts;
pub mod resources;
pub mod tools;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufReader};

/// Latest protocol revision we implement. Clients requesting anything we don't
/// know get this back from `initialize` and decide whether to proceed (MCP
/// lifecycle → version negotiation).
///
/// 2025-11-25 changelog review (https://modelcontextprotocol.io/specification/2025-11-25/changelog):
/// nothing there changes a tools-only stdio server. Icons, tasks, URL
/// elicitation, sampling-with-tools and the OAuth/HTTP items are optional or
/// HTTP-only; JSON Schema 2020-12 as default dialect is satisfied by our plain
/// `{"type":"object",…}` schemas; SEP-1303 (input-validation failures are tool
/// execution errors, not protocol errors) is how `tools::call` already behaves;
/// stderr may carry any logging (we only use it for diagnostics).
pub const PROTOCOL_VERSION: &str = "2025-11-25";
/// Revisions we accept verbatim in `initialize` (echoed back when requested).
const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// JSON-RPC batching exists only in this revision: added in 2025-03-26, removed
/// in 2025-06-18 (changelog "Remove support for JSON-RPC batching"), absent from 2024-11-05.
const BATCH_VERSION: &str = "2025-03-26";

const INSTRUCTIONS: &str = "Atlas dev-environment tools for this project. Start with atlas_status. \
Read tools (status, logs, logs_search, config) work from files even when `atlas dev` is not running. \
Control tools (restart/stop/start service, run_action, wait_ready) need `atlas dev` running in a terminal. \
Actions with a confirm message need the user's approval before you pass confirmed=true.";

// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub struct Server {
    root: PathBuf,
    /// Version agreed in `initialize` (per connection); gates JSON-RPC batching.
    negotiated: std::sync::Mutex<Option<&'static str>>,
}

impl Server {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            negotiated: std::sync::Mutex::new(None),
        }
    }

    fn negotiated(&self) -> Option<&'static str> {
        self.negotiated.lock().ok().and_then(|g| *g)
    }

    /// Handle one raw line. `None` = nothing to send (notification or all-notification batch).
    pub async fn handle_line(&self, line: &str) -> Option<Value> {
        match serde_json::from_str(line) {
            Ok(msg) => self.handle_message(msg).await,
            Err(e) => Some(error(Value::Null, PARSE_ERROR, format!("parse error: {e}"))),
        }
    }

    /// Handle one already-parsed JSON value (single message or batch).
    pub async fn handle_message(&self, msg: Value) -> Option<Value> {
        match msg {
            Value::Array(items) => {
                if self.negotiated() != Some(BATCH_VERSION) {
                    return Some(error(
                        Value::Null,
                        INVALID_REQUEST,
                        format!(
                            "JSON-RPC batching is only allowed with protocol version {BATCH_VERSION}"
                        ),
                    ));
                }
                if items.is_empty() {
                    return Some(error(Value::Null, INVALID_REQUEST, "empty batch".into()));
                }
                let replies =
                    futures_util::future::join_all(items.into_iter().map(|m| self.handle(m))).await;
                let replies: Vec<Value> = replies.into_iter().flatten().collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            other => self.handle(other).await,
        }
    }

    /// Handle one JSON-RPC message object.
    pub async fn handle(&self, msg: Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "request must be a JSON object".into(),
            ));
        };
        // A valid id is a string or number; anything else can't be echoed safely.
        let id = match obj.get("id") {
            None => None,
            Some(v @ (Value::String(_) | Value::Number(_))) => Some(v.clone()),
            Some(_) => {
                return Some(error(
                    Value::Null,
                    INVALID_REQUEST,
                    "id must be a string or number".into(),
                ));
            }
        };
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // No method: either a client response (ignored) or junk (flagged when it has an id).
            let is_response = obj.contains_key("result") || obj.contains_key("error");
            return match id {
                Some(id) if !is_response => {
                    Some(error(id, INVALID_REQUEST, "missing method".into()))
                }
                None if !is_response => {
                    Some(error(Value::Null, INVALID_REQUEST, "missing method".into()))
                }
                _ => None,
            };
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(
                id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "jsonrpc must be \"2.0\"".into(),
            ));
        }
        // No id → notification: never answered, even when the method is unknown.
        let id = id?;
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => {
                let result = initialize_result(&params);
                if let Some(v) = SUPPORTED_VERSIONS
                    .iter()
                    .find(|s| result["protocolVersion"] == **s)
                    && let Ok(mut g) = self.negotiated.lock()
                {
                    *g = Some(v);
                }
                ok(id, result)
            }
            // ping works in any lifecycle phase; so do tools/* (clients that skip
            // the handshake still get served — the tools are read-mostly and local).
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools::catalogue() })),
            "tools/call" => self.tools_call(id, &params).await,
            "resources/list" => ok(id, json!({ "resources": resources::list() })),
            "resources/templates/list" => {
                ok(id, json!({ "resourceTemplates": resources::templates() }))
            }
            "resources/read" => match params.get("uri").and_then(Value::as_str) {
                None => error(
                    id,
                    INVALID_PARAMS,
                    "resources/read requires params.uri".into(),
                ),
                Some(uri) => match resources::read(&self.root, uri) {
                    Ok(r) => ok(id, r),
                    Err(e) => error(id, INVALID_PARAMS, e),
                },
            },
            "prompts/list" => ok(id, json!({ "prompts": prompts::list() })),
            "prompts/get" => {
                let empty = serde_json::Map::new();
                let args = params
                    .get("arguments")
                    .and_then(Value::as_object)
                    .unwrap_or(&empty);
                match params.get("name").and_then(Value::as_str) {
                    None => error(
                        id,
                        INVALID_PARAMS,
                        "prompts/get requires params.name".into(),
                    ),
                    Some(name) => match prompts::get(name, args) {
                        Ok(r) => ok(id, r),
                        Err(e) => error(id, INVALID_PARAMS, e),
                    },
                }
            }
            other => error(id, METHOD_NOT_FOUND, format!("method not found: {other}")),
        })
    }

    async fn tools_call(&self, id: Value, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error(id, INVALID_PARAMS, "tools/call requires params.name".into());
        };
        let args = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(a) if a.is_object() => a.clone(),
            Some(_) => {
                return error(id, INVALID_PARAMS, "arguments must be an object".into());
            }
        };
        // Unknown tool is a protocol error; everything else is a tool-level result.
        if !tools::is_tool(name) {
            return error(id, INVALID_PARAMS, format!("unknown tool: {name}"));
        }
        let (text, is_error) = match tools::call(&self.root, name, &args).await {
            Ok(t) => (t, false),
            Err(e) => (e, true),
        };
        ok(
            id,
            json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
        )
    }
}

fn initialize_result(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|v| SUPPORTED_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false },
            "prompts": { "listChanged": false },
        },
        "serverInfo": { "name": "atlas", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

async fn write_msg<W: AsyncWrite + Unpin>(out: &mut W, v: &Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    out.write_all(s.as_bytes()).await?;
    out.flush().await
}

/// Serve MCP on stdin/stdout until stdin closes. Requests run concurrently so a
/// long `atlas_wait_ready` never blocks `ping` or other calls.
pub async fn serve(root: &Path) -> anyhow::Result<()> {
    serve_io(root, tokio::io::stdin(), tokio::io::stdout()).await
}

pub async fn serve_io<R, W>(root: &Path, input: R, output: W) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let server = std::sync::Arc::new(Server::new(root));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut output = output;
        while let Some(v) = rx.recv().await {
            if write_msg(&mut output, &v).await.is_err() {
                break;
            }
        }
    });

    let mut input = BufReader::new(input);
    let mut inflight = tokio::task::JoinSet::new();
    let mut buf = Vec::new();
    let mut result = Ok(());
    loop {
        buf.clear();
        // Byte-oriented, bounded read: invalid UTF-8 becomes a parse error, not a
        // dead server; a line past the limit ends the session instead of eating RAM.
        match crate::control::read_bounded_line(&mut input, &mut buf).await {
            Ok(Some(())) => {}
            Ok(None) => break,
            Err(e) => {
                result = Err(anyhow::anyhow!(
                    "stdin line exceeds {} bytes or failed to read: {e}",
                    crate::control::MAX_LINE_BYTES
                ));
                break;
            }
        }
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(error(Value::Null, PARSE_ERROR, format!("parse error: {e}")));
                continue;
            }
        };
        // `initialize` fixes the protocol version that later lines (batching) depend on,
        // so it is handled in arrival order; everything else runs concurrently.
        if msg.get("method").and_then(Value::as_str) == Some("initialize") {
            if let Some(resp) = server.handle_message(msg).await {
                let _ = tx.send(resp);
            }
            continue;
        }
        let server = server.clone();
        let tx = tx.clone();
        inflight.spawn(async move {
            if let Some(resp) = server.handle_message(msg).await {
                let _ = tx.send(resp);
            }
        });
    }
    // stdin closed: let in-flight calls answer before exiting.
    while inflight.join_next().await.is_some() {}
    drop(tx);
    let _ = writer.await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;

    async fn rpc(server: &Server, v: Value) -> Option<Value> {
        server.handle_line(&v.to_string()).await
    }

    fn server() -> (tempfile::TempDir, Server) {
        let d = tempfile::TempDir::new().unwrap();
        let s = Server::new(d.path());
        (d, s)
    }

    #[tokio::test]
    async fn initialize_negotiates_version_and_declares_tools() {
        let (_d, s) = server();
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}))
            .await
            .unwrap();
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "atlas");

        // Unknown future version → we answer with our latest.
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}))
            .await
            .unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
        let caps = &r["result"]["capabilities"];
        assert!(caps["resources"].is_object() && caps["prompts"].is_object());
    }

    #[tokio::test]
    async fn resources_list_and_read_status_and_logs() {
        let (d, s) = server();
        std::fs::create_dir_all(d.path().join(".logs")).unwrap();
        std::fs::write(
            d.path().join(".logs/api.log"),
            "10:00:00 [stdout] one\n10:00:01 [stdout] two\n10:00:02 [stderr] three\n",
        )
        .unwrap();
        let r = rpc(
            &s,
            json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
        )
        .await
        .unwrap();
        assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 2);
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"atlas://logs/api"}}))
            .await
            .unwrap();
        let text = r["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("one") && text.contains("two") && text.contains("three"));
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"atlas://status"}}))
            .await
            .unwrap();
        assert!(
            r["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("\"running\": false")
        );
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"atlas://nope"}}))
            .await
            .unwrap();
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn prompts_list_and_get_diagnose_substitutes_service() {
        let (_d, s) = server();
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}))
            .await
            .unwrap();
        assert_eq!(r["result"]["prompts"].as_array().unwrap().len(), 3);
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"diagnose","arguments":{"service":"api"}}}))
            .await
            .unwrap();
        let text = r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("atlas_restart_service(\"api\")"));
        let r = rpc(
            &s,
            json!({"jsonrpc":"2.0","id":3,"method":"prompts/get","params":{"name":"diagnose"}}),
        )
        .await
        .unwrap();
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        let (_d, s) = server();
        assert!(
            rpc(
                &s,
                json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .await
            .is_none()
        );
    }

    #[tokio::test]
    async fn ping_and_tools_list() {
        let (_d, s) = server();
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":"a","method":"ping"}))
            .await
            .unwrap();
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"], json!({}));
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
            .await
            .unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 9);
    }

    #[tokio::test]
    async fn unknown_method_and_bad_json_are_jsonrpc_errors() {
        let (_d, s) = server();
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":4,"method":"nope"}))
            .await
            .unwrap();
        assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
        let r = s.handle_line("{not json").await.unwrap();
        assert_eq!(r["error"]["code"], PARSE_ERROR);
        assert_eq!(r["id"], Value::Null);
    }

    #[tokio::test]
    async fn tools_call_dispatches_and_wraps_tool_errors_as_is_error() {
        let (_d, s) = server();
        let r = rpc(
            &s,
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"atlas_status"}}),
        )
        .await
        .unwrap();
        assert_eq!(r["result"]["isError"], false);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"running\": false"));

        // Control tool without a running atlas: tool-level error, not protocol error.
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"atlas_restart_service","arguments":{"service":"api"}}}))
            .await
            .unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(
            r["result"]["content"][0]["text"],
            "atlas dev is not running; start it in a terminal"
        );
    }

    #[tokio::test]
    async fn tools_call_rejects_unknown_tool_and_bad_params() {
        let (_d, s) = server();
        let r = rpc(
            &s,
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"rm"}}),
        )
        .await
        .unwrap();
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
        let r = rpc(
            &s,
            json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{}}),
        )
        .await
        .unwrap();
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
        let r = rpc(&s, json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"atlas_logs","arguments":[1]}}))
            .await
            .unwrap();
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn serve_io_round_trips_lines_and_exits_on_eof() {
        let d = tempfile::TempDir::new().unwrap();
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let (client, server_out) = tokio::io::duplex(4096);
        serve_io(d.path(), &input[..], server_out).await.unwrap();
        let mut lines = BufReader::new(client).lines();
        let first = lines.next_line().await.unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&first).unwrap()["id"], 1);
        assert!(lines.next_line().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn serve_io_rejects_oversized_line() {
        let d = tempfile::TempDir::new().unwrap();
        let input = vec![b'a'; crate::control::MAX_LINE_BYTES * 2];
        let (_client, server_out) = tokio::io::duplex(4096);
        let r = serve_io(d.path(), &input[..], server_out).await;
        assert!(r.is_err());
    }
}
