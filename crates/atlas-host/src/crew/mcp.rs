//! Crewmate MCP stdio server (`atlas mcp --crewmate <id>`).
//!
//! Implements a minimal JSON-RPC 2.0 MCP server over stdin/stdout.
//! Exposes five tools to the agent running inside the crewmate:
//!
//! - **`ask_user`**  — post a question to the mobile/desktop UI and block until
//!   answered (up to 30 min).
//! - **`set_profile`**   — update this crewmate's name or role.
//! - **`capture`**   — screenshots of a URL in real browsers (chromium + webkit, widths, themes),
//!   attached to the task; an incomplete matrix is an error (`crew::capture`).
//! - **`locate`**    — which route, component and selector draw a region of a screenshot
//!   (`crew::locate`); asks for `ask_user` when two candidates are equally likely.
//! - **`ui_gate`**   — the fast UI gate: capture only the scenarios a change needs, rerun only the
//!   failed ones, register a fixed bug as a permanent regression (`crew::uigate`).
//! - **`approve`**   — answer a Claude permission-prompt (used with
//!   `--permission-prompt-tool atlas:approve`).
//!
//! Reads from env:
//!   `ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`, `ATLAS_CREWMATE_ID`, `ATLAS_PANE_KEY`

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Serve the crewmate MCP protocol on stdin/stdout until EOF.
pub async fn serve(crewmate_id: &str) -> anyhow::Result<()> {
    // Install the ring crypto provider so rustls-no-provider works.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1800)) // ask_user can take up to 30 min
        .build()?;

    let env = Env::from_env(crewmate_id);

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
    crewmate_id: String,
    pane_key: String,
}

