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
/// How long a crewmate-pane permission prompt waits (30 min). Read from ATLAS_HOOK_DECISION_SECS in the pty env.
pub const CREWMATE_DECISION_WAIT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

fn write_endpoint(dir: &Path, port: u16, token: &str) -> std::io::Result<()> {
    paths::write_private(
        &paths::hook_endpoint(dir),
        format!("ATLAS_HOOK_PORT={port}\nATLAS_HOOK_TOKEN={token}\n").as_bytes(),
    )
}

/// Bind the hook server, publish its endpoint file and serve until the process exits.
/// `mobile` says whether a connected phone may answer a permission prompt.
/// `crew_hub` is an optional weak reference to the full `Hub`; when present, crewmate routes
/// (`/crew/…`) are handled without going through the agent hub.
pub(crate) async fn start(
    dir: &Path,
    hub: Arc<AgentHub>,
    mobile: Arc<crate::mobile::Mobile>,
    crew_hub: Option<std::sync::Weak<crate::server::Hub>>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = paths::random_hex(24)?;
    *hub.port.lock() = port;
    write_endpoint(dir, port, &token)?;
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (token, hub, mobile, bh) =
                (token.clone(), hub.clone(), mobile.clone(), crew_hub.clone());
            tokio::spawn(async move {
                let _ = serve(stream, &hub, &mobile, &token, bh).await;
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
    crew_hub: Option<std::sync::Weak<crate::server::Hub>>,
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
        let mut body_bytes = vec![0u8; len];
        r.read_exact(&mut body_bytes).await?;
        // Crewmate routes may have non-JSON bodies (webhook payloads). `/bots/` is only the
        // pre-rename webhook URL (`crew::triggers::webhook`).
        if path.starts_with("/crew/")
            || path.starts_with(crate::crew::triggers::webhook::LEGACY_PREFIX)
        {
            if let Some(bh) = crew_hub.as_ref().and_then(|w| w.upgrade()) {
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();
                route_crew(&bh, &path, &body_str).await
            } else {
                ("503 Service Unavailable", String::new())
            }
        } else {
            match serde_json::from_slice::<Value>(&body_bytes) {
                Err(_) => ("400 Bad Request", String::new()),
                Ok(v) => route(hub, mobile.can_approve(), &path, &v).await,
            }
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
            let wait = if crate::crew::pane::is_crewmate_pane(pane) {
                CREWMATE_DECISION_WAIT
            } else {
                DECISION_WAIT
            };
            hub.ask(pane, agent, &v["payload"], wait, can_approve).await
        }
        None => None,
    };
    ("200 OK", decision_body(answer))
}

/// Route a crewmate-specific request (path under `/crew/`, or a pre-rename `/bots/` webhook).
///
/// Handles:
/// - `POST /crew/<id>/hook/<token>`  — webhook trigger (`webhook::parse_webhook_path` owns the shape)
/// - `POST /crew/<id>/ask`           — ask_user call from an engine; blocks until answered
/// - `POST /crew/<id>/set_profile`   — update crewmate name/role from within a task
async fn route_crew(
    hub: &Arc<crate::server::Hub>,
    path: &str,
    body: &str,
) -> (&'static str, String) {
    if let Some((crewmate_id, token)) = crate::crew::triggers::webhook::parse_webhook_path(path) {
        let status = crate::crew::triggers::webhook::handle(hub, &crewmate_id, &token, body);
        return (status, String::new());
    }
    let Some((crewmate_id, subpath)) = path
        .strip_prefix("/crew/")
        .and_then(|rest| rest.split_once('/'))
        .filter(|(id, _)| !id.is_empty())
    else {
        return ("404 Not Found", String::new());
    };
    let v = || serde_json::from_str::<Value>(body).unwrap_or_default();
    match subpath {
        "ask" => handle_ask(hub, crewmate_id, &v()).await,
        "set_profile" => handle_set_profile(hub, crewmate_id, &v()),
        _ => ("404 Not Found", String::new()),
    }
}

/// Handle `POST /crew/<id>/ask`: record the ask on the task, block until answered (≤30 min).
async fn handle_ask(
    hub: &Arc<crate::server::Hub>,
    crewmate_id: &str,
    v: &Value,
) -> (&'static str, String) {
    use crate::crew::store::{AskAnswer, AskOption, AskRequest};

    let ask_id = v["ask_id"].as_str().unwrap_or("").to_string();
    let question = v["question"].as_str().unwrap_or("").to_string();
    if ask_id.is_empty() || question.is_empty() {
        return ("400 Bad Request", String::new());
    }
    let options: Vec<AskOption> = serde_json::from_value(v["options"].clone()).unwrap_or_default();
    let allow_text = v["allow_text"].as_bool().unwrap_or(false);
    let pane_key = v["pane_key"].as_str().unwrap_or("").to_string();

    let ask = AskRequest {
        id: ask_id.clone(),
        question,
        options,
        allow_text,
        answered: None,
    };

    // Attach ask to the running task so clients can display it.
    if !pane_key.is_empty() {
        // pane_key format: "crew:<crewmate_id>:<task_id>" (`crew::pane`)
        let parts: Vec<&str> = pane_key.splitn(3, ':').collect();
        if parts.len() == 3 {
            let task_id = parts[2];
            let tasks = hub.crew.store.read_tasks(crewmate_id, 500);
            if let Some(mut t) = tasks.into_iter().find(|t| t.id == task_id) {
                t.ask = Some(ask.clone());
                let _ = hub.crew.store.append_task(crewmate_id, &t);
                hub.broadcast(crate::crew::events::task(crewmate_id, Some(&t)));
            }
        }
    }

    // Register waiter and block until answered or timed out (30 min).
    let rx = hub.crew.wait_ask(ask_id.clone());
    match tokio::time::timeout(std::time::Duration::from_secs(30 * 60), rx).await {
        Ok(Ok(answer)) => {
            let text = match &answer {
                AskAnswer::Key { value } => value.clone(),
                AskAnswer::Text { value } => value.clone(),
            };
            (
                "200 OK",
                serde_json::json!({"answer": text, "answer_raw": answer}).to_string(),
            )
        }
        _ => ("504 Gateway Timeout", String::new()),
    }
}

/// Handle `POST /crew/<id>/set_profile`: update name/role on the crewmate from within a task.
fn handle_set_profile(
    hub: &Arc<crate::server::Hub>,
    crewmate_id: &str,
    v: &Value,
) -> (&'static str, String) {
    let Ok(mut crewmate) = hub.crew.store.load_crewmate(crewmate_id) else {
        return ("404 Not Found", String::new());
    };
    if let Some(name) = v["name"].as_str() {
        let name = name.trim().to_string();
        if name.is_empty() || name.len() > 40 {
            return ("400 Bad Request", String::new());
        }
        crewmate.name = name;
    }
    if let Some(role) = v["role"].as_str() {
        if role.len() > 2000 {
            return ("400 Bad Request", String::new());
        }
        crewmate.role = role.to_string();
    }
    match hub.crew.store.save_crewmate(&crewmate) {
        Ok(()) => ("204 No Content", String::new()),
        Err(_) => ("500 Internal Server Error", String::new()),
    }
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
