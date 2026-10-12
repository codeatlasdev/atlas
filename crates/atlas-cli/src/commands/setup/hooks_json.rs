//! Registering `atlas hook <harness> <event>` command hooks in a JSON file whose
//! top-level `hooks` object follows the Claude Code / Codex schema:
//!
//! ```json
//! { "hooks": { "<Event>": [ { "matcher": "...", "hooks": [ { "type": "command", "command": "..." } ] } ] } }
//! ```
//!
//! * Claude Code — `settings.json` (user: `<config dir>/settings.json`, project:
//!   `.claude/settings.json`): <https://code.claude.com/docs/en/hooks#configuration>
//! * Codex — `hooks.json` (user: `$CODEX_HOME/hooks.json`, project: `.codex/hooks.json`):
//!   <https://developers.openai.com/codex/hooks#config-shape>
//!
//! Ownership is the exact command string: a handler is atlas-owned iff its command is
//! `<atlas|…/atlas> hook <harness> <event> 2>/dev/null || true` for one of our events.
//! Everything else in the file (other hooks, other keys, key order) is preserved.

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::plan::{MergeOutcome, RemoveOutcome};
use crate::commands::hook::{Event, Harness};

/// Seconds. Both harnesses default to 600; the atlas hook reads one local file.
const TIMEOUT_SECS: u64 = 10;

/// What one event registers: harness event name, tool matcher (None = every occurrence).
struct Registration {
    event: Event,
    json_event: &'static str,
    matcher: Option<&'static str>,
}

/// Both harnesses name these events identically and run `Bash` for shell commands
/// (Claude: <https://code.claude.com/docs/en/hooks#pretooluse>,
/// Codex: <https://developers.openai.com/codex/hooks#pretooluse>). The status
/// events (prompt, tools, stop, approval) feed Atlas Desktop and are no-ops elsewhere.
const fn reg(
    event: Event,
    json_event: &'static str,
    matcher: Option<&'static str>,
) -> Registration {
    Registration {
        event,
        json_event,
        matcher,
    }
}

const COMMON: [Registration; 5] = [
    reg(Event::SessionStart, "SessionStart", None),
    reg(Event::PreToolUse, "PreToolUse", Some("Bash")),
    reg(Event::UserPromptSubmit, "UserPromptSubmit", None),
    reg(Event::PostToolUse, "PostToolUse", None),
    reg(Event::Stop, "Stop", None),
];
/// Approval prompts: Claude reports them as `Notification`, Codex as `PermissionRequest`.
const CLAUDE_ONLY: [Registration; 3] = [
    reg(Event::Notification, "Notification", None),
    reg(Event::StopFailure, "StopFailure", None),
    reg(Event::SessionEnd, "SessionEnd", None),
];
const CODEX_ONLY: [Registration; 1] = [reg(Event::PermissionRequest, "PermissionRequest", None)];

fn registrations(harness: Harness) -> impl Iterator<Item = &'static Registration> {
    let extra: &'static [Registration] = match harness {
        Harness::ClaudeCode => &CLAUDE_ONLY,
        Harness::Codex => &CODEX_ONLY,
    };
    COMMON.iter().chain(extra)
}

