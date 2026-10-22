//! Loopback HTTP endpoint for agent lifecycle events.
//!
//! `POST /hook/<agent>`, header `X-Atlas-Hook-Token`, body ≤ 1 MiB, reply `204`.
//! The port and token (new per daemon run) go to `<dir>/desktop/hook-endpoint.env`
//! (0600; `~/.atlas/desktop/…` by default), which the omp extension and
//! `atlas hook` re-read on every event, so they survive daemon restarts.
//!
//! `POST /hook/<agent>/decision` (a permission prompt) replies `{"decision": "allow"|"deny"|null}`
//! once a mobile client answers, or `null` at once when none is connected.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::AgentHub;
use crate::paths;

const MAX_BODY: usize = 1024 * 1024;
/// How long a permission prompt waits for a phone. Below the 60 s timeout the hook is
/// registered with (`atlas setup`); `atlas hook` waits this plus a margin.
pub const DECISION_WAIT: std::time::Duration = std::time::Duration::from_secs(55);
/// How long a bot-pane permission prompt waits (30 min). Read from ATLAS_HOOK_DECISION_SECS in the pty env.
pub const BOT_DECISION_WAIT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

fn write_endpoint(dir: &Path, port: u16, token: &str) -> std::io::Result<()> {
    paths::write_private(
        &paths::hook_endpoint(dir),
        format!("ATLAS_HOOK_PORT={port}\nATLAS_HOOK_TOKEN={token}\n").as_bytes(),
    )
}

/// Bind the hook server, publish its endpoint file and serve until the process exits.
/// `mobile` says whether a connected phone may answer a permission prompt.
pub async fn start(
    dir: &Path,
    hub: Arc<AgentHub>,
    mobile: Arc<crate::mobile::Mobile>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = paths::random_hex(24)?;
    *hub.port.lock() = port;
    write_endpoint(dir, port, &token)?;
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (token, hub, mobile) = (token.clone(), hub.clone(), mobile.clone());
            tokio::spawn(async move {
                let _ = serve(stream, &hub, &mobile, &token).await;
            });
        }
    });
    Ok(())
}

async fn serve(
    stream: TcpStream,
    hub: &AgentHub,
    mobile: &crate::mobile::Mobile,
    token: &str,
) -> std::io::Result<()> {
    let (r, mut w) = stream.into_split();
    let mut r = BufReader::new(r);
    let mut line = String::new();
    r.read_line(&mut line).await?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let (mut len, mut tok) = (0usize, String::new());
    loop {
        line.clear();
        if r.read_line(&mut line).await? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "x-atlas-hook-token" => tok = v.trim().to_string(),
                _ => {}
            }
        }
    }
    let (status, body) = if method.as_str() != "POST" {
        ("405 Method Not Allowed", String::new())
    } else if tok != token {
        ("401 Unauthorized", String::new())
    } else if len > MAX_BODY {
        ("413 Payload Too Large", String::new())
    } else {
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).await?;
        match serde_json::from_slice::<Value>(&body) {
            Err(_) => ("400 Bad Request", String::new()),
            Ok(v) => route(hub, mobile.can_approve(), &path, &v).await,
        }
    };
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    w.write_all(resp.as_bytes()).await
}

async fn route(hub: &AgentHub, can_approve: bool, path: &str, v: &Value) -> (&'static str, String) {
    let Some(rest) = path.strip_prefix("/hook/") else {
        return ("404 Not Found", String::new());
    };
    let (agent, decision) = match rest.strip_suffix("/decision") {
        Some(a) => (a, true),
        None => (rest, false),
    };
    if agent.is_empty() || agent.contains('/') {
        return ("404 Not Found", String::new());
    }
    if !decision {
        hub.apply(agent, v);
        return ("204 No Content", String::new());
    }
    // Permission prompt: block until a phone answers (or `DECISION_WAIT`). Hooks without a
    // pane key cannot be routed to a tab, so they get no remote answer.
    let answer = match v["pane_key"].as_str() {
        Some(pane) => {
            hub.ask(pane, agent, &v["payload"], DECISION_WAIT, can_approve)
                .await
        }
        None => None,
    };
    ("200 OK", decision_body(answer))
}

/// The `/decision` reply: `{"decision": "allow"|"deny"|null}`. One place writes it and
/// [`parse_decision`] (used by `atlas hook`) reads it.
pub fn decision_body(answer: Option<bool>) -> String {
    let d = answer.map(|a| if a { "allow" } else { "deny" });
    serde_json::json!({ "decision": d }).to_string()
}

/// Read a `/decision` reply: `Some(true)` allow, `Some(false)` deny, `None` no answer.
pub fn parse_decision(body: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(body).ok()?;
    match v["decision"].as_str()? {
        "allow" => Some(true),
        "deny" => Some(false),
        _ => None,
    }
}
