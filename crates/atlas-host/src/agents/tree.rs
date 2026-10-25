//! The agent tree of a pane: the root agent and every subagent it started, however deep,
//! each with what it is doing now, for the phone's navigation.
//!
//! omp keeps a subagent's transcript under its parent's session dir (`<s>.jsonl` →
//! `<s>/<A>.jsonl`, and `<s>/<A>/<A>.<B>.jsonl` for a child of `A`), its final report in
//! `<A>.md` and a `.<A>.jsonl.lock.os` file while its writer lives. The parent's transcript
//! says how it ended: a `custom_message` `async-result` carrying
//! `<task-result id=".." status="..">` (and `details.jobs[]`). Claude Code keeps its
//! subagents flat in `<sid>/subagents/agent-<id>.jsonl` with a `.meta.json` beside each.
//! Codex has none.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::State;

/// A transcript that changed this recently, without a lock file to say otherwise, is alive.
const RUNNING_WINDOW_MS: u64 = 60_000;
const TASK_CHARS: usize = 300;
const NOW_CHARS: usize = 160;
const REPORT_CHARS: usize = 600;

/// `[A-Za-z0-9_-]`, non-empty: one segment of a dotted subagent id.
fn segment_ok(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// The directory holding a session's subagents: `<s>.jsonl` → `<s>/`.
fn session_dir(session_file: &str) -> &Path {
    Path::new(session_file.strip_suffix(".jsonl").unwrap_or(session_file))
}

/// The transcript of subagent `id` of `session_file`. omp ids are dotted (`A`, `A.B`):
/// every segment is validated, so an id never names a path; `A.B` lives in `<dir>/A/`.
/// Claude's are `agent-<id>` stems under `<dir>/subagents/`.
pub fn subagent_file(session_file: &str, id: &str) -> Result<String, String> {
    let dir = session_dir(session_file);
    let segs: Vec<&str> = id.split('.').collect();
    if !segs.iter().all(|s| segment_ok(s)) {
        return Err("bad subagent name".into());
    }
    let mut holder = dir.to_path_buf();
    for i in 0..segs.len() - 1 {
        holder.push(segs[..=i].join("."));
    }
    let mut candidates = vec![holder.join(format!("{id}.jsonl"))];
    if segs.len() == 1 && id.starts_with("agent-") {
        candidates.push(dir.join("subagents").join(format!("{id}.jsonl")));
    }
    candidates
        .into_iter()
        .find(|f| f.is_file())
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| "no such subagent".into())
}

/// `s` cut to `max` chars, with `…` when cut.
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn mtime_ms(path: &Path) -> Option<(u64, SystemTime)> {
    let m = path.metadata().ok()?.modified().ok()?;
    Some((m.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64, m))
}

/// How a subagent ended, as its parent's transcript recorded it: id → status.
/// Reads `async-result` messages: `<task-result id=".." status="..">` in the text and
/// `details.jobs[]` (`jobId`, `status`); the text wins, it names the task, not the job.
fn finished(parent: &[Value]) -> HashMap<String, (String, Option<String>)> {
    let mut out = HashMap::new();
    for v in parent {
        if v["type"] != "custom_message" || v["customType"] != "async-result" {
            continue;
        }
        let content = v["content"].as_str().unwrap_or_default();
        for (id, status, duration) in task_results(content) {
            out.insert(id, (status, duration));
        }
        for j in v["details"]["jobs"].as_array().into_iter().flatten() {
            if let (Some(id), Some(status)) = (j["jobId"].as_str(), j["status"].as_str()) {
                out.entry(id.to_string()).or_insert_with(|| {
                    (
                        status.to_string(),
                        j["durationMs"].as_u64().map(|d| d.to_string()),
                    )
                });
            }
        }
    }
    out
}

/// `(id, status, duration)` of each `<task-result id=".." … status=".." duration="..">`.
pub fn task_results(content: &str) -> Vec<(String, String, Option<String>)> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let key = format!("{name}=\"");
        let from = tag.find(&format!(" {key}"))? + 1 + key.len();
        let to = tag[from..].find('"')?;
        Some(tag[from..from + to].to_string())
    };
    content
        .split("<task-result")
        .skip(1)
        .filter_map(|rest| {
            let tag = &rest[..rest.find('>')?];
            Some((
                attr(tag, "id")?,
                attr(tag, "status")?,
                attr(tag, "duration"),
            ))
        })
        .collect()
}