/// The shell command registered for `event`. The `2>/dev/null || true` guard matters:
/// both harnesses treat exit code 2 as a *blocking* error, and an older `atlas` on PATH
/// (no `hook` subcommand) or a missing binary must never block the agent's tool calls.
pub fn command_line(atlas: &str, harness: Harness, event: Event) -> String {
    format!(
        "{} hook {} {} 2>/dev/null || true",
        shell_quote(atlas),
        harness.name(),
        event.name()
    )
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Is `command` an atlas hook command for `harness`/`event`, whatever atlas path it uses?
fn is_owned_command(command: &str, harness: Harness, event: Event) -> bool {
    let suffix = format!(
        " hook {} {} 2>/dev/null || true",
        harness.name(),
        event.name()
    );
    let Some(bin) = command.strip_suffix(&suffix) else {
        return false;
    };
    let bin = bin
        .strip_prefix('\'')
        .and_then(|b| b.strip_suffix('\''))
        .unwrap_or(bin);
    bin == "atlas" || bin.ends_with("/atlas")
}

fn handler(command: &str) -> Value {
    json!({ "type": "command", "command": command, "timeout": TIMEOUT_SECS })
}

fn group(reg: &Registration, command: &str) -> Value {
    let mut g = Map::new();
    if let Some(m) = reg.matcher {
        g.insert("matcher".into(), json!(m));
    }
    g.insert("hooks".into(), json!([handler(command)]));
    Value::Object(g)
}

fn parse_root(existing: Option<&str>) -> Result<(Map<String, Value>, bool)> {
    match existing {
        None => Ok((Map::new(), true)),
        Some(t) if t.trim().is_empty() => Ok((Map::new(), true)),
        Some(t) => match serde_json::from_str::<Value>(t)? {
            Value::Object(m) => Ok((m, false)),
            _ => bail!("top-level JSON value is not an object"),
        },
    }
}

fn to_pretty(root: &Map<String, Value>) -> String {
    let mut s = serde_json::to_string_pretty(root).expect("serialize json");
    s.push('\n');
    s
}

/// Mutable access to `hooks.<json_event>` (creating it), erroring on ill-shaped files.
fn event_groups<'a>(
    root: &'a mut Map<String, Value>,
    json_event: &str,
) -> Result<&'a mut Vec<Value>> {
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        bail!("`hooks` is not an object");
    };
    let Some(groups) = hooks
        .entry(json_event)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
    else {
        bail!("`hooks.{json_event}` is not an array");
    };
    Ok(groups)
}

/// Indices (group, handler) of atlas-owned handlers in `groups` for this event.
fn owned_handlers(groups: &[Value], harness: Harness, event: Event) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        let Some(hs) = g.get("hooks").and_then(Value::as_array) else {
            continue;
        };
        for (hi, h) in hs.iter().enumerate() {
            let owned = h.get("type").and_then(Value::as_str) == Some("command")
                && h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| is_owned_command(c, harness, event));
            if owned {
                out.push((gi, hi));
            }
        }
    }
    out
}

/// Register both atlas hooks. Handlers we already own are refreshed in place (matcher and
/// sibling handlers of the user's group are kept); duplicates are dropped; missing ones
/// are appended as a new group. There is no foreign-slot conflict for hooks: only
/// unparsable / ill-shaped files are errors.
pub fn merge(existing: Option<&str>, harness: Harness, atlas: &str) -> Result<MergeOutcome> {
    let (mut root, created) = parse_root(existing)?;
    let before = Value::Object(root.clone());
    for reg in registrations(harness) {
        let command = command_line(atlas, harness, reg.event);
        let desired = handler(&command);
        let groups = event_groups(&mut root, reg.json_event)?;
        let owned = owned_handlers(groups, harness, reg.event);
        match owned.split_first() {
            None => groups.push(group(reg, &command)),
            Some((&(gi, hi), dups)) => {
                groups[gi]["hooks"][hi] = desired;
                // Drop duplicates last-to-first so indices stay valid.
                for &(dgi, dhi) in dups.iter().rev() {
                    if let Some(hs) = groups[dgi]["hooks"].as_array_mut() {
                        hs.remove(dhi);
                    }
                }
                groups.retain(|g| {
                    g.get("hooks")
                        .and_then(Value::as_array)
                        .is_none_or(|h| !h.is_empty())
                });
            }
        }
    }
    if Value::Object(root.clone()) == before && !created {
        return Ok(MergeOutcome::UpToDate);
    }
    let text = to_pretty(&root);
    Ok(if created {
        MergeOutcome::Create(text)
    } else {
        MergeOutcome::Update(text)
    })
}

