//! Loopback HTTP endpoint for agent lifecycle events.
//!
//! `POST /hook/<agent>`, header `X-Atlas-Hook-Token`, body ≤ 1 MiB,
//! reply `204`. The port and token (new per app run) go to
//! `~/.atlas/desktop/hook-endpoint.env` (0600), which the omp extension and
//! `atlas hook` read on every event, so they survive app restarts.
//!
//! Events: `agent://status {pane_key, status}`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const MAX_BODY: usize = 1024 * 1024;

#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Working,
    Blocked,
    Waiting,
    Done,
}

#[derive(Clone, Serialize, Debug)]
pub struct AgentStatus {
    pub state: State,
    pub agent: String,
    pub session_id: Option<String>,
    pub session_file: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub last_message: Option<String>,
    pub updated_at: u64,
    pub state_started_at: u64,
    pub worktree_id: Option<String>,
    pub tab_id: Option<String>,
}

#[derive(Default)]
pub struct AgentHub {
    panes: Mutex<HashMap<String, AgentStatus>>,
    /// `agent|cwd` → pane key, for hooks that arrive without one (Codex runs
    /// hooks in a shared daemon that lacks the terminal's env).
    by_cwd: Mutex<HashMap<String, String>>,
    pub port: Mutex<u16>,
    pub token: Mutex<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Map one lifecycle event to the next state (`None` = keep).
pub fn normalize(cur: Option<State>, p: &Value) -> Option<State> {
    let ev = p["event"].as_str().unwrap_or_default();
    match ev {
        "tool_approval_requested" | "permission_request" => Some(State::Blocked),
        "tool_call" if p["tool_name"] == "ask" => Some(State::Waiting),
        "agent_end" | "stop" if p["will_continue"] != true => Some(State::Done),
        "session_shutdown" | "session_end" => Some(State::Idle),
        // An approval prompt holds Blocked until it resolves or a new turn starts.
        "tool_execution_start" | "tool_execution_end" | "message_end"
            if cur == Some(State::Blocked) =>
        {
            None
        }
        "before_agent_start"
        | "agent_start"
        | "tool_execution_start"
        | "tool_execution_end"
        | "message_end"
        | "tool_approval_resolved"
        | "user_prompt_submit"
        | "pre_tool_use"
        | "post_tool_use" => Some(State::Working),
        _ => None,
    }
}

/// `/private/tmp/x/` and `/tmp/x` are the same directory on macOS.
fn norm_cwd(p: &str) -> String {
    let p = p.trim_end_matches('/');
    p.strip_prefix("/private").unwrap_or(p).to_string()
}

impl AgentHub {
    pub fn bind(&self, pane: &str, agent: &str, cwd: &str) {
        self.by_cwd
            .lock()
            .insert(format!("{agent}|{}", norm_cwd(cwd)), pane.to_string());
    }

    pub fn apply(&self, agent: &str, body: &Value) -> Option<(String, AgentStatus)> {
        let pane = match body["pane_key"].as_str() {
            Some(p) => p.to_string(),
            None => {
                let cwd = norm_cwd(body["payload"]["cwd"].as_str()?);
                self.by_cwd.lock().get(&format!("{agent}|{cwd}"))?.clone()
            }
        };
        let p = &body["payload"];
        let now = now_ms();
        let mut panes = self.panes.lock();
        let st = panes.entry(pane.clone()).or_insert_with(|| AgentStatus {
            state: State::Idle,
            agent: agent.to_string(),
            session_id: None,
            session_file: None,
            model: None,
            prompt: None,
            last_message: None,
            updated_at: now,
            state_started_at: now,
            worktree_id: None,
            tab_id: None,
        });
        let str_of = |k: &str| p[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
        st.worktree_id = body["worktree_id"]
            .as_str()
            .map(str::to_string)
            .or(st.worktree_id.take());
        st.tab_id = body["tab_id"]
            .as_str()
            .map(str::to_string)
            .or(st.tab_id.take());
        st.session_id = str_of("session_id").or(st.session_id.take());
        st.session_file = str_of("session_file").or(st.session_file.take());
        st.model = str_of("model").or(st.model.take());
        if let Some(pr) = str_of("prompt") {
            st.prompt = Some(pr);
        }
        if let Some(t) = str_of("text") {
            st.last_message = Some(t.chars().take(2000).collect());
        }
        if let Some(next) = normalize(Some(st.state), p)
            && next != st.state
        {
            st.state = next;
            st.state_started_at = now;
        }
        st.updated_at = now;
        Some((pane, st.clone()))
    }

    /// Forget a pane (its pty exited).
    pub fn idle(&self, pane: &str) -> Option<AgentStatus> {
        let mut panes = self.panes.lock();
        let st = panes.get_mut(pane)?;
        st.state = State::Idle;
        st.state_started_at = now_ms();
        Some(st.clone())
    }

    pub fn all(&self) -> HashMap<String, AgentStatus> {
        self.panes.lock().clone()
    }
}

/// Remember which tab runs `agent` in `cwd` (fallback for hooks without a pane key).
#[tauri::command]
pub fn agent_bind(hub: tauri::State<'_, Arc<AgentHub>>, pane: String, agent: String, cwd: String) {
    hub.bind(&pane, &agent, &cwd);
}

/// The pane's process exited: back to Idle (frontend knows pty → pane).
#[tauri::command]
pub fn agent_pane_exited(app: AppHandle, hub: tauri::State<'_, Arc<AgentHub>>, pane: String) {
    if let Some(st) = hub.idle(&pane) {
        let _ = app.emit("agent://status", json!({"pane_key": pane, "status": st}));
    }
}

#[tauri::command]
pub fn agent_status_all(hub: tauri::State<'_, Arc<AgentHub>>) -> HashMap<String, AgentStatus> {
    hub.all()
}

fn write_endpoint(port: u16, token: &str) -> std::io::Result<()> {
    let p = super::launch::hook_endpoint_file();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("tmp~");
    let mut f = atlas_core::sys::owner_only(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .open(&tmp)?;
    writeln!(f, "ATLAS_HOOK_PORT={port}\nATLAS_HOOK_TOKEN={token}")?;
    std::fs::rename(tmp, p)
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("atlas-desktop: hook server: {e}");
                return;
            }
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let token = atlas_host::paths::random_hex(24).unwrap_or_default();
        let hub = app.state::<Arc<AgentHub>>().inner().clone();
        *hub.port.lock() = port;
        *hub.token.lock() = token.clone();
        if let Err(e) = write_endpoint(port, &token) {
            eprintln!("atlas-desktop: hook endpoint file: {e}");
        }
        while let Ok((stream, _)) = listener.accept().await {
            let (app, token, hub) = (app.clone(), token.clone(), hub.clone());
            tauri::async_runtime::spawn(async move {
                let _ = serve(stream, &app, &hub, &token).await;
            });
        }
    });
}

