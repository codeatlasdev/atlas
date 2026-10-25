//! An agent's conversation, read from its session `.jsonl` (the path arrives in
//! `AgentStatus.session_file`), as a timeline the phone can render: user and assistant text,
//! the model's thinking, each tool call joined with its result, and the subagents a `task`
//! call started. Context injections and system entries are not part of it.
//!
//! omp writes `{"type":"message","message":{"role","content"}}` with `toolCall` blocks and
//! `toolResult` messages; Claude Code writes `{"type":"user"|"assistant","message":…}` with
//! `tool_use` blocks and `tool_result` blocks inside user messages. omp keeps the session
//! title as a `title` entry on the first line and each subagent's transcript next to the
//! session (`<session>/<Name>.jsonl`, its final report in `<Name>.md`).

use std::collections::HashMap;

use serde_json::{Value, json};

/// Longest tool output kept per call; the phone shows a preview, the terminal has the rest.
const OUTPUT_CHARS: usize = 1500;
const THINKING_CHARS: usize = 4000;
const COMPACTION_CHARS: usize = 400;

/// The conversation: `{items, messages, title, truncated}`, newest `limit` items, oldest first.
/// `messages` (`[{role, text}]`, user and assistant text only) is what phones before the
/// timeline read. omp's `async-result` (a subagent finished) becomes a `result` item and a
/// `compaction` a short `compaction` item; a tool call without an intent of its own takes
/// the one omp logged when it started it (`tool_execution_start`).
pub fn read(path: &str, limit: usize) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut items: Vec<Value> = Vec::new();
    // Tool call id → index in `items`, to attach its result.
    let mut calls: HashMap<String, usize> = HashMap::new();
    let mut title = None;
    // `tool_execution_start` intents by call id, applied to tool items once all is read: the
    // entry follows the call in the file, so it is not known while the call is pushed.
    let mut intents: HashMap<String, String> = HashMap::new();
    for v in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        match v["type"].as_str() {
            Some("title" | "title_change") => {
                if let Some(t) = clean_title(v["title"].as_str().unwrap_or_default()) {
                    title = Some(t);
                }
            }
            Some("message" | "user" | "assistant") => {
                push_message(&v["message"], &mut items, &mut calls)
            }
            Some("response_item") => push_codex(&v["payload"], &mut items, &mut calls),
            Some("custom") if v["customType"] == "tool_execution_start" => {
                if let (Some(id), Some(i)) = (
                    v["data"]["toolCallId"].as_str(),
                    v["data"]["intent"].as_str(),
                ) {
                    intents.insert(id.to_string(), i.to_string());
                }
            }
            Some("custom_message") if v["customType"] == "async-result" => {
                push_results(&v, &mut items)
            }
            Some("compaction") => {
                let t = v["shortSummary"]
                    .as_str()
                    .or_else(|| v["summary"].as_str())
                    .unwrap_or_default()
                    .trim();
                items.push(json!({"kind": "compaction", "text": clip(t, COMPACTION_CHARS)}));
            }
            _ => {}
        }
    }
    for item in &mut items {
        if item["kind"] == "tool"
            && item["intent"].is_null()
            && let Some(i) = item["id"].as_str().and_then(|id| intents.get(id))
        {
            item["intent"] = json!(i);
        }
    }
    let truncated = items.len() > limit;
    if truncated {
        items.drain(..items.len() - limit);
    }
    let messages: Vec<Value> = items
        .iter()
        .filter(|i| matches!(i["kind"].as_str(), Some("user" | "assistant")))
        .map(|i| json!({"role": i["kind"], "text": i["text"]}))
        .collect();
    Ok(json!({"items": items, "messages": messages, "title": title, "truncated": truncated}))
}

