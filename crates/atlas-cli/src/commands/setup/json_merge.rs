//! Merging the `atlas` MCP server entry into a JSON MCP config (`mcpServers.atlas`),
//! preserving every unrelated key and server (and their order), with strict
//! ownership checks. Shared by OMP (`mcp.json`) and Claude Code (`~/.claude.json`,
//! `.mcp.json`).

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::plan::{MergeOutcome, RemoveOutcome};

pub const SERVER_NAME: &str = "atlas";

/// A JSON MCP config dialect: the entry atlas writes and the `$schema` stamped on new files.
#[derive(Clone, Copy)]
pub struct Flavor {
    /// `$schema` added only when atlas creates the file; `None` = no schema key.
    pub schema: Option<&'static str>,
    /// Entry for `command` (`atlas` when on PATH, else an absolute path).
    pub entry: fn(&str) -> Value,
}

/// OMP's legacy `mcp.json` dialect (atlas no longer writes it; the plugin ships `.mcp.json`).
/// Kept for the generic merge tests below.
#[cfg(test)]
pub const OMP: Flavor = Flavor {
    schema: Some(
        "https://raw.githubusercontent.com/can1357/oh-my-pi/main/packages/coding-agent/src/config/mcp-schema.json",
    ),
    entry: |command| json!({ "type": "stdio", "command": command, "args": ["mcp"] }),
};

/// Claude Code writes exactly this shape for `claude mcp add --scope user|project atlas -- atlas mcp`
/// (verified against Claude Code 2.1.292), so entries created by either tool compare equal.
/// Docs: <https://code.claude.com/docs/en/mcp#option-3-add-a-local-stdio-server>
pub const CLAUDE: Flavor = Flavor {
    schema: None,
    entry: |command| json!({ "type": "stdio", "command": command, "args": ["mcp"], "env": {} }),
};

/// An entry is atlas-owned when it is a stdio server whose command is `atlas`
/// (bare or any path ending in `/atlas`) running exactly `mcp`.
pub fn is_owned(entry: &Value) -> bool {
    let Some(obj) = entry.as_object() else {
        return false;
    };
    let ty_ok = obj.get("type").is_none_or(|t| t.as_str() == Some("stdio"));
    let cmd_ok = obj
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c == "atlas" || c.ends_with("/atlas"));
    let args_ok = obj
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|a| a.len() == 1 && a[0].as_str() == Some("mcp"));
    // A non-empty `env` is the user's customisation: replacing the entry would drop it.
    let env_ok = obj
        .get("env")
        .is_none_or(|e| e.as_object().is_some_and(Map::is_empty));
    ty_ok && cmd_ok && args_ok && env_ok && !obj.contains_key("url")
}

/// Parse `existing` (None = file absent) and compute the merged document.
/// Errors only on unparsable/ill-shaped JSON (never overwrite what we can't read).
pub fn merge(
    flavor: Flavor,
    existing: Option<&str>,
    command: &str,
    force: bool,
) -> Result<MergeOutcome> {
    let desired = (flavor.entry)(command);
    let (mut root, created_file) = match existing {
        None => (Map::new(), true),
        Some(text) if text.trim().is_empty() => (Map::new(), true),
        Some(text) => match serde_json::from_str::<Value>(text)? {
            Value::Object(m) => (m, false),
            _ => bail!("top-level JSON value is not an object"),
        },
    };

    if created_file && let Some(schema) = flavor.schema {
        root.insert("$schema".into(), json!(schema));
    }
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        bail!("`mcpServers` is not an object");
    };

    match servers.get(SERVER_NAME) {
        Some(cur) if *cur == desired => return Ok(MergeOutcome::UpToDate),
        Some(cur) if !is_owned(cur) && !force => {
            return Ok(MergeOutcome::Conflict(cur.to_string()));
        }
        Some(_) => {
            servers.insert(SERVER_NAME.into(), desired);
            return Ok(MergeOutcome::Update(to_pretty(&root)));
        }
        None => {
            servers.insert(SERVER_NAME.into(), desired);
        }
    }
    let text = to_pretty(&root);
    Ok(if created_file {
        MergeOutcome::Create(text)
    } else {
        MergeOutcome::Update(text)
    })
}

