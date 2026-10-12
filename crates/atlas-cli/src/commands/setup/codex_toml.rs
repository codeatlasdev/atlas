//! Merging `[mcp_servers.atlas]` into Codex's `config.toml`.
//!
//! Codex stores MCP servers as `[mcp_servers.<name>]` tables with `command`/`args`
//! (<https://developers.openai.com/codex/mcp#configure-with-configtoml>; `codex mcp add atlas -- atlas mcp`
//! writes exactly `command = "atlas"` + `args = ["mcp"]`, verified with codex-cli 0.158.0).
//! The file is the user's main Codex config (model, approvals, profiles, projects, comments),
//! so it is edited with `toml_edit`, which round-trips comments, ordering and whitespace.
//! A hand-rolled line scanner cannot be trusted here: valid TOML spells the same server as
//! a table, an inline table under `[mcp_servers]`, dotted keys, or with quoted keys, and
//! multi-line arrays/strings can contain lines that look like table headers.
//!
//! Ownership: the `atlas` server is atlas-owned when it is a stdio server running exactly
//! `atlas mcp` (bare `atlas` or a path ending in `/atlas`) with no `url`. Extra keys the user
//! added (`startup_timeout_sec`, `enabled`, `env`, tool filters, …) are kept on update.

use anyhow::{Result, bail};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use super::plan::{MergeOutcome, RemoveOutcome};

const SERVERS: &str = "mcp_servers";
const NAME: &str = "atlas";

fn parse(existing: Option<&str>) -> Result<(DocumentMut, bool)> {
    match existing {
        None => Ok((DocumentMut::new(), true)),
        Some(t) if t.trim().is_empty() => Ok((DocumentMut::new(), true)),
        Some(t) => Ok((t.parse::<DocumentMut>()?, false)),
    }
}

/// The `atlas` server item (table or inline table), if any.
fn server(doc: &DocumentMut) -> Result<Option<&Item>> {
    let Some(servers) = doc.get(SERVERS) else {
        return Ok(None);
    };
    if !servers.is_table_like() {
        bail!("`{SERVERS}` is not a table");
    }
    Ok(servers.as_table_like().and_then(|t| t.get(NAME)))
}

fn command_of(item: &Item) -> Option<&str> {
    item.as_table_like()?.get("command")?.as_str()
}

fn args_are_mcp(item: &Item) -> bool {
    item.as_table_like()
        .and_then(|t| t.get("args"))
        .and_then(Item::as_array)
        .is_some_and(|a| a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("mcp"))
}

fn is_owned(item: &Item) -> bool {
    let Some(t) = item.as_table_like() else {
        return false;
    };
    let cmd_ok = command_of(item).is_some_and(|c| c == "atlas" || c.ends_with("/atlas"));
    cmd_ok && args_are_mcp(item) && t.get("url").is_none()
}

/// Short human description of a foreign entry for the conflict message.
fn describe(item: &Item) -> String {
    item.to_string().trim().replace('\n', "; ")
}

pub fn merge(existing: Option<&str>, command: &str, force: bool) -> Result<MergeOutcome> {
    let (mut doc, created) = parse(existing)?;
    match server(&doc)? {
        Some(cur) if !is_owned(cur) && !force => {
            return Ok(MergeOutcome::Conflict(describe(cur)));
        }
        Some(cur) if is_owned(cur) && command_of(cur) == Some(command) => {
            return Ok(MergeOutcome::UpToDate);
        }
        _ => {}
    }

    // Make sure `[mcp_servers]` exists (implicit: no empty `[mcp_servers]` header line).
    if doc.get(SERVERS).is_none() {
        let mut t = Table::new();
        t.set_implicit(true);
        doc[SERVERS] = Item::Table(t);
    }
    let servers = doc[SERVERS]
        .as_table_like_mut()
        .expect("checked table-like above");
    let owned_in_place = servers.get(NAME).is_some_and(is_owned);
    if owned_in_place {
        // Keep the user's extra keys and formatting; only `command` can differ.
        // Replace the value, not the key, so comments attached to the key survive.
        if let Some(cmd) = servers
            .get_mut(NAME)
            .and_then(Item::as_table_like_mut)
            .and_then(|t| t.get_mut("command"))
            .and_then(Item::as_value_mut)
        {
            let decor = cmd.decor().clone();
            *cmd = toml_edit::Value::from(command);
            *cmd.decor_mut() = decor;
        }
    } else {
        // Absent, or foreign + --force: replace with the canonical table.
        let mut t = Table::new();
        t.insert("command", value(command));
        let mut args = Array::new();
        args.push("mcp");
        t.insert("args", value(args));
        // Append after everything else (toml_edit would otherwise slot the table in right
        // after its sibling server, i.e. into the middle of the user's file).
        t.set_position(Some(isize::MAX));
        servers.insert(NAME, Item::Table(t));
    }
    let text = doc.to_string();
    Ok(if created {
        MergeOutcome::Create(text)
    } else {
        MergeOutcome::Update(text)
    })
}