fn push_message(m: &Value, items: &mut Vec<Value>, calls: &mut HashMap<String, usize>) {
    let role = m["role"].as_str().unwrap_or_default();
    match role {
        "toolResult" => {
            let id = m["toolCallId"].as_str().unwrap_or_default();
            attach_result(items, calls, id, &m["content"], m["isError"] == true);
        }
        "user" | "assistant" => {
            let mut text = Vec::new();
            let flush = |text: &mut Vec<String>, items: &mut Vec<Value>| {
                let t = text.join("\n").trim().to_string();
                text.clear();
                if !t.is_empty() {
                    items.push(json!({"kind": role, "text": t}));
                }
            };
            let blocks = match &m["content"] {
                Value::String(s) => vec![json!({"type": "text", "text": s})],
                Value::Array(a) => a.clone(),
                _ => Vec::new(),
            };
            for b in &blocks {
                match b["type"].as_str() {
                    Some("text") => text.push(b["text"].as_str().unwrap_or_default().to_string()),
                    Some("thinking") => {
                        let t = b["thinking"].as_str().unwrap_or_default().trim();
                        if !t.is_empty() {
                            flush(&mut text, items);
                            items
                                .push(json!({"kind": "thinking", "text": clip(t, THINKING_CHARS)}));
                        }
                    }
                    Some("toolCall" | "tool_use") => {
                        flush(&mut text, items);
                        let args = if b["arguments"].is_null() {
                            &b["input"]
                        } else {
                            &b["arguments"]
                        };
                        let id = b["id"].as_str().unwrap_or_default().to_string();
                        calls.insert(id.clone(), items.len());
                        items.push(tool_item(
                            &id,
                            b["name"].as_str().unwrap_or("tool"),
                            args,
                            b["intent"].as_str(),
                        ));
                    }
                    Some("tool_result") => {
                        let id = b["tool_use_id"].as_str().unwrap_or_default();
                        attach_result(items, calls, id, &b["content"], b["is_error"] == true);
                    }
                    _ => {}
                }
            }
            flush(&mut text, items);
        }
        _ => {}
    }
}