/// Remove every atlas-owned handler; drop groups/events/`hooks` that become empty.
/// The file is deleted only when nothing is left (or only `description`, which atlas
/// never writes but Codex documents as optional `hooks.json` metadata).
pub fn remove(existing: Option<&str>, harness: Harness) -> Result<RemoveOutcome> {
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return Ok(RemoveOutcome::Absent);
    };
    let (mut root, _) = parse_root(Some(text))?;
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(RemoveOutcome::Absent);
    };
    let mut removed_any = false;
    for reg in registrations(harness) {
        let Some(groups) = hooks.get_mut(reg.json_event).and_then(Value::as_array_mut) else {
            continue;
        };
        for (gi, g) in groups.iter_mut().enumerate() {
            let _ = gi;
            if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let n = hs.len();
                hs.retain(|h| {
                    !(h.get("type").and_then(Value::as_str) == Some("command")
                        && h.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| is_owned_command(c, harness, reg.event)))
                });
                removed_any |= hs.len() != n;
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
        if groups.is_empty() {
            hooks.shift_remove(reg.json_event);
        }
    }
    if !removed_any {
        return Ok(RemoveOutcome::Absent);
    }
    if hooks.is_empty() {
        root.shift_remove("hooks");
    }
    let leftover = root.keys().all(|k| k == "description");
    Ok(RemoveOutcome::Removed(if leftover {
        None
    } else {
        Some(to_pretty(&root))
    }))
}