pub fn remove(existing: Option<&str>, force: bool) -> Result<RemoveOutcome> {
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return Ok(RemoveOutcome::Absent);
    };
    let (mut doc, _) = parse(Some(text))?;
    match server(&doc)? {
        None => return Ok(RemoveOutcome::Absent),
        Some(cur) if !is_owned(cur) && !force => return Ok(RemoveOutcome::Foreign),
        Some(_) => {}
    }
    let servers = doc[SERVERS]
        .as_table_like_mut()
        .expect("checked table-like above");
    servers.remove(NAME);
    let servers_empty = servers.is_empty();
    if servers_empty {
        doc.remove(SERVERS);
    }
    // The file is deleted only when nothing at all is left.
    Ok(RemoveOutcome::Removed(if doc.as_table().is_empty() {
        None
    } else {
        Some(doc.to_string())
    }))
}

/// True when `[mcp_servers.atlas]` exists and is atlas-owned.
pub fn has_owned(existing: Option<&str>) -> bool {
    existing
        .and_then(|t| t.parse::<DocumentMut>().ok())
        .is_some_and(|d| server(&d).ok().flatten().is_some_and(is_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::merge_text as text;

    #[test]
    fn creates_the_same_table_codex_mcp_add_writes() {
        let t = text(merge(None, "atlas", false).unwrap());
        // Exactly what `codex mcp add atlas -- atlas mcp` produced (codex-cli 0.158.0).
        assert_eq!(
            t,
            "[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\"]\n"
        );
        assert_eq!(
            merge(None, "atlas", false).unwrap(),
            MergeOutcome::Create(t.clone())
        );
        assert_eq!(
            merge(Some(&t), "atlas", false).unwrap(),
            MergeOutcome::UpToDate
        );
        // Codex's own output (no trailing newline variations) is also up to date.
        assert_eq!(
            merge(
                Some("[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\"]"),
                "atlas",
                false
            )
            .unwrap(),
            MergeOutcome::UpToDate
        );
    }

    #[test]
    fn preserves_comments_order_and_other_servers_byte_for_byte() {
        let existing = r##"# my codex config
model = "gpt-5"   # inline comment

[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]  # keep me

# profiles below
[profiles.fast]
model = "mini"
"##;
        let t = text(merge(Some(existing), "atlas", false).unwrap());
        assert!(t.starts_with(existing), "existing bytes untouched:\n{t}");
        assert!(t.contains("[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\"]"));
        // And removing restores the original exactly.
        let RemoveOutcome::Removed(Some(back)) = remove(Some(&t), false).unwrap() else {
            panic!("expected rewrite");
        };
        assert_eq!(back.trim_end(), existing.trim_end());
    }

    #[test]
    fn update_changes_only_command_and_keeps_user_tuning() {
        let existing = "[mcp_servers.atlas]\n# tuned\ncommand = \"/old/bin/atlas\"\nargs = [\"mcp\"]\nstartup_timeout_sec = 30\nenabled = true\n";
        let t = text(merge(Some(existing), "atlas", false).unwrap());
        assert_eq!(
            t,
            "[mcp_servers.atlas]\n# tuned\ncommand = \"atlas\"\nargs = [\"mcp\"]\nstartup_timeout_sec = 30\nenabled = true\n"
        );
    }

    #[test]
    fn foreign_entry_conflicts_unless_forced_in_every_toml_spelling() {
        for foreign in [
            "[mcp_servers.atlas]\ncommand = \"node\"\nargs = [\"x.js\"]\n",
            "[mcp_servers]\natlas = { command = \"node\", args = [\"x.js\"] }\n",
            "mcp_servers.atlas.command = \"node\"\nmcp_servers.atlas.args = [\"x.js\"]\n",
            "[mcp_servers.\"atlas\"]\ncommand = \"node\"\n",
            "[mcp_servers.atlas]\nurl = \"https://example.com/mcp\"\n",
            "[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\", \"--dir\", \"x\"]\n",
        ] {
            assert!(
                matches!(
                    merge(Some(foreign), "atlas", false).unwrap(),
                    MergeOutcome::Conflict(_)
                ),
                "{foreign}"
            );
            assert_eq!(
                remove(Some(foreign), false).unwrap(),
                RemoveOutcome::Foreign,
                "{foreign}"
            );
            let forced = text(merge(Some(foreign), "atlas", true).unwrap());
            let d: DocumentMut = forced.parse().unwrap();
            assert!(is_owned(server(&d).unwrap().unwrap()), "{forced}");
            assert!(
                !forced.contains("node") && !forced.contains("example.com"),
                "{forced}"
            );
        }
    }

    #[test]
    fn owned_inline_and_dotted_spellings_are_recognised_and_removable() {
        for owned in [
            "[mcp_servers]\natlas = { command = \"atlas\", args = [\"mcp\"] }\n",
            "mcp_servers.atlas.command = \"/Users/a/.atlas/bin/atlas\"\nmcp_servers.atlas.args = [\"mcp\"]\n",
        ] {
            assert!(has_owned(Some(owned)), "{owned}");
            assert_eq!(
                remove(Some(owned), false).unwrap(),
                RemoveOutcome::Removed(None),
                "{owned}"
            );
        }
        // Inline table updated in place keeps its inline shape.
        let t = text(
            merge(
                Some("[mcp_servers]\natlas = { command = \"/x/atlas\", args = [\"mcp\"] }\n"),
                "atlas",
                false,
            )
            .unwrap(),
        );
        assert!(
            t.contains("atlas = { command = \"atlas\", args = [\"mcp\"] }"),
            "{t}"
        );
    }

    #[test]
    fn multiline_arrays_that_look_like_headers_do_not_confuse_us() {
        let existing = "[mcp_servers.other]\ncommand = \"x\"\nargs = [\n[\"nested\"],\n]\n\n[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\"]\n\n[after]\nk = 1\n";
        assert_eq!(
            merge(Some(existing), "atlas", false).unwrap(),
            MergeOutcome::UpToDate
        );
        let RemoveOutcome::Removed(Some(t)) = remove(Some(existing), false).unwrap() else {
            panic!("expected rewrite");
        };
        assert!(
            t.contains("[mcp_servers.other]") && t.contains("[after]") && !t.contains("atlas"),
            "{t}"
        );
    }

    #[test]
    fn remove_deletes_file_only_when_nothing_else_is_left() {
        let only = text(merge(None, "atlas", false).unwrap());
        assert_eq!(
            remove(Some(&only), false).unwrap(),
            RemoveOutcome::Removed(None)
        );
        let with_other = format!("{only}\n[mcp_servers.keep]\ncommand = \"k\"\n");
        let RemoveOutcome::Removed(Some(t)) = remove(Some(&with_other), false).unwrap() else {
            panic!("expected rewrite");
        };
        assert!(
            t.contains("[mcp_servers.keep]") && !t.contains("atlas"),
            "{t}"
        );
        assert_eq!(remove(None, false).unwrap(), RemoveOutcome::Absent);
        assert_eq!(
            remove(Some("model = \"x\"\n"), false).unwrap(),
            RemoveOutcome::Absent
        );
        assert_eq!(
            remove(Some("[mcp_servers.other]\ncommand = \"x\"\n"), false).unwrap(),
            RemoveOutcome::Absent
        );
    }

    #[test]
    fn invalid_or_ill_shaped_toml_is_an_error_not_overwritten() {
        assert!(merge(Some("[mcp_servers.atlas"), "atlas", true).is_err());
        assert!(merge(Some("mcp_servers = 3\n"), "atlas", true).is_err());
        assert!(remove(Some("= nope"), false).is_err());
    }
}