/// The assignment of each subagent a transcript started: id → (task, agent), from its `task`
/// tool calls.
fn assignments(parent: &[Value]) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    for v in parent {
        let Some(blocks) = v["message"]["content"].as_array() else {
            continue;
        };
        for b in blocks {
            if b["type"] != "toolCall" || !matches!(b["name"].as_str(), Some("task" | "Task")) {
                continue;
            }
            let args = &b["arguments"];
            let single = [args.clone()];
            let tasks = args["tasks"].as_array().map_or(&single[..], Vec::as_slice);
            for t in tasks {
                if let Some(name) = t["name"].as_str() {
                    out.push((
                        name.to_string(),
                        t["task"].as_str().unwrap_or_default().trim().to_string(),
                        t["agent"].as_str().map(str::to_string),
                    ));
                }
            }
        }
    }
    out
}

/// What a transcript says about its own agent: model, tokens, cost, tool count, the tool
/// it runs now, and how its last turn stopped.
#[derive(Default)]
struct Facts {
    model: Option<String>,
    tokens: u64,
    cost: f64,
    tools: u64,
    now: Option<String>,
    stop: Option<String>,
    /// First user message: the assignment, for agents whose parent does not carry it.
    first_user: Option<String>,
    agent: Option<String>,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A tool call as one line: its intent, else `name: detail`.
fn call_line(b: &Value, intents: &HashMap<String, String>) -> String {
    let name = b["name"].as_str().unwrap_or("tool");
    let args = if b["arguments"].is_null() {
        &b["input"]
    } else {
        &b["arguments"]
    };
    let intent = b["intent"]
        .as_str()
        .or_else(|| args["i"].as_str())
        .or_else(|| {
            b["id"]
                .as_str()
                .and_then(|i| intents.get(i))
                .map(String::as_str)
        });
    let s = match intent {
        Some(i) => i.to_string(),
        None => {
            let detail = [
                "command",
                "path",
                "pattern",
                "query",
                "url",
                "file_path",
                "description",
            ]
            .iter()
            .find_map(|k| args[*k].as_str())
            .unwrap_or_default();
            if detail.is_empty() {
                name.to_string()
            } else {
                format!("{name}: {}", detail.trim())
            }
        }
    };
    clip(s.trim(), NOW_CHARS)
}

fn facts(entries: &[Value]) -> Facts {
    let mut f = Facts::default();
    // Calls without a result yet: id → one-line description, in call order.
    let mut open: Vec<(String, String)> = Vec::new();
    // `tool_execution_start` carries the intent omp left out of the call.
    let mut intents: HashMap<String, String> = HashMap::new();
    for v in entries {
        match v["type"].as_str() {
            Some("model_change") => {
                if let Some(m) = v["model"].as_str() {
                    f.model = Some(m.to_string());
                }
            }
            Some("session_init") => {
                if let Some(a) = v["agent"].as_str() {
                    f.agent = Some(a.to_string());
                }
            }
            Some("custom") if v["customType"] == "tool_execution_start" => {
                if let (Some(id), Some(i)) = (
                    v["data"]["toolCallId"].as_str(),
                    v["data"]["intent"].as_str(),
                ) {
                    intents.insert(id.to_string(), i.to_string());
                }
            }
            Some("message" | "user" | "assistant") => {
                let m = &v["message"];
                match m["role"].as_str() {
                    Some("assistant") => {
                        if let Some(model) = m["model"].as_str() {
                            f.model = Some(model.to_string());
                        }
                        f.tokens += m["usage"]["totalTokens"].as_u64().unwrap_or_else(|| {
                            ["input_tokens", "output_tokens"]
                                .iter()
                                .filter_map(|k| m["usage"][*k].as_u64())
                                .sum()
                        });
                        f.cost += m["usage"]["cost"]["total"].as_f64().unwrap_or(0.0);
                        f.stop = m["stopReason"].as_str().map(str::to_string);
                        for b in m["content"].as_array().into_iter().flatten() {
                            if matches!(b["type"].as_str(), Some("toolCall" | "tool_use")) {
                                f.tools += 1;
                                open.push((
                                    b["id"].as_str().unwrap_or_default().to_string(),
                                    call_line(b, &intents),
                                ));
                            }
                        }
                    }
                    Some("toolResult") => {
                        let id = m["toolCallId"].as_str().unwrap_or_default();
                        open.retain(|(i, _)| i != id);
                    }
                    Some("user") => {
                        for b in m["content"].as_array().into_iter().flatten() {
                            if b["type"] == "tool_result"
                                && let Some(id) = b["tool_use_id"].as_str()
                            {
                                open.retain(|(i, _)| i != id);
                            }
                        }
                        if f.first_user.is_none() {
                            let t = text_of(&m["content"]);
                            if !t.trim().is_empty() {
                                f.first_user = Some(t.trim().to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // The intent can arrive after the call was seen: refresh from the full map.
    f.now = open.last().map(|(id, line)| {
        intents
            .get(id)
            .map_or_else(|| line.clone(), |i| clip(i.trim(), NOW_CHARS))
    });
    f
}

/// A subagent's status from the files, before its parent's verdict: completed once its
/// report exists (omp writes it last; the `.lock.os` file lingers after the writer ends,
/// seen on disk 2026-10-25, so it never overrides a report); running while the lock is
/// there or it wrote in the last minute; else how its last turn stopped.
fn status_from_files(
    transcript: &Path,
    holder: &Path,
    id: &str,
    is_claude: bool,
    stop: Option<&str>,
) -> &'static str {
    if !is_claude && holder.join(format!("{id}.md")).is_file() {
        return "completed";
    }
    let fresh = mtime_ms(transcript).is_some_and(|(_, t)| {
        SystemTime::now()
            .duration_since(t)
            .is_ok_and(|d| (d.as_millis() as u64) < RUNNING_WINDOW_MS)
    });
    if fresh || holder.join(format!(".{id}.jsonl.lock.os")).exists() {
        return "running";
    }
    match stop {
        Some("error") => "failed",
        Some("aborted") => "aborted",
        // A Claude subagent has no report file; its last turn ending is its end.
        Some("end_turn" | "stop") if is_claude => "completed",
        _ => "pending",
    }
}

fn norm_status(s: &str) -> &'static str {
    match s {
        "completed" | "done" | "success" => "completed",
        "failed" | "error" => "failed",
        "aborted" | "cancelled" | "canceled" => "aborted",
        _ => "running",
    }
}

fn duration_ms(s: &str) -> Option<u64> {
    if let Ok(ms) = s.parse::<u64>() {
        return Some(ms);
    }
    // omp renders `27m20s`, `1h2m`, `45s`.
    let mut total = 0f64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        total += match c {
            'h' => n * 3_600_000.0,
            'm' => n * 60_000.0,
            's' => n * 1000.0,
            _ => return None,
        };
    }
    num.is_empty().then_some(total as u64)
}

/// A node as the phone reads it.
#[allow(clippy::too_many_arguments)]
fn node(
    id: &str,
    parent: Option<&str>,
    name: Option<String>,
    agent: Option<String>,
    status: &str,
    task: Option<String>,
    f: &Facts,
    duration: Option<u64>,
    report: Option<String>,
    updated_at: Option<u64>,
) -> Value {
    json!({
        "id": id, "parent": parent, "name": name, "agent": agent, "model": f.model,
        "status": status, "task": task.map(|t| clip(&t, TASK_CHARS)),
        "duration_ms": duration, "tokens": f.tokens, "cost": f.cost,
        "tool_count": f.tools, "now": f.now, "report": report, "updated_at": updated_at,
    })
}

/// Root status from the pane's state: the agent works, or it does not.
fn root_status(state: State) -> &'static str {
    match state {
        State::Working | State::Blocked | State::Waiting => "running",
        State::Done => "completed",
        State::Idle => "idle",
    }
}

/// `{nodes: [Node]}` for the pane: the root (`id: ""`) first, then its subagents depth
/// first. `title` is the session title, `agent` the registry id (`omp`, `claude`, `codex`).
pub fn build(session_file: &str, agent: &str, state: State, title: Option<String>) -> Value {
    let root_entries = lines(Path::new(session_file));
    let mut rf = facts(&root_entries);
    if root_status(state) != "running" {
        rf.now = None;
    }
    let root_updated = mtime_ms(Path::new(session_file)).map(|(ms, _)| ms);
    let mut nodes = vec![node(
        "",
        None,
        title,
        Some(agent.to_string()),
        root_status(state),
        None,
        &rf,
        None,
        None,
        root_updated,
    )];
    match agent {
        "omp" => omp_children(session_dir(session_file), "", &root_entries, &mut nodes),
        "claude" => claude_children(session_dir(session_file), &mut nodes),
        _ => {}
    }
    json!({"nodes": nodes})
}

/// The children of `parent_id` (`""` = root), whose transcripts sit in `holder`; recurses
/// into each child's own directory.
fn omp_children(holder: &Path, parent_id: &str, parent: &[Value], nodes: &mut Vec<Value>) {
    let done = finished(parent);
    let spawned = assignments(parent);
    let tasks: HashMap<&str, (&str, Option<&str>)> = spawned
        .iter()
        .map(|(n, t, a)| (n.as_str(), (t.as_str(), a.as_deref())))
        .collect();
    // Transcripts on disk: every `<id>.jsonl` of this directory, side channels excluded.
    let mut ids: Vec<(u64, String)> = std::fs::read_dir(holder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name().into_string().ok()?;
            let stem = file.strip_suffix(".jsonl")?;
            if stem.starts_with('_') || stem.starts_with('.') {
                return None;
            }
            // A child of `A` is `A.B`, one segment more than its parent.
            let want = if parent_id.is_empty() {
                stem.split('.').count() == 1
            } else {
                stem.strip_prefix(parent_id)
                    .and_then(|r| r.strip_prefix('.'))
                    .is_some_and(|r| !r.contains('.'))
            };
            if !want || !stem.split('.').all(segment_ok) {
                return None;
            }
            let (ms, _) = mtime_ms(&e.path())?;
            Some((ms, stem.to_string()))
        })
        .collect();
    // Spawn order is unknown on disk; newest transcript first reads best on a phone.
    ids.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    // Subagents announced by a `task` call whose transcript is not on disk yet.
    for (name, _, _) in &spawned {
        let full = if parent_id.is_empty() {
            name.clone()
        } else {
            format!("{parent_id}.{name}")
        };
        if !ids.iter().any(|(_, s)| *s == full) && segment_ok(name) {
            ids.push((0, full));
        }
    }
    for (_, id) in ids {
        let short = id.rsplit('.').next().unwrap_or(&id);
        let file = holder.join(format!("{id}.jsonl"));
        let entries = lines(&file);
        let f = facts(&entries);
        let (task, agent) = tasks.get(short).copied().unzip();
        let task = task
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .or_else(|| f.first_user.clone());
        let report = std::fs::read_to_string(holder.join(format!("{id}.md")))
            .ok()
            .map(|r| clip(r.trim(), REPORT_CHARS));
        let (status, duration) = match done.get(short).or_else(|| done.get(&id)) {
            Some((s, d)) => (norm_status(s), d.as_deref().and_then(duration_ms)),
            None if !file.is_file() => ("pending", None),
            None => (
                status_from_files(&file, holder, &id, false, f.stop.as_deref()),
                None,
            ),
        };
        let updated = mtime_ms(&file).map(|(ms, _)| ms);
        let mut n = node(
            &id,
            Some(parent_id),
            Some(short.to_string()),
            agent
                .flatten()
                .map(str::to_string)
                .or_else(|| f.agent.clone()),
            status,
            task,
            &f,
            duration,
            report,
            updated,
        );
        // Without the parent's figure, how long the transcript has been written to.
        if n["duration_ms"].is_null()
            && let Some(start) = entries.iter().find_map(|v| v["timestamp"].as_str())
        {
            n["duration_ms"] = elapsed_ms(start, updated).map_or(Value::Null, Value::from);
        }
        if status != "running" {
            n["now"] = Value::Null;
        }
        nodes.push(n);
        // Its own children, in `<holder>/<id>/`.
        omp_children(&holder.join(&id), &id, &entries, nodes);
    }
}

/// Claude Code's subagents: flat, `agent-<id>.jsonl` with a `.meta.json` (`description`).
fn claude_children(dir: &Path, nodes: &mut Vec<Value>) {
    let holder = dir.join("subagents");
    let mut found: Vec<(u64, String)> = std::fs::read_dir(&holder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name().into_string().ok()?;
            let stem = file.strip_suffix(".jsonl")?.to_string();
            if !stem.starts_with("agent-") || !segment_ok(&stem) {
                return None;
            }
            Some((mtime_ms(&e.path())?.0, stem))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (updated, id) in found {
        let file = holder.join(format!("{id}.jsonl"));
        let entries = lines(&file);
        let f = facts(&entries);
        let meta: Value = std::fs::read_to_string(holder.join(format!("{id}.meta.json")))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let task = meta["description"]
            .as_str()
            .map(str::to_string)
            .or_else(|| f.first_user.clone());
        let status = status_from_files(&file, &holder, &id, true, f.stop.as_deref());
        let mut n = node(
            &id,
            Some(""),
            meta["description"].as_str().map(str::to_string),
            meta["agentType"].as_str().map(str::to_string),
            status,
            task,
            &f,
            None,
            None,
            Some(updated),
        );
        if status != "running" {
            n["now"] = Value::Null;
        }
        nodes.push(n);
    }
}

/// Milliseconds from an RFC 3339 `start` to `end_ms` (epoch ms).
fn elapsed_ms(start: &str, end_ms: Option<u64>) -> Option<u64> {
    let start = parse_rfc3339_ms(start)?;
    end_ms?.checked_sub(start)
}

/// `2026-10-25T19:27:48.388Z` → epoch ms (UTC only: omp writes nothing else).
fn parse_rfc3339_ms(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.split(':').map(|p| p.parse::<i64>().ok());
    let (h, mi, sec) = (t.next()??, t.next()??, t.next()??);
    let ms: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    // Days since 1970-01-01 (Howard Hinnant's civil-from-days, inverted).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + ms).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, lines: &[Value]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            p,
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
    }

    fn assistant(content: Value, usage: Value, stop: &str) -> Value {
        json!({"type": "message", "message": {"role": "assistant", "content": content,
               "model": "cc/claude-sonnet-5-5", "usage": usage, "stopReason": stop}})
    }

    fn usage(total: u64, cost: f64) -> Value {
        json!({"input": total / 2, "output": total / 2, "totalTokens": total,
               "cost": {"total": cost}})
    }

    fn task_call(names: &[(&str, &str)]) -> Value {
        let tasks: Vec<Value> = names
            .iter()
            .map(|(n, t)| json!({"name": n, "agent": "scout", "task": t}))
            .collect();
        assistant(
            json!([{"type": "toolCall", "id": "t1", "name": "task",
                    "arguments": {"context": "# Goal", "tasks": tasks}}]),
            usage(10, 0.01),
            "toolUse",
        )
    }

    fn async_result(id: &str, status: &str) -> Value {
        json!({"type": "custom_message", "customType": "async-result", "display": true,
               "content": format!("<system-notice>\nBackground job {id} has completed.\n<task-result id=\"{id}\" agent=\"scout\" status=\"{status}\" duration=\"1m5s\">\n\n<output>x</output>\n</task-result>")})
    }

    /// A parent with two subagents, one of which spawned a child of its own.
    fn omp_session() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("2026_abc.jsonl");
        write(
            &root,
            &[
                json!({"type": "session", "id": "abc"}),
                task_call(&[("Scout", "map hostd"), ("Fixer", "edit it")]),
            ],
        );
        let dir = tmp.path().join("2026_abc");
        write(
            &dir.join("Scout.jsonl"),
            &[
                json!({"type": "model_change", "model": "codeatlas/cc/claude-haiku-5-5",
                       "role": "subagent:Scout"}),
                task_call(&[("Deep", "go deeper")]),
            ],
        );
        write(
            &dir.join("Scout").join("Scout.Deep.jsonl"),
            &[assistant(
                json!([{"type": "text", "text": "ok"}]),
                usage(100, 0.5),
                "stop",
            )],
        );
        write(&dir.join("Fixer.jsonl"), &[]);
        (tmp, root.to_string_lossy().into_owned())
    }

    fn by_id<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node {id}: {v}"))
    }

    #[test]
    fn nested_subagents_get_dotted_ids_and_parent_links() {
        let (_t, f) = omp_session();
        let v = build(&f, "omp", State::Working, Some("Title".into()));
        let ids: Vec<&str> = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids[0], "");
        let mut rest = ids[1..].to_vec();
        rest.sort();
        assert_eq!(rest, ["Fixer", "Scout", "Scout.Deep"]);
        let root = by_id(&v, "");
        assert_eq!(
            (
                root["parent"].is_null(),
                root["name"].as_str(),
                root["status"].as_str()
            ),
            (true, Some("Title"), Some("running"))
        );
        assert_eq!(by_id(&v, "Scout")["parent"], "");
        assert_eq!(by_id(&v, "Fixer")["parent"], "");
        assert_eq!(by_id(&v, "Scout.Deep")["parent"], "Scout");
        assert_eq!(by_id(&v, "Scout.Deep")["name"], "Deep");
        // The assignment comes from the spawning `task` call.
        assert_eq!(by_id(&v, "Scout")["task"], "map hostd");
        assert_eq!(by_id(&v, "Scout.Deep")["task"], "go deeper");
        assert_eq!(by_id(&v, "Scout")["agent"], "scout");
        // The last assistant message's model wins over the `model_change` before it.
        assert_eq!(by_id(&v, "Scout")["model"], "cc/claude-sonnet-5-5");
        assert_eq!(by_id(&v, "Fixer")["model"], Value::Null);
    }

    #[test]
    fn root_status_maps_the_pane_state() {
        let (_t, f) = omp_session();
        for (st, want) in [
            (State::Working, "running"),
            (State::Blocked, "running"),
            (State::Waiting, "running"),
            (State::Done, "completed"),
            (State::Idle, "idle"),
        ] {
            assert_eq!(by_id(&build(&f, "omp", st, None), "")["status"], want);
        }
        assert!(by_id(&build(&f, "omp", State::Idle, None), "")["name"].is_null());
    }

    /// The parent's `async-result` is the verdict, whatever the files and lock say.
    #[test]
    fn an_async_result_overrides_the_lock_and_names_failures() {
        let (_t, f) = omp_session();
        let dir = Path::new(&f).with_extension("");
        std::fs::write(dir.join(".Scout.jsonl.lock.os"), "").unwrap();
        let v = build(&f, "omp", State::Working, None);
        assert_eq!(by_id(&v, "Scout")["status"], "running");
        // Append the verdicts to the parent.
        let mut text = std::fs::read_to_string(&f).unwrap();
        for e in [
            async_result("Scout", "failed"),
            async_result("Fixer", "aborted"),
        ] {
            text.push('\n');
            text.push_str(&e.to_string());
        }
        std::fs::write(&f, text).unwrap();
        let v = build(&f, "omp", State::Working, None);
        assert_eq!(by_id(&v, "Scout")["status"], "failed");
        assert_eq!(by_id(&v, "Scout")["duration_ms"], 65_000);
        assert_eq!(by_id(&v, "Fixer")["status"], "aborted");
        assert!(by_id(&v, "Scout")["now"].is_null());
    }

    #[test]
    fn a_job_entry_decides_when_the_text_has_no_task_result() {
        let (_t, f) = omp_session();
        let mut text = std::fs::read_to_string(&f).unwrap();
        text.push('\n');
        text.push_str(
            &json!({"type": "custom_message", "customType": "async-result", "content": "done",
                    "details": {"jobs": [{"jobId": "Fixer", "type": "task", "status": "completed",
                                          "durationMs": 4200}]}})
            .to_string(),
        );
        std::fs::write(&f, text).unwrap();
        let n = build(&f, "omp", State::Idle, None);
        assert_eq!(by_id(&n, "Fixer")["status"], "completed");
        assert_eq!(by_id(&n, "Fixer")["duration_ms"], 4200);
    }

    #[test]
    fn running_by_lock_completed_by_report_pending_without_a_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(
            &root,
            &[task_call(&[
                ("Locked", "a"),
                ("Reported", "b"),
                ("Failing", "c"),
                ("Queued", "d"),
            ])],
        );
        let dir = tmp.path().join("s");
        for n in ["Locked", "Reported", "Failing"] {
            write(&dir.join(format!("{n}.jsonl")), &[]);
        }
        std::fs::write(dir.join(".Locked.jsonl.lock.os"), "").unwrap();
        std::fs::write(dir.join("Reported.md"), "all good").unwrap();
        write(
            &dir.join("Failing.jsonl"),
            &[assistant(json!([]), usage(2, 0.0), "error")],
        );
        // Old enough that mtime alone no longer says "running".
        let old = SystemTime::now() - std::time::Duration::from_secs(3600);
        for n in ["Reported", "Failing"] {
            std::fs::File::options()
                .write(true)
                .open(dir.join(format!("{n}.jsonl")))
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        let v = build(&root.to_string_lossy(), "omp", State::Working, None);
        assert_eq!(by_id(&v, "Locked")["status"], "running");
        assert_eq!(by_id(&v, "Reported")["status"], "completed");
        assert_eq!(by_id(&v, "Reported")["report"], "all good");
        assert_eq!(by_id(&v, "Failing")["status"], "failed");
        assert_eq!(by_id(&v, "Queued")["status"], "pending");
    }

    /// Regression (2026-10-25, on disk): omp leaves `.<Name>.jsonl.lock.os` behind after a
    /// subagent ends, so five finished subagents of one session would read as running
    /// forever. A report is terminal, whatever the lock and a fresh mtime say.
    #[test]
    fn a_report_wins_over_a_lingering_lock_and_a_fresh_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[task_call(&[("Done", "a")])]);
        let dir = tmp.path().join("s");
        write(&dir.join("Done.jsonl"), &[]);
        std::fs::write(dir.join(".Done.jsonl.lock.os"), "").unwrap();
        std::fs::write(dir.join("Done.md"), "ok").unwrap();
        let v = build(&root.to_string_lossy(), "omp", State::Working, None);
        assert_eq!(by_id(&v, "Done")["status"], "completed");
    }

    #[test]
    fn a_stale_transcript_without_report_or_lock_uses_its_stop_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[]);
        let dir = tmp.path().join("s");
        write(
            &dir.join("Cut.jsonl"),
            &[assistant(json!([]), usage(2, 0.0), "aborted")],
        );
        std::fs::File::options()
            .write(true)
            .open(dir.join("Cut.jsonl"))
            .unwrap()
            .set_modified(SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        let v = build(&root.to_string_lossy(), "omp", State::Idle, None);
        assert_eq!(by_id(&v, "Cut")["status"], "aborted");
    }

    #[test]
    fn tokens_cost_and_tools_are_summed_from_the_nodes_own_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[assistant(json!([]), usage(1000, 1.0), "stop")]);
        write(
            &tmp.path().join("s").join("A.jsonl"),
            &[
                assistant(
                    json!([{"type": "toolCall", "id": "1", "name": "read", "arguments": {"path": "a"}}]),
                    usage(100, 0.25),
                    "toolUse",
                ),
                json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "1"}}),
                assistant(
                    json!([{"type": "toolCall", "id": "2", "name": "bash", "arguments": {"command": "ls"}}]),
                    usage(50, 0.5),
                    "toolUse",
                ),
                json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "2"}}),
            ],
        );
        let v = build(&root.to_string_lossy(), "omp", State::Idle, None);
        let a = by_id(&v, "A");
        assert_eq!(
            (a["tokens"].as_u64(), a["tool_count"].as_u64()),
            (Some(150), Some(2))
        );
        assert!((a["cost"].as_f64().unwrap() - 0.75).abs() < 1e-9);
        // The root's own figures do not include its children's.
        assert_eq!(by_id(&v, "")["tokens"], 1000);
    }

    /// `now` is the intent of the call that has no result yet; none once it has one.
    #[test]
    fn now_is_the_open_tool_call() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[task_call(&[("A", "x")])]);
        let a = tmp.path().join("s").join("A.jsonl");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(tmp.path().join("s").join(".A.jsonl.lock.os"), "").unwrap();
        let open = assistant(
            json!([
                {"type": "toolCall", "id": "1", "name": "read", "arguments": {"path": "a.rs"}},
                {"type": "toolCall", "id": "2", "name": "bash", "arguments": {"command": "cargo test"}},
            ]),
            usage(2, 0.0),
            "toolUse",
        );
        let res1 = json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "1"}});
        write(&a, &[open.clone(), res1.clone()]);
        let v = build(&root.to_string_lossy(), "omp", State::Working, None);
        assert_eq!(by_id(&v, "A")["now"], "bash: cargo test");
        // omp's `tool_execution_start` supplies the intent the call lacks.
        let start = json!({"type": "custom", "customType": "tool_execution_start",
                           "data": {"toolCallId": "2", "toolName": "bash", "intent": "Running the host tests"}});
        write(&a, &[open.clone(), res1.clone(), start]);
        let v = build(&root.to_string_lossy(), "omp", State::Working, None);
        assert_eq!(by_id(&v, "A")["now"], "Running the host tests");
        let res2 = json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "2"}});
        write(&a, &[open, res1, res2]);
        let v = build(&root.to_string_lossy(), "omp", State::Working, None);
        assert!(by_id(&v, "A")["now"].is_null());
    }

    #[test]
    fn the_task_is_clipped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[task_call(&[("A", &"x".repeat(1000))])]);
        let v = build(&root.to_string_lossy(), "omp", State::Idle, None);
        assert_eq!(
            by_id(&v, "A")["task"].as_str().unwrap().chars().count(),
            TASK_CHARS + 1
        );
    }

    #[test]
    fn omps_side_files_are_not_nodes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("s.jsonl");
        write(&root, &[]);
        write(&tmp.path().join("s").join("__advisor.jsonl"), &[]);
        let v = build(&root.to_string_lossy(), "omp", State::Idle, None);
        assert_eq!(v["nodes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn claude_subagents_are_listed_flat_under_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("sid.jsonl");
        write(&root, &[]);
        let subs = tmp.path().join("sid").join("subagents");
        write(
            &subs.join("agent-ab12.jsonl"),
            &[
                json!({"type": "fork-context-ref", "agentId": "ab12"}),
                json!({"type": "assistant", "message": {"role": "assistant", "model": "claude-sonnet-4.6",
                       "content": [{"type": "tool_use", "id": "u1", "name": "Grep", "input": {"pattern": "fn main"}}],
                       "usage": {"input_tokens": 10, "output_tokens": 5}}}),
            ],
        );
        std::fs::write(
            subs.join("agent-ab12.meta.json"),
            json!({"agentType": "fork", "description": "Refatorar renderers"}).to_string(),
        )
        .unwrap();
        let v = build(&root.to_string_lossy(), "claude", State::Working, None);
        let n = by_id(&v, "agent-ab12");
        assert_eq!(n["parent"], "");
        assert_eq!(n["name"], "Refatorar renderers");
        assert_eq!(n["agent"], "fork");
        assert_eq!(n["model"], "claude-sonnet-4.6");
        assert_eq!(n["tokens"], 15);
        assert_eq!(n["status"], "running");
        assert_eq!(n["now"], "Grep: fn main");
        // Codex has no subagents: the root alone.
        assert_eq!(
            build(&root.to_string_lossy(), "codex", State::Idle, None)["nodes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn subagent_paths_resolve_for_nested_ids_and_refuse_escapes() {
        let (_t, f) = omp_session();
        assert!(
            subagent_file(&f, "Scout")
                .unwrap()
                .ends_with("2026_abc/Scout.jsonl")
        );
        assert!(
            subagent_file(&f, "Scout.Deep")
                .unwrap()
                .ends_with("2026_abc/Scout/Scout.Deep.jsonl")
        );
        for bad in [
            "..",
            "../2026_abc",
            "Scout/../Scout",
            "a/b",
            "",
            ".",
            "Scout.",
            ".Scout",
            "Scout..Deep",
            "Scout.Deep.",
            "x\\y",
            "Missing",
            "Scout.Missing",
            "Scout.Deep.Deeper",
            "Fixer.Deep",
        ] {
            assert!(subagent_file(&f, bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn claude_subagent_paths_resolve_by_stem_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("sid.jsonl");
        write(&root, &[]);
        write(
            &tmp.path()
                .join("sid")
                .join("subagents")
                .join("agent-ab12.jsonl"),
            &[],
        );
        let f = root.to_string_lossy();
        assert!(
            subagent_file(&f, "agent-ab12")
                .unwrap()
                .ends_with("subagents/agent-ab12.jsonl")
        );
        for bad in [
            "agent-zz",
            "subagents",
            "agent-ab12/../x",
            "agent-ab12.jsonl",
        ] {
            assert!(subagent_file(&f, bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn durations_and_timestamps_parse() {
        assert_eq!(duration_ms("27m20s"), Some(1_640_000));
        assert_eq!(duration_ms("1h2m"), Some(3_720_000));
        assert_eq!(duration_ms("45s"), Some(45_000));
        assert_eq!(duration_ms("4200"), Some(4200));
        assert_eq!(duration_ms("soon"), None);
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:01.500Z"), Some(1500));
        assert_eq!(
            parse_rfc3339_ms("2026-10-25T19:27:48.388Z"),
            Some(1_792_956_468_388)
        );
        assert_eq!(parse_rfc3339_ms("not a date"), None);
    }
}
