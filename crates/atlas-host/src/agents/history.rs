//! An agent's conversation, read from its session `.jsonl` (the path arrives in
//! `AgentStatus.session_file`). Only user and assistant text is kept: tool calls and
//! results, context injections and system entries are not part of the conversation.
//!
//! omp writes `{"type":"message","message":{"role","content"}}`; Claude Code writes
//! `{"type":"user"|"assistant","message":{"role","content"}}`. `content` is a string or a
//! list of blocks, of which only `{"type":"text"}` blocks are read.

use serde_json::{Value, json};

/// The newest `limit` turns, oldest first, and whether older ones were dropped.
pub fn read(path: &str, limit: usize) -> Result<(Vec<Value>, bool), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut all: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| matches!(v["type"].as_str(), Some("message" | "user" | "assistant")))
        .filter_map(|v| {
            let m = &v["message"];
            let role = m["role"].as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let text = message_text(&m["content"]);
            (!text.is_empty()).then(|| json!({"role": role, "text": text}))
        })
        .collect();
    let truncated = all.len() > limit;
    if truncated {
        all.drain(..all.len() - limit);
    }
    Ok((all, truncated))
}

fn message_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(parts) => parts
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
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

    /// The phone shows the conversation, not tool traffic: a tool result or an empty
    /// tool-only assistant turn in the history would be noise on a small screen.
    #[test]
    fn keeps_user_and_assistant_text_only_in_both_session_formats() {
        let (_d, p) = session(&[
            json!({"type": "session", "id": "x"}).to_string(),
            msg("message", "user", json!("oi")),
            msg("message", "toolResult", json!("ls output")),
            msg(
                "message",
                "assistant",
                json!([{"type": "tool_use", "name": "bash"}]),
            ),
            msg(
                "message",
                "assistant",
                json!([{"type": "text", "text": "oi do Mac"}]),
            ),
            json!({"type": "custom_message", "content": "atlas dev is RUNNING"}).to_string(),
            "not json".to_string(),
            // Claude Code's shape.
            msg("user", "user", json!([{"type": "text", "text": "lista"}])),
            msg(
                "assistant",
                "assistant",
                json!([{"type": "text", "text": "- api"}, {"type": "text", "text": "- web"}]),
            ),
        ]);
        let (m, truncated) = read(&p, 200).unwrap();
        assert!(!truncated);
        let got: Vec<(&str, &str)> = m
            .iter()
            .map(|x| (x["role"].as_str().unwrap(), x["text"].as_str().unwrap()))
            .collect();
        assert_eq!(
            got,
            [
                ("user", "oi"),
                ("assistant", "oi do Mac"),
                ("user", "lista"),
                ("assistant", "- api\n- web"),
            ]
        );
    }

    #[test]
    fn limit_keeps_the_newest_turns_and_says_so() {
        let lines: Vec<String> = (0..10)
            .map(|i| msg("message", "user", json!(format!("m{i}"))))
            .collect();
        let (_d, p) = session(&lines);
        let (m, truncated) = read(&p, 3).unwrap();
        assert!(truncated);
        let texts: Vec<&str> = m.iter().map(|x| x["text"].as_str().unwrap()).collect();
        assert_eq!(texts, ["m7", "m8", "m9"]);
        assert!(read("/nope/missing.jsonl", 3).is_err());
    }
}