/// Codex's rollout (`~/.codex/sessions/…/rollout-*.jsonl`): `response_item` payloads.
/// User messages that are context (`<environment_context>`, `<user_instructions>`, AGENTS.md)
/// start with a tag and are not turns; developer messages never are.
fn push_codex(p: &Value, items: &mut Vec<Value>, calls: &mut HashMap<String, usize>) {
    match p["type"].as_str() {
        Some("message") => {
            let role = p["role"].as_str().unwrap_or_default();
            if role != "user" && role != "assistant" {
                return;
            }
            let text = p["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let text = text.trim();
            if text.is_empty()
                || (role == "user" && (text.starts_with('<') || text.starts_with("# AGENTS.md")))
            {
                return;
            }
            items.push(json!({"kind": role, "text": text}));
        }
        Some("reasoning") => {
            let t = p["summary"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if !t.trim().is_empty() {
                items.push(json!({"kind": "thinking", "text": clip(t.trim(), THINKING_CHARS)}));
            }
        }
        Some("function_call" | "custom_tool_call" | "local_shell_call") => {
            let id = p["call_id"].as_str().unwrap_or_default().to_string();
            // `arguments` is a JSON string; `input` (custom tools) is free text.
            let args = match p["arguments"].as_str().map(serde_json::from_str::<Value>) {
                Some(Ok(v)) => v,
                _ => json!({"command": p["input"].as_str().or(p["arguments"].as_str())
                    .map(|s| clip(s, 300))}),
            };
            let args = match &args["cmd"] {
                Value::String(c) => json!({"command": c}),
                Value::Array(a) => {
                    json!({"command": a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")})
                }
                _ => args,
            };
            calls.insert(id.clone(), items.len());
            items.push(tool_item(
                &id,
                p["name"].as_str().unwrap_or("shell"),
                &args,
                None,
            ));
        }
        Some("function_call_output" | "custom_tool_call_output") => {
            let id = p["call_id"].as_str().unwrap_or_default();
            let out = match &p["output"] {
                Value::String(s) => serde_json::from_str::<Value>(s)
                    .ok()
                    .and_then(|v| v["output"].as_str().map(str::to_string))
                    .map_or_else(|| p["output"].clone(), Value::String),
                other => other.clone(),
            };
            attach_result(items, calls, id, &out, false);
        }
        _ => {}
    }
}

/// One tool call: `{kind: tool, id, name, intent, detail, status: running}`; a `task` call
/// also lists the subagents it starts (`agents: [{name, agent, task}]`).
fn tool_item(id: &str, name: &str, args: &Value, intent: Option<&str>) -> Value {
    let intent = intent.or_else(|| args["i"].as_str()).map(str::to_string);
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
    .map(|s| clip(s.trim(), 300));
    let mut item = json!({"kind": "tool", "id": id, "name": name, "intent": intent,
                          "detail": detail, "status": "running"});
    if (name == "task" || name == "Task")
        && let Some(tasks) = args["tasks"].as_array()
    {
        item["agents"] = tasks
            .iter()
            .map(|t| {
                json!({"name": t["name"], "agent": t["agent"],
                       "task": clip(t["task"].as_str().unwrap_or_default().trim(), 400)})
            })
            .collect();
    }
    item
}

fn attach_result(
    items: &mut [Value],
    calls: &HashMap<String, usize>,
    id: &str,
    content: &Value,
    error: bool,
) {
    let Some(item) = calls.get(id).and_then(|&i| items.get_mut(i)) else {
        return;
    };
    item["status"] = json!(if error { "error" } else { "ok" });
    let out = match content {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let out = out.trim();
    if !out.is_empty() {
        item["output"] = json!(clip(out, OUTPUT_CHARS));
    }
}

/// `s` cut to `max` chars, with `…` when cut.
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// A title without the leading icon glyphs omp puts in it (private-use code points).
fn clean_title(t: &str) -> Option<String> {
    let t = t
        .trim_start_matches(|c: char| {
            c.is_whitespace() || ('\u{e000}'..='\u{f8ff}').contains(&c) || c >= '\u{f0000}'
        })
        .trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// The session's title, read from the head of the file where omp keeps it (rewritten in
/// place), so a list can name an agent without reading its whole transcript.
pub fn title(path: &str) -> Option<String> {
    use std::io::Read;
    let mut head = vec![0; 8192];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    String::from_utf8_lossy(&head[..n])
        .lines()
        .take(4)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["type"] == "title")
        .and_then(|v| clean_title(v["title"].as_str()?))
}

/// omp's `async-result`: one `result` item per finished task, `{id, status, duration}`
/// (`duration` as omp wrote it, `27m20s`; a bare `details.jobs[]` entry carries ms).
fn push_results(v: &Value, items: &mut Vec<Value>) {
    let text = v["content"].as_str().unwrap_or_default();
    let from_text = super::tree::task_results(text);
    if !from_text.is_empty() {
        for (id, status, duration) in from_text {
            items.push(json!({"kind": "result", "id": id, "status": status, "duration": duration}));
        }
        return;
    }
    for j in v["details"]["jobs"].as_array().into_iter().flatten() {
        if j["type"] == "task"
            && let (Some(id), Some(status)) = (j["jobId"].as_str(), j["status"].as_str())
        {
            let duration = j["durationMs"].as_u64().map(|ms| format!("{}s", ms / 1000));
            items.push(json!({"kind": "result", "id": id, "status": status, "duration": duration}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(lines: &[String]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.jsonl");
        std::fs::write(&p, lines.join("\n")).unwrap();
        let p = p.to_string_lossy().into_owned();
        (dir, p)
    }

    fn msg(kind: &str, role: &str, content: Value) -> String {
        json!({"type": kind, "message": {"role": role, "content": content}}).to_string()
    }

    fn kinds(v: &Value) -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["kind"].as_str().unwrap().to_string())
            .collect()
    }

    /// Older phones read `messages`: still only user and assistant text, in both formats.
    #[test]
    fn messages_keep_user_and_assistant_text_only_in_both_session_formats() {
        let (_d, p) = session(&[
            json!({"type": "session", "id": "x"}).to_string(),
            msg("message", "user", json!("oi")),
            msg("message", "toolResult", json!("ls output")),
            msg(
                "message",
                "assistant",
                json!([{"type": "toolCall", "id": "c", "name": "bash", "arguments": {}}]),
            ),
            msg(
                "message",
                "assistant",
                json!([{"type": "text", "text": "oi do Mac"}]),
            ),
            json!({"type": "custom_message", "content": "atlas dev is RUNNING"}).to_string(),
            "not json".to_string(),
            msg("user", "user", json!([{"type": "text", "text": "lista"}])),
            msg(
                "assistant",
                "assistant",
                json!([{"type": "text", "text": "- api"}, {"type": "text", "text": "- web"}]),
            ),
        ]);
        let v = read(&p, 200).unwrap();
        let got: Vec<(&str, &str)> = v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x["role"].as_str().unwrap(), x["text"].as_str().unwrap()))
            .collect();
        assert_eq!(
            got,
            [
                ("user", "oi"),
                ("assistant", "oi do Mac"),
                ("user", "lista"),
                ("assistant", "- api\n- web")
            ]
        );
    }

    /// The phone shows what the agent is doing: each tool call with its intent, joined to
    /// its result by id (omp `toolResult` messages and Claude `tool_result` blocks), an
    /// error flagged, and a call still without a result shown as running.
    #[test]
    fn tool_calls_carry_intent_and_their_result_in_both_formats() {
        let (_d, p) = session(&[
            msg("message", "assistant", json!([
                {"type": "thinking", "thinking": "plan it"},
                {"type": "text", "text": "Vou olhar."},
                {"type": "toolCall", "id": "a", "name": "bash", "intent": "Listing files", "arguments": {"command": "ls -la"}},
                {"type": "toolCall", "id": "b", "name": "read", "arguments": {"path": "x.rs", "i": "Reading x"}},
            ])),
            json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "b", "isError": true,
                   "content": [{"type": "text", "text": "no such file"}]}}).to_string(),
            msg("assistant", "assistant", json!([{"type": "tool_use", "id": "c", "name": "Grep", "input": {"pattern": "fn main"}}])),
            msg("user", "user", json!([{"type": "tool_result", "tool_use_id": "c", "content": "src/main.rs:1"}])),
        ]);
        let v = read(&p, 200).unwrap();
        assert_eq!(kinds(&v), ["thinking", "assistant", "tool", "tool", "tool"]);
        let it = &v["items"];
        assert_eq!(
            (
                it[2]["intent"].as_str(),
                it[2]["detail"].as_str(),
                it[2]["status"].as_str()
            ),
            (Some("Listing files"), Some("ls -la"), Some("running"))
        );
        assert_eq!(
            (
                it[3]["intent"].as_str(),
                it[3]["status"].as_str(),
                it[3]["output"].as_str()
            ),
            (Some("Reading x"), Some("error"), Some("no such file"))
        );
        assert_eq!(
            (
                it[4]["detail"].as_str(),
                it[4]["status"].as_str(),
                it[4]["output"].as_str()
            ),
            (Some("fn main"), Some("ok"), Some("src/main.rs:1"))
        );
        // A Claude tool result is a user message, but never a user turn.
        assert_eq!(v["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_task_call_lists_its_subagents() {
        let (_d, p) = session(&[msg(
            "message",
            "assistant",
            json!([{"type": "toolCall", "id": "t", "name": "task",
            "arguments": {"tasks": [{"name": "HostScout", "agent": "scout", "task": "map hostd"}, {"name": "UiFix", "task": "edit"}]}}]),
        )]);
        let v = read(&p, 200).unwrap();
        let agents = v["items"][0]["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(
            (
                agents[0]["name"].as_str(),
                agents[0]["agent"].as_str(),
                agents[0]["task"].as_str()
            ),
            (Some("HostScout"), Some("scout"), Some("map hostd"))
        );
    }

    /// Codex rollouts: context messages are not turns, function calls join their output.
    #[test]
    fn codex_rollout_becomes_a_timeline() {
        let ri = |p: Value| json!({"type": "response_item", "payload": p}).to_string();
        let (_d, p) = session(&[
            ri(
                json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]}),
            ),
            ri(
                json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>x"}]}),
            ),
            ri(
                json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "roda os testes"}]}),
            ),
            ri(
                json!({"type": "reasoning", "summary": [{"type": "summary_text", "text": "run cargo"}]}),
            ),
            ri(
                json!({"type": "function_call", "call_id": "c1", "name": "shell", "arguments": "{\"cmd\":[\"cargo\",\"test\"]}"}),
            ),
            ri(
                json!({"type": "function_call_output", "call_id": "c1", "output": "{\"output\":\"ok 3 passed\"}"}),
            ),
            ri(
                json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Passou."}]}),
            ),
        ]);
        let v = read(&p, 200).unwrap();
        assert_eq!(kinds(&v), ["user", "thinking", "tool", "assistant"]);
        let t = &v["items"][2];
        assert_eq!(
            (
                t["detail"].as_str(),
                t["status"].as_str(),
                t["output"].as_str()
            ),
            (Some("cargo test"), Some("ok"), Some("ok 3 passed"))
        );
    }

    /// A long tool output never ships whole to the phone.
    #[test]
    fn long_output_is_clipped_on_a_char_boundary() {
        let big = "é".repeat(OUTPUT_CHARS + 10);
        let (_d, p) = session(&[
            msg("message", "assistant", json!([{"type": "toolCall", "id": "a", "name": "bash", "arguments": {}}])),
            json!({"type": "message", "message": {"role": "toolResult", "toolCallId": "a", "content": big}}).to_string(),
        ]);
        let out = read(&p, 200).unwrap()["items"][0]["output"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(out.chars().count(), OUTPUT_CHARS + 1);
        assert!(out.ends_with('…'));
    }

    /// omp rewrites the title line in place; the last title wins and its icon glyph goes.
    #[test]
    fn title_is_read_without_its_icon_and_empty_titles_are_none() {
        let (_d, p) = session(&[
            json!({"type": "title", "title": "\u{f011c} MOBILE: Improve omp agents"}).to_string(),
            msg("message", "user", json!("oi")),
        ]);
        assert_eq!(title(&p).as_deref(), Some("MOBILE: Improve omp agents"));
        assert_eq!(read(&p, 10).unwrap()["title"], "MOBILE: Improve omp agents");
        let (_d2, p2) = session(&[json!({"type": "title", "title": ""}).to_string()]);
        assert_eq!(title(&p2), None);
        assert_eq!(title("/nope.jsonl"), None);
    }

    #[test]
    fn limit_keeps_the_newest_items_and_says_so() {
        let lines: Vec<String> = (0..10)
            .map(|i| msg("message", "user", json!(format!("m{i}"))))
            .collect();
        let (_d, p) = session(&lines);
        let v = read(&p, 3).unwrap();
        assert_eq!(v["truncated"], true);
        let texts: Vec<&str> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, ["m7", "m8", "m9"]);
        assert!(read("/nope/missing.jsonl", 3).is_err());
    }

    /// Subagents are listed from the session dir: a report means done, a fresh transcript
    /// A subagent that ended shows in its parent's timeline as a `result`, whether omp wrote
    /// the `<task-result>` wrapper or only the `details.jobs[]` entry; a bash job is not one.
    #[test]
    fn async_results_become_result_items() {
        let (_d, p) = session(&[
            json!({"type": "custom_message", "customType": "async-result",
                   "content": "<system-notice>\nBackground job Scout has completed.\n<task-result id=\"Scout\" agent=\"scout\" status=\"completed\" duration=\"27m20s\">\n<output>x</output>"})
            .to_string(),
            json!({"type": "custom_message", "customType": "async-result", "content": "done",
                   "details": {"jobs": [
                       {"jobId": "Fixer", "type": "task", "status": "failed", "durationMs": 4200},
                       {"jobId": "bg_240", "type": "bash", "status": "completed", "durationMs": 9}]}})
            .to_string(),
            json!({"type": "custom_message", "customType": "atlas-context", "content": "ctx"}).to_string(),
        ]);
        let v = read(&p, 200).unwrap();
        assert_eq!(kinds(&v), ["result", "result"]);
        let it = &v["items"];
        assert_eq!(
            (
                it[0]["id"].as_str(),
                it[0]["status"].as_str(),
                it[0]["duration"].as_str()
            ),
            (Some("Scout"), Some("completed"), Some("27m20s"))
        );
        assert_eq!(
            (
                it[1]["id"].as_str(),
                it[1]["status"].as_str(),
                it[1]["duration"].as_str()
            ),
            (Some("Fixer"), Some("failed"), Some("4s"))
        );
        assert!(v["messages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_compaction_becomes_a_short_item() {
        let (_d, p) = session(&[
            json!({"type": "compaction", "summary": "long long summary", "shortSummary": "Mapped hostd",
                   "firstKeptEntryId": "a", "tokensBefore": 9000})
            .to_string(),
            json!({"type": "compaction", "summary": "only the long one"}).to_string(),
        ]);
        let v = read(&p, 200).unwrap();
        assert_eq!(kinds(&v), ["compaction", "compaction"]);
        assert_eq!(v["items"][0]["text"], "Mapped hostd");
        assert_eq!(v["items"][1]["text"], "only the long one");
    }

    /// omp logs the intent of a call in `tool_execution_start`, after the call itself.
    #[test]
    fn a_tool_without_an_intent_takes_the_one_omp_logged_at_start() {
        let (_d, p) = session(&[
            msg(
                "message",
                "assistant",
                json!([
                    {"type": "toolCall", "id": "a", "name": "bash", "arguments": {"command": "ls"}},
                    {"type": "toolCall", "id": "b", "name": "read", "intent": "Own intent", "arguments": {"path": "x"}},
                ]),
            ),
            json!({"type": "custom", "customType": "tool_execution_start",
                   "data": {"toolCallId": "a", "toolName": "bash", "intent": "Listing files"}})
            .to_string(),
            json!({"type": "custom", "customType": "tool_execution_start",
                   "data": {"toolCallId": "b", "toolName": "read", "intent": "Logged intent"}})
            .to_string(),
        ]);
        let v = read(&p, 200).unwrap();
        assert_eq!(v["items"][0]["intent"], "Listing files");
        assert_eq!(v["items"][1]["intent"], "Own intent");
    }
}