impl Env {
    fn from_env(crewmate_id: &str) -> Self {
        Self {
            hook_port: std::env::var("ATLAS_HOOK_PORT").unwrap_or_default(),
            hook_token: std::env::var("ATLAS_HOOK_TOKEN").unwrap_or_default(),
            crewmate_id: std::env::var("ATLAS_CREWMATE_ID")
                .unwrap_or_else(|_| crewmate_id.to_string()),
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
            "serverInfo": { "name": "atlas-crew-mcp", "version": env!("CARGO_PKG_VERSION") }
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
        "set_profile" => tool_set_profile(args, client, env).await,
        "capture" => tool_capture(args, client, env).await,
        "locate" => tool_locate(args, client, env).await,
        "ui_gate" => tool_ui_gate(args, client, env).await,
        "set_ui" => tool_set_ui(client, env).await,
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

    let url = env.hook_url(&format!("/crew/{}/ask", env.crewmate_id));
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

// ─── set_profile ──────────────────────────────────────────────────────────────────

async fn tool_set_profile(
    args: Value,
    client: &reqwest::Client,
    env: &Env,
) -> Result<Value, String> {
    let url = env.hook_url(&format!("/crew/{}/set_profile", env.crewmate_id));
    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&args)
        .send()
        .await
        .map_err(|e| format!("set_profile: HTTP error: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("set_profile: server returned {status}: {body}"));
    }

    Ok(json!({
        "content": [{ "type": "text", "text": "{\"ok\":true}" }]
    }))
}

// ─── capture ──────────────────────────────────────────────────────────────────

/// Post the request to hostd, which runs the browsers (they are on the crew server, not in this
/// process) and answers with the combinations visited or `{"error": …}`.
async fn tool_capture(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    post_task_tool("capture", args, client, env).await
}

// ─── locate ───────────────────────────────────────────────────────────────────

/// Same transport as `capture`: hostd reads the work tree and runs the browser.
async fn tool_locate(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    post_task_tool("locate", args, client, env).await
}

// ─── ui_gate ──────────────────────────────────────────────────────────────────

/// Same road as `capture`: hostd plans, captures and remembers; this process only relays.
async fn tool_ui_gate(args: Value, client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    post_task_tool("ui_gate", args, client, env).await
}

/// `POST /crew/<id>/<tool>` with the pane key and the arguments; the answer text is the tool result,
/// `{"error": …}` is the tool's error.
async fn post_task_tool(
    tool: &str,
    args: Value,
    client: &reqwest::Client,
    env: &Env,
) -> Result<Value, String> {
    let url = env.hook_url(&format!("/crew/{}/{tool}", env.crewmate_id));
    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&json!({ "pane_key": env.pane_key, "args": args }))
        .send()
        .await
        .map_err(|e| format!("{tool}: HTTP error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let why = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("server returned {status}: {text}"));
        return Err(why);
    }
    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
}

// ─── set_ui ───────────────────────────────────────────────────────────────────

/// Post the mark to hostd, which records it on the task the pane names.
async fn tool_set_ui(client: &reqwest::Client, env: &Env) -> Result<Value, String> {
    let url = env.hook_url(&format!("/crew/{}/set_ui", env.crewmate_id));
    let resp = client
        .post(&url)
        .header("X-Atlas-Hook-Token", &env.hook_token)
        .json(&json!({ "pane_key": env.pane_key }))
        .send()
        .await
        .map_err(|e| format!("set_ui: HTTP error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let why = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("server returned {status}: {text}"));
        return Err(why);
    }
    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
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
            "name": "set_profile",
            "description": "Update this crewmate's name or role. Automations are not set here: propose one with the add_trigger action so the user approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name":        { "type": "string" },
                    "role":        { "type": "string" }
                }
            }
        },
        {
            "name": "capture",
            "description": "Open a URL in real browsers and attach screenshots to this task: one per engine x width x theme (default chromium + webkit, 390 + 1440 px, light + dark, full page). Fails, naming them, when any combination is missing, so a capture that returns is complete. localhost and LAN URLs work. Use it to look at a UI change before and after you make it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "http:// or https:// only." },
                    "widths": { "type": "array", "items": { "type": "integer" }, "description": "Viewport widths in px (200-4000). Default [390, 1440]." },
                    "themes": { "type": "array", "items": { "type": "string", "enum": ["light", "dark"] } },
                    "engines": { "type": "array", "items": { "type": "string", "enum": ["chromium", "webkit"] } },
                    "slow_network": { "type": "boolean", "description": "Delay every request (a poor connection)." },
                    "full_page": { "type": "boolean", "description": "Scroll to the end and shoot the whole page. Default true." },
                    "dom": { "type": "boolean", "description": "Also write each cell's element boxes (tag, role, name, text, data-testid, selector, x/y/w/h in CSS px) as <name>.dom.json next to the PNG; each shot then lists it as `dom`. Default false. The `locate` tool does this for you." },
                    "checks": { "type": "boolean", "description": "Also run the UI detectors (empty band, gutter, sibling overlap, raw text, action hierarchy, layout shift, focus after activation) and write each cell's findings as <name>.findings.json. Default false. The `ui_gate` tool does this for you." },
                    "steps": {
                        "type": "array",
                        "description": "Before the screenshot, at most 20: {do:'click'|'fill'|'press'|'wait_for'|'wait_ms', selector?, text?, key?, ms?}.",
                        "items": { "type": "object" }
                    }
                },
                "required": ["url"]
            }
        },
        {
            "name": "locate",
            "description": "Find where a screenshot is drawn in the code. Give the image (a path in your work tree, e.g. .atlas/attachments/<task>/<name>), optionally a rect [x, y, w, h] in the image's pixels, and the `url` of the page it shows (a dev server on this machine works): atlas opens that page in a real browser, ranks the elements under the rect (or the page's main landmark when there is no rect) and maps the URL to route files (SvelteKit src/routes/**/+page.svelte, Next app/**/page.tsx and pages/**, Flutter GoRoute(path:)) and the element's data-testid or text to the files that contain it. Returns `candidates` [{route_file, component_file, selector, testid, text, score}] best first, and `ambiguous: true` when the top two are within 10% of each other. When `ambiguous` is true you MUST call ask_user with the candidates as options (each option: its selector and text) and only then edit; never pick one yourself.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image": { "type": "string", "description": "Path of the screenshot inside your work tree." },
                    "rect": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "[x, y, w, h] of the region, in the image's pixels. Omit for the whole page." },
                    "url": { "type": "string", "description": "http(s) URL of the page the image shows. Required: the element boxes come from the live page." },
                    "width": { "type": "integer", "description": "Viewport width in px to open the page at. Default: the image's width when it is 390 or 1440, else 1440." }
                },
                "required": ["image", "url"]
            }
        },
        {
            "name": "ui_gate",
            "description": "The fast UI gate. `run {base_url}` plans from your change: the routes the diff touches (atlas.yaml `affected:` rules) plus every registered regression, captures each scenario (chromium + webkit, 390 + 1440 px, light + dark) and answers pass/fail per scenario with the capture names. A scenario that already passed over the same project bytes is a cache hit and is not captured again. `rerun_failed` captures only the scenarios that failed last time. `add_regression {url, note}` registers a UI bug you just fixed in `.atlas/ui-regressions.json`: commit that file with your fix, and every later run replays it. Run it before you finish a UI task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["run", "rerun_failed", "add_regression"] },
                    "base_url": { "type": "string", "description": "run: where the app is served, e.g. http://localhost:5173. Routes are joined to it." },
                    "routes": { "type": "array", "items": { "type": "string" }, "description": "run: pages to check instead of the ones the diff implies (paths starting with /)." },
                    "files": { "type": "array", "items": { "type": "string" }, "description": "run: changed files instead of the git diff, project-relative." },
                    "force": { "type": "boolean", "description": "run: ignore the cache and capture everything (the data behind the page changed)." },
                    "url": { "type": "string", "description": "add_regression: the page that was broken." },
                    "note": { "type": "string", "description": "add_regression: what was wrong." },
                    "widths": { "type": "array", "items": { "type": "integer" }, "description": "add_regression: widths of the scenario (default 390, 1440)." },
                    "themes": { "type": "array", "items": { "type": "string", "enum": ["light", "dark"] } },
                    "engines": { "type": "array", "items": { "type": "string", "enum": ["chromium", "webkit"] } }
                },
                "required": ["action"]
            }
        },
        {
            "name": "set_ui",
            "description": "Mark this task as a UI task: it changes what people see. A UI task is only done with at least two captures you took (use the capture tool, before and after your change); finishing without them asks the user instead. Call it when the task turns out to change the interface and nobody said so.",
            "inputSchema": { "type": "object", "properties": {} }
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

/// What the agent's MCP client does for one `tools/call`, against the hostd whose hook endpoint is
/// in `hostd_dir`: the real dispatch, the real HTTP request, as crewmate `crewmate_id` in pane
/// `pane_key`. Lets a test cross the whole path (MCP, hook route, hostd, runner).
#[cfg(all(test, unix))]
pub(crate) async fn call_for_test(
    hostd_dir: &std::path::Path,
    crewmate_id: &str,
    pane_key: &str,
    tool: &str,
    arguments: Value,
) -> Result<Value, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let endpoint = std::fs::read_to_string(crate::paths::hook_endpoint(hostd_dir))
        .map_err(|e| format!("no hook endpoint: {e}"))?;
    let get = |k: &str| {
        endpoint
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let env = Env {
        hook_port: get("ATLAS_HOOK_PORT"),
        hook_token: get("ATLAS_HOOK_TOKEN"),
        crewmate_id: crewmate_id.to_string(),
        pane_key: pane_key.to_string(),
    };
    let params = json!({"name": tool, "arguments": arguments});
    dispatch("tools/call", params, &reqwest::Client::new(), &env).await
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
    fn tools_list_names_every_tool_exactly_once() {
        // By name, not by count: a count went stale every time two branches each added a tool.
        let list = tools_list();
        let mut all: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        all.sort_unstable();
        assert_eq!(
            all,
            [
                "approve",
                "ask_user",
                "capture",
                "locate",
                "set_profile",
                "set_ui",
                "ui_gate"
            ]
        );
    }

    // The description is the only place the agent learns the ask_user rule.
    #[test]
    fn the_locate_description_tells_the_agent_to_ask_when_ambiguous() {
        let list = tools_list();
        let t = list
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "locate")
            .unwrap();
        let d = t["description"].as_str().unwrap();
        assert!(
            d.contains("ambiguous") && d.contains("ask_user") && d.contains("10%"),
            "{d}"
        );
        assert_eq!(t["inputSchema"]["required"], json!(["image", "url"]));
    }

    #[tokio::test]
    async fn dispatch_initialize() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let env = Env {
            hook_port: "9999".into(),
            hook_token: "tok".into(),
            crewmate_id: "bot1".into(),
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
            crewmate_id: "bot1".into(),
            pane_key: "pane1".into(),
        };
        let result = dispatch("tools/list", Value::Null, &client, &env)
            .await
            .unwrap();
        // The same list the MCP client is told about.
        assert_eq!(result["tools"], tools_list());
    }

    #[tokio::test]
    async fn dispatch_unknown_method() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let env = Env {
            hook_port: "9999".into(),
            hook_token: "tok".into(),
            crewmate_id: "bot1".into(),
            pane_key: "pane1".into(),
        };
        let result = dispatch("nonexistent/method", Value::Null, &client, &env).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("method not found"));
    }
}