/// True when the file holds at least one atlas-owned hook handler for `harness`.
pub fn has_owned(existing: Option<&str>, harness: Harness) -> bool {
    let Some(Ok(Value::Object(root))) = existing.map(serde_json::from_str::<Value>) else {
        return false;
    };
    registrations(harness).any(|reg| {
        root.get("hooks")
            .and_then(|h| h.get(reg.json_event))
            .and_then(Value::as_array)
            .is_some_and(|g| !owned_handlers(g, harness, reg.event).is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::merge_text as text;

    #[test]
    fn command_line_and_ownership() {
        let c = command_line("atlas", Harness::ClaudeCode, Event::PreToolUse);
        assert_eq!(c, "atlas hook claude-code pre-tool-use 2>/dev/null || true");
        assert!(is_owned_command(&c, Harness::ClaudeCode, Event::PreToolUse));
        assert!(!is_owned_command(&c, Harness::Codex, Event::PreToolUse));
        assert!(!is_owned_command(
            &c,
            Harness::ClaudeCode,
            Event::SessionStart
        ));
        let abs = command_line(
            "/Users/a b/.atlas/bin/atlas",
            Harness::Codex,
            Event::SessionStart,
        );
        assert!(abs.starts_with("'/Users/a b/.atlas/bin/atlas' hook codex session-start"));
        assert!(is_owned_command(&abs, Harness::Codex, Event::SessionStart));
        // Someone else's command that merely mentions atlas is never ours.
        assert!(!is_owned_command(
            "myatlas hook claude-code session-start 2>/dev/null || true",
            Harness::ClaudeCode,
            Event::SessionStart
        ));
        assert!(!is_owned_command(
            "atlas hook claude-code session-start",
            Harness::ClaudeCode,
            Event::SessionStart
        ));
    }

    #[test]
    fn create_idempotent_and_shape() {
        let t = text(merge(None, Harness::ClaudeCode, "atlas").unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        let ss = &v["hooks"]["SessionStart"][0];
        assert!(ss.get("matcher").is_none());
        assert_eq!(ss["hooks"][0]["type"], "command");
        assert_eq!(
            ss["hooks"][0]["command"],
            "atlas hook claude-code session-start 2>/dev/null || true"
        );
        let pre = &v["hooks"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "Bash");
        assert_eq!(
            merge(Some(&t), Harness::ClaudeCode, "atlas").unwrap(),
            MergeOutcome::UpToDate
        );
    }

    #[test]
    fn preserves_user_hooks_keys_and_order() {
        let existing = r#"{
  "permissions": {"allow": ["Bash(ls)"]},
  "hooks": {
    "PreToolUse": [
      {"matcher": "Edit", "hooks": [{"type": "command", "command": "echo mine"}]}
    ],
    "Stop": [{"hooks": [{"type": "command", "command": "echo stop"}]}]
  },
  "model": "opus"
}"#;
        let t = text(merge(Some(existing), Harness::ClaudeCode, "atlas").unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "echo stop");
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], "echo mine");
        assert_eq!(pre[1]["matcher"], "Bash");
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["permissions", "hooks", "model"]);
    }

    #[test]
    fn owned_handler_with_old_path_is_refreshed_in_place_keeping_siblings_and_matcher() {
        let existing = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash|PowerShell","hooks":[
          {"type":"command","command":"echo mine"},
          {"type":"command","command":"/old/bin/atlas hook claude-code pre-tool-use 2>/dev/null || true"}
        ]}]}}"#;
        let t = text(merge(Some(existing), Harness::ClaudeCode, "atlas").unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        let g = &v["hooks"]["PreToolUse"][0];
        assert_eq!(g["matcher"], "Bash|PowerShell");
        assert_eq!(g["hooks"][0]["command"], "echo mine");
        assert_eq!(
            g["hooks"][1]["command"],
            "atlas hook claude-code pre-tool-use 2>/dev/null || true"
        );
        assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn duplicate_owned_handlers_collapse_to_one() {
        let cmd = "atlas hook codex session-start 2>/dev/null || true";
        let existing = format!(
            r#"{{"hooks":{{"SessionStart":[
              {{"hooks":[{{"type":"command","command":"{cmd}"}}]}},
              {{"hooks":[{{"type":"command","command":"{cmd}"}}]}}]}}}}"#
        );
        let t = text(merge(Some(&existing), Harness::Codex, "atlas").unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn other_harness_hooks_are_not_ours() {
        let t = text(merge(None, Harness::Codex, "atlas").unwrap());
        // A Claude-flavoured merge into a Codex file adds its own, keeps codex's.
        let both = text(merge(Some(&t), Harness::ClaudeCode, "atlas").unwrap());
        assert!(has_owned(Some(&both), Harness::Codex));
        assert!(has_owned(Some(&both), Harness::ClaudeCode));
        let RemoveOutcome::Removed(Some(after)) = remove(Some(&both), Harness::ClaudeCode).unwrap()
        else {
            panic!("codex hooks must remain");
        };
        assert!(has_owned(Some(&after), Harness::Codex));
        assert!(!has_owned(Some(&after), Harness::ClaudeCode));
    }

    #[test]
    fn remove_keeps_user_content_and_deletes_atlas_only_file() {
        let created = text(merge(None, Harness::ClaudeCode, "atlas").unwrap());
        assert_eq!(
            remove(Some(&created), Harness::ClaudeCode).unwrap(),
            RemoveOutcome::Removed(None)
        );
        let mixed = r#"{"theme":"dark","hooks":{"PreToolUse":[
          {"matcher":"Edit","hooks":[{"type":"command","command":"echo mine"}]},
          {"matcher":"Bash","hooks":[{"type":"command","command":"atlas hook claude-code pre-tool-use 2>/dev/null || true","timeout":10}]}]}}"#;
        let RemoveOutcome::Removed(Some(t)) = remove(Some(mixed), Harness::ClaudeCode).unwrap()
        else {
            panic!("expected rewrite");
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["theme"], "dark");
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 1);
        assert_eq!(pre[0]["hooks"][0]["command"], "echo mine");
    }

    #[test]
    fn remove_absent_cases_and_user_group_sharing_a_handler_survives() {
        assert_eq!(remove(None, Harness::Codex).unwrap(), RemoveOutcome::Absent);
        assert_eq!(
            remove(Some("{}"), Harness::Codex).unwrap(),
            RemoveOutcome::Absent
        );
        assert_eq!(
            remove(Some(r#"{"hooks":{"Stop":[{"hooks":[]}]}}"#), Harness::Codex).unwrap(),
            RemoveOutcome::Absent
        );
        // A user's handler inside the same group as ours stays.
        let shared = r#"{"hooks":{"SessionStart":[{"hooks":[
          {"type":"command","command":"echo mine"},
          {"type":"command","command":"atlas hook codex session-start 2>/dev/null || true"}]}]}}"#;
        let RemoveOutcome::Removed(Some(t)) = remove(Some(shared), Harness::Codex).unwrap() else {
            panic!("expected rewrite");
        };
        assert!(t.contains("echo mine") && !t.contains("atlas hook"));
    }

    #[test]
    fn ill_shaped_files_are_errors_never_overwritten() {
        assert!(merge(Some("{nope"), Harness::Codex, "atlas").is_err());
        assert!(merge(Some("[]"), Harness::Codex, "atlas").is_err());
        assert!(merge(Some(r#"{"hooks":[]}"#), Harness::Codex, "atlas").is_err());
        assert!(
            merge(
                Some(r#"{"hooks":{"PreToolUse":{}}}"#),
                Harness::Codex,
                "atlas"
            )
            .is_err()
        );
        assert!(remove(Some("{nope"), Harness::Codex).is_err());
    }
}
