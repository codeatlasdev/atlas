//! Agent session history (omp `~/.omp/agent/sessions/<cwd-slug>/*.jsonl`) and
//! the listening ports of a worktree's terminals.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

/// How many lines to scan for the header, title and first prompt.
const HEAD_LINES: usize = 40;
const MAX_SESSIONS: usize = 400;

#[derive(Serialize)]
pub struct Session {
    agent: &'static str,
    id: String,
    /// What the agent's resume flag takes: omp a file path, claude/codex a session id.
    resume: String,
    file: String,
    cwd: String,
    title: Option<String>,
    first_prompt: Option<String>,
    mtime: u64,
    size: u64,
}

fn first_text(msg: &Value) -> Option<String> {
    match &msg["content"] {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => parts.iter().find_map(|p| {
            (p["type"] == "text")
                .then(|| p["text"].as_str().map(str::to_string))
                .flatten()
        }),
        _ => None,
    }
}

/// One scanner for the three formats; fields differ per agent:
/// omp `{type:session,id,cwd}` / `{type:title}` / `{type:message,message:{role:user}}`;
/// claude `{sessionId,cwd}` + `{type:user,message:{content}}`;
/// codex `{type:session_meta,payload:{id,cwd}}` + `{type:response_item,payload:{role:user,content:[input_text]}}`.
fn read_session(agent: &'static str, path: &Path) -> Option<Session> {
    let meta = std::fs::metadata(path).ok()?;
    let f = std::fs::File::open(path).ok()?;
    let (mut id, mut cwd, mut title, mut prompt) = (None, None, None, None);
    for line in BufReader::new(f)
        .lines()
        .take(HEAD_LINES)
        .map_while(Result::ok)
    {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if id.is_none() {
            id = v["sessionId"]
                .as_str()
                .or(v["payload"]["id"].as_str())
                .map(str::to_string);
        }
        if cwd.is_none() {
            cwd = v["cwd"]
                .as_str()
                .or(v["payload"]["cwd"].as_str())
                .map(str::to_string);
        }
        if prompt.is_none() && v["type"] == "user" && agent == "claude" {
            prompt = first_text(&v["message"]).map(|t| t.chars().take(300).collect());
        }
        if prompt.is_none() && v["type"] == "response_item" && v["payload"]["role"] == "user" {
            let t = v["payload"]["content"]
                .as_array()
                .and_then(|a| a.iter().find_map(|c| c["text"].as_str()))
                .filter(|t| !t.starts_with('<'));
            prompt = t.map(|t| t.chars().take(300).collect());
        }
        match v["type"].as_str() {
            Some("session") => {
                id = v["id"].as_str().map(str::to_string);
                cwd = v["cwd"].as_str().map(str::to_string);
            }
            Some("title") => {
                title = v["title"]
                    .as_str()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
            }
            Some("message") if prompt.is_none() && v["message"]["role"] == "user" => {
                prompt = first_text(&v["message"]).map(|t| t.chars().take(300).collect());
            }
            _ => {}
        }
        if id.is_some() && title.is_some() && prompt.is_some() {
            break;
        }
    }
    let id = id?;
    Some(Session {
        agent,
        resume: if agent == "omp" {
            path.to_string_lossy().into_owned()
        } else {
            id.clone()
        },
        id,
        file: path.to_string_lossy().into_owned(),
        cwd: cwd.unwrap_or_default(),
        title,
        first_prompt: prompt,
        mtime: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs()),
        size: meta.len(),
    })
}

fn jsonl_under(dir: &Path, out: &mut Vec<(u64, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            jsonl_under(&p, out);
        } else if p.extension().is_some_and(|x| x == "jsonl")
            // omp's internal sub-sessions (`__advisor.jsonl`…) aren't user sessions.
            && !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("__"))
        {
            let m = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            out.push((m, p));
        }
    }
}

/// Newest first. `cwds`: keep sessions whose cwd is one of these (or under it); empty = all.
#[tauri::command]
pub async fn agent_sessions(cwds: Vec<String>, query: Option<String>) -> Vec<Session> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir().unwrap_or_default();
        let mut files = Vec::new();
        for (agent, dir) in [
            ("omp", ".omp/agent/sessions"),
            ("claude", ".claude/projects"),
            ("codex", ".codex/sessions"),
        ] {
            let mut f = Vec::new();
            jsonl_under(&home.join(dir), &mut f);
            files.extend(f.into_iter().map(|(m, p)| (m, agent, p)));
        }
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        let q = query.unwrap_or_default().to_lowercase();
        // macOS reports /tmp as /private/tmp in some agents: compare both spellings.
        let norm = |p: &str| p.strip_prefix("/private").unwrap_or(p).to_string();
        let cwds: Vec<String> = cwds.iter().map(|c| norm(c)).collect();
        files
            .into_iter()
            .filter_map(|(_, agent, p)| read_session(agent, &p))
            .filter(|s| {
                let cwd = norm(&s.cwd);
                cwds.is_empty()
                    || cwds
                        .iter()
                        .any(|c| cwd == *c || cwd.starts_with(&format!("{c}/")))
            })
            .filter(|s| {
                q.is_empty()
                    || [&s.title, &s.first_prompt, &Some(s.cwd.clone())]
                        .iter()
                        .any(|f| f.as_deref().is_some_and(|t| t.to_lowercase().contains(&q)))
            })
            .take(MAX_SESSIONS)
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[derive(Serialize)]
pub struct Port {
    port: u16,
    pid: u32,
    command: String,
}

/// Every pid in the process trees rooted at `roots` (via `ps -axo pid,ppid`).
fn descendants(roots: &[u32]) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
    else {
        return roots.to_vec();
    };
    let pairs: Vec<(u32, u32)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().filter_map(|n| n.parse().ok());
            Some((it.next()?, it.next()?))
        })
        .collect();
    let mut all = roots.to_vec();
    let mut i = 0;
    while i < all.len() {
        let parent = all[i];
        all.extend(
            pairs
                .iter()
                .filter(|(_, pp)| *pp == parent)
                .map(|(p, _)| *p),
        );
        i += 1;
    }
    all
}

/// TCP ports listened on by the given terminals' process trees.
#[tauri::command]
pub async fn listening_ports(pids: Vec<u32>) -> Vec<Port> {
    tauri::async_runtime::spawn_blocking(move || {
        let tree = descendants(&pids);
        let Ok(out) = std::process::Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
            .output()
        else {
            return Vec::new();
        };
        let (mut pid, mut cmd) = (0u32, String::new());
        let mut ports: Vec<Port> = Vec::new();
        for l in String::from_utf8_lossy(&out.stdout).lines() {
            match l.split_at(1) {
                ("p", v) => pid = v.parse().unwrap_or(0),
                ("c", v) => cmd = v.to_string(),
                ("n", v) if tree.contains(&pid) => {
                    if let Some(port) = v.rsplit(':').next().and_then(|p| p.parse().ok())
                        && !ports.iter().any(|x| x.port == port)
                    {
                        ports.push(Port {
                            port,
                            pid,
                            command: cmd.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        ports.sort_by_key(|p| p.port);
        ports
    })
    .await
    .unwrap_or_default()
}