async fn serve(
    stream: TcpStream,
    app: &AppHandle,
    hub: &AgentHub,
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
    let status = if method.as_str() != "POST" {
        "405 Method Not Allowed"
    } else if tok != token {
        "401 Unauthorized"
    } else if len > MAX_BODY {
        "413 Payload Too Large"
    } else {
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).await?;
        match serde_json::from_slice::<Value>(&body) {
            Err(_) => "400 Bad Request",
            Ok(v) => {
                if let Some(agent) = path.strip_prefix("/hook/") {
                    if let Some((pane, st)) = hub.apply(agent, &v) {
                        let _ = app.emit("agent://status", json!({"pane_key": pane, "status": st}));
                    }
                    "204 No Content"
                } else {
                    "404 Not Found"
                }
            }
        }
    };
    let resp = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    w.write_all(resp.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(e: &str) -> Value {
        json!({"event": e})
    }

    #[test]
    fn transitions() {
        use State::*;
        assert_eq!(normalize(Some(Idle), &ev("agent_start")), Some(Working));
        assert_eq!(
            normalize(
                Some(Working),
                &json!({"event":"tool_call","tool_name":"ask"})
            ),
            Some(Waiting)
        );
        assert_eq!(
            normalize(
                Some(Working),
                &json!({"event":"tool_call","tool_name":"bash"})
            ),
            None
        );
        assert_eq!(
            normalize(Some(Working), &ev("tool_approval_requested")),
            Some(Blocked)
        );
        assert_eq!(normalize(Some(Blocked), &ev("tool_execution_end")), None);
        assert_eq!(
            normalize(Some(Blocked), &ev("tool_approval_resolved")),
            Some(Working)
        );
        assert_eq!(
            normalize(
                Some(Working),
                &json!({"event":"agent_end","will_continue":true})
            ),
            None
        );
        assert_eq!(
            normalize(
                Some(Working),
                &json!({"event":"agent_end","will_continue":false})
            ),
            Some(Done)
        );
        assert_eq!(normalize(Some(Done), &ev("session_shutdown")), Some(Idle));
    }

    #[test]
    fn apply_keeps_session_fields_across_events() {
        let hub = AgentHub::default();
        let body = |p: Value| json!({"pane_key": "t1", "worktree_id": "w", "payload": p});
        hub.apply(
            "omp",
            &body(json!({"event":"session_start","session_id":"s1","session_file":"/f"})),
        );
        let (_, st) = hub
            .apply("omp", &body(json!({"event":"agent_start","prompt":"hi"})))
            .unwrap();
        assert_eq!(st.state, State::Working);
        assert_eq!(st.session_file.as_deref(), Some("/f"));
        assert_eq!(st.prompt.as_deref(), Some("hi"));
        assert!(hub.apply("omp", &json!({"payload":{}})).is_none());
    }

    #[test]
    fn events_without_pane_key_resolve_by_agent_and_cwd() {
        let hub = AgentHub::default();
        hub.bind("tab-1", "codex", "/tmp/wt");
        let ev = |agent: &str, cwd: &str| {
            json!({"payload": {"event": "user_prompt_submit", "cwd": cwd}})
                .pipe(|b| hub.apply(agent, &b))
        };
        assert_eq!(
            ev("codex", "/private/tmp/wt/").map(|x| x.0).as_deref(),
            Some("tab-1")
        );
        assert!(ev("codex", "/tmp/other").is_none());
        assert!(ev("claude", "/tmp/wt").is_none());
    }

    trait Pipe: Sized {
        fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
            f(self)
        }
    }
    impl Pipe for Value {}
}