/// Remove the atlas-owned entry, keeping everything else.
pub fn remove(existing: Option<&str>, force: bool) -> Result<RemoveOutcome> {
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return Ok(RemoveOutcome::Absent);
    };
    let Value::Object(mut root) = serde_json::from_str::<Value>(text)? else {
        bail!("top-level JSON value is not an object");
    };
    let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else {
        return Ok(RemoveOutcome::Absent);
    };
    match servers.get(SERVER_NAME) {
        None => return Ok(RemoveOutcome::Absent),
        Some(cur) if !is_owned(cur) && !force => return Ok(RemoveOutcome::Foreign),
        Some(_) => {
            servers.shift_remove(SERVER_NAME);
        }
    }
    let servers_empty = servers.is_empty();
    if servers_empty {
        root.shift_remove("mcpServers");
    }
    // Delete the file only when nothing but our own scaffolding remains.
    let only_scaffold = root.keys().all(|k| k == "$schema");
    Ok(RemoveOutcome::Removed(if only_scaffold {
        None
    } else {
        Some(to_pretty(&root))
    }))
}

fn to_pretty(root: &Map<String, Value>) -> String {
    let mut s = serde_json::to_string_pretty(root).expect("serialize json");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::merge_text as text;

    #[test]
    fn creates_new_file_with_schema() {
        let t = text(merge(OMP, None, "atlas", false).unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["$schema"], OMP.schema.unwrap());
        assert_eq!(v["mcpServers"]["atlas"], (OMP.entry)("atlas"));
        assert!(matches!(
            merge(OMP, None, "atlas", false).unwrap(),
            MergeOutcome::Create(_)
        ));
    }

    #[test]
    fn empty_file_is_treated_as_new() {
        assert!(matches!(
            merge(OMP, Some("  \n"), "atlas", false).unwrap(),
            MergeOutcome::Create(_)
        ));
    }

    #[test]
    fn preserves_siblings_and_unrelated_keys() {
        let existing = r#"{"mcpServers":{"other":{"command":"x","args":["y"]}},"disabledServers":["z"],"custom":1}"#;
        let o = merge(OMP, Some(existing), "atlas", false).unwrap();
        assert!(matches!(o, MergeOutcome::Update(_)));
        let v: Value = serde_json::from_str(&text(o)).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["disabledServers"][0], "z");
        assert_eq!(v["custom"], 1);
        assert_eq!(v["mcpServers"]["atlas"]["args"][0], "mcp");
        // $schema is only added when creating, never injected into user files.
        assert!(v.get("$schema").is_none());
    }

    #[test]
    fn idempotent() {
        let first = text(merge(OMP, None, "atlas", false).unwrap());
        assert_eq!(
            merge(OMP, Some(&first), "atlas", false).unwrap(),
            MergeOutcome::UpToDate
        );
    }

    #[test]
    fn foreign_entry_conflicts_unless_forced() {
        let existing = r#"{"mcpServers":{"atlas":{"command":"node","args":["server.js"]}}}"#;
        assert!(matches!(
            merge(OMP, Some(existing), "atlas", false).unwrap(),
            MergeOutcome::Conflict(_)
        ));
        let forced = merge(OMP, Some(existing), "atlas", true).unwrap();
        let v: Value = serde_json::from_str(&text(forced)).unwrap();
        assert_eq!(v["mcpServers"]["atlas"], (OMP.entry)("atlas"));
    }

    #[test]
    fn owned_entry_with_different_path_is_updated_not_conflict() {
        let existing = r#"{"mcpServers":{"atlas":{"type":"stdio","command":"/old/bin/atlas","args":["mcp"]}}}"#;
        assert!(matches!(
            merge(OMP, Some(existing), "atlas", false).unwrap(),
            MergeOutcome::Update(_)
        ));
    }

    #[test]
    fn atlas_entry_with_extra_args_or_url_is_foreign() {
        assert!(!is_owned(
            &json!({"command":"atlas","args":["mcp","--dir","x"]})
        ));
        assert!(!is_owned(&json!({"type":"http","url":"https://x"})));
        assert!(!is_owned(&json!({"command":"atlasx","args":["mcp"]})));
        assert!(is_owned(
            &json!({"command":"/Users/a/.atlas/bin/atlas","args":["mcp"]})
        ));
    }

    #[test]
    fn invalid_json_is_an_error_not_overwritten() {
        assert!(merge(OMP, Some("{not json"), "atlas", true).is_err());
        assert!(merge(OMP, Some("[]"), "atlas", true).is_err());
        assert!(merge(OMP, Some(r#"{"mcpServers":[]}"#), "atlas", true).is_err());
    }

    #[test]
    fn remove_keeps_siblings_and_deletes_scaffold_only_file() {
        let created = text(merge(OMP, None, "atlas", false).unwrap());
        assert_eq!(
            remove(Some(&created), false).unwrap(),
            RemoveOutcome::Removed(None)
        );

        let with_sibling = r#"{"mcpServers":{"atlas":{"type":"stdio","command":"atlas","args":["mcp"]},"other":{"command":"x"}}}"#;
        let RemoveOutcome::Removed(Some(t)) = remove(Some(with_sibling), false).unwrap() else {
            panic!("expected rewritten file");
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        assert!(v["mcpServers"].get("atlas").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn claude_flavor_matches_claude_mcp_add_output_and_has_no_schema() {
        // Exactly what `claude mcp add --scope user atlas -- atlas mcp` writes.
        let theirs = r#"{"mcpServers":{"atlas":{"type":"stdio","command":"atlas","args":["mcp"],"env":{}}}}"#;
        assert_eq!(
            merge(CLAUDE, Some(theirs), "atlas", false).unwrap(),
            MergeOutcome::UpToDate
        );
        let t = text(merge(CLAUDE, None, "atlas", false).unwrap());
        let v: Value = serde_json::from_str(&t).unwrap();
        assert!(v.get("$schema").is_none());
        assert_eq!(v["mcpServers"]["atlas"]["env"], json!({}));
    }

    #[test]
    fn entry_with_custom_env_is_foreign() {
        assert!(!is_owned(
            &json!({"type":"stdio","command":"atlas","args":["mcp"],"env":{"A":"1"}})
        ));
        assert!(is_owned(
            &json!({"type":"stdio","command":"atlas","args":["mcp"],"env":{}})
        ));
    }

    #[test]
    fn key_order_of_the_user_file_is_preserved() {
        let existing =
            r#"{"zeta":1,"mcpServers":{"b":{"command":"b"},"a":{"command":"a"}},"alpha":2}"#;
        let t = text(merge(CLAUDE, Some(existing), "atlas", false).unwrap());
        let keys: Vec<_> = serde_json::from_str::<Map<String, Value>>(&t)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["zeta", "mcpServers", "alpha"]);
        let servers: Vec<_> = serde_json::from_str::<Value>(&t).unwrap()["mcpServers"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(servers, ["b", "a", "atlas"]);
        let RemoveOutcome::Removed(Some(back)) = remove(Some(&t), false).unwrap() else {
            panic!("expected rewritten file");
        };
        assert_eq!(
            serde_json::from_str::<Map<String, Value>>(&back)
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["zeta", "mcpServers", "alpha"]
        );
    }

    #[test]
    fn remove_refuses_foreign_and_reports_absent() {
        let foreign = r#"{"mcpServers":{"atlas":{"command":"node","args":["s.js"]}}}"#;
        assert_eq!(
            remove(Some(foreign), false).unwrap(),
            RemoveOutcome::Foreign
        );
        assert!(matches!(
            remove(Some(foreign), true).unwrap(),
            RemoveOutcome::Removed(_)
        ));
        assert_eq!(remove(None, false).unwrap(), RemoveOutcome::Absent);
        assert_eq!(remove(Some("{}"), false).unwrap(), RemoveOutcome::Absent);
    }
}
