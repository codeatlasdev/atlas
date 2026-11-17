//! The built-in coding agents: one list for the CLI, hostd, Atlas Desktop and Atlas Mobile.
//!
//! `data/agents.json` is the only place an agent is named: id, label, binary, the `atlas setup` name that
//! wires its hooks, launch flags, and what it can do (a one-shot run for commit messages, a session
//! directory the desktop lists, whether a headless crewmate can pause for approval). Rust embeds the file
//! here; the desktop frontend imports it (`apps/desktop/src/lib/agentRegistry.ts`) and Atlas Mobile reads a
//! Dart file generated from it (`bun scripts/gen-agents-dart.ts`, checked for staleness by
//! `bun test scripts`). Adding an agent is one entry in that file.
//!
//! The desktop's `agent::registry` builds its full `Agent` type from these definitions, adding
//! custom-agent support, `which` resolution and `tail_args`.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// A built-in agent definition (one entry of `data/agents.json`).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDef {
    pub id: String,
    pub label: String,
    pub bin: String,
    /// The name `atlas setup <name>` takes: it installs the hooks that report this agent's state.
    pub setup: String,
    pub yolo_args: Vec<String>,
    /// Resume argv; `{session}` is replaced by the session id (or file, for omp).
    pub resume: Vec<String>,
    /// The flag that carries the initial prompt into the TUI, e.g. `--prompt`.
    /// `None` means the prompt is positional (appended as a bare argument).
    pub prompt_flag: Option<String>,
    /// How the CLI answers one prompt (on stdin) and exits, with no tools and no saved session: what AI
    /// commit messages run. `None`: no such invocation is verified, so the agent is not offered there.
    pub one_shot: Option<Vec<String>>,
    /// Where its session files live under `$HOME`, for the desktop's session list. `None`: not readable.
    pub sessions_dir: Option<String>,
    /// The vendor's own install line (what `atlas crew login` prints and a crew server runs).
    pub install: String,
    /// The environment variable the CLI reads its API key from (a custom provider's key).
    pub key_env: String,
    /// Whether a headless crewmate of this agent can pause a tool call for the user's approval.
    pub can_ask: bool,
}

static DEFS: LazyLock<Vec<AgentDef>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/agents.json")).expect("data/agents.json is valid")
});

/// Every built-in agent, in display order.
pub fn all() -> &'static [AgentDef] {
    &DEFS
}

/// Look up a built-in agent by id.
pub fn get(id: &str) -> Option<&'static AgentDef> {
    all().iter().find(|a| a.id == id)
}

/// The built-in whose binary is `bin` (a bare program name).
pub fn by_bin(bin: &str) -> Option<&'static AgentDef> {
    all().iter().find(|a| a.bin == bin)
}

impl AgentDef {
    /// Build argv for launching this agent: [bin, yolo_args?, prompt_args?].
    /// Used by the crew runner (atlas-host). Desktop uses prompt_argv() directly.
    pub fn argv(&self, yolo: bool, prompt: Option<&str>) -> Vec<String> {
        let mut v = vec![self.bin.clone()];
        if yolo {
            v.extend(self.yolo_args.iter().cloned());
        }
        v.extend(prompt_argv(self.prompt_flag.as_deref(), prompt));
        v
    }
}

/// The prompt portion of argv: [flag, prompt] (flagged) or [prompt] (positional).
/// Empty/whitespace-only prompt produces no args.
pub fn prompt_argv(flag: Option<&str>, prompt: Option<&str>) -> Vec<String> {
    let Some(p) = prompt.filter(|p| !p.trim().is_empty()) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if let Some(f) = flag {
        v.push(f.to_string());
    }
    v.push(p.to_string());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> serde_json::Value {
        serde_json::from_str(include_str!("../data/agents.json")).unwrap()
    }

    // Contract: Atlas Desktop (`agentRegistry.ts`) and Atlas Mobile (`agent_registry_data.dart`) read the same
    // file; what Rust serializes from the parsed list must be that file, key for key, so a field the
    // struct drops or renames can never hide from them.
    #[test]
    fn the_parsed_list_serializes_back_to_the_file_the_other_languages_read() {
        assert_eq!(serde_json::to_value(all()).unwrap(), file());
    }

    // The invariants every consumer relies on: unique slug ids (settings keys, tab ids, wire values), a
    // bare binary name (the root helper allows exactly these names), and `{session}` in resume argv.
    #[test]
    fn every_entry_is_well_formed_and_ids_and_binaries_are_unique() {
        assert!(all().len() >= 4);
        for (i, a) in all().iter().enumerate() {
            assert!(
                !a.id.is_empty()
                    && a.id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
                "{}: id is lowercase alphanumerics",
                a.id
            );
            assert!(!a.label.is_empty() && !a.setup.is_empty() && !a.install.is_empty());
            assert!(
                !a.bin.is_empty() && !a.bin.contains('/') && !a.bin.contains(char::is_whitespace),
                "{}: bin is a bare program name",
                a.id
            );
            assert!(
                a.resume.iter().any(|t| t.contains("{session}")),
                "{}: resume argv carries the session",
                a.id
            );
            assert!(
                a.key_env
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_')
            );
            if let Some(one_shot) = &a.one_shot {
                assert_eq!(
                    one_shot[0], a.bin,
                    "{}: one_shot starts with the binary",
                    a.id
                );
            }
            for b in &all()[..i] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.bin, b.bin);
                assert_ne!(a.setup, b.setup);
            }
        }
    }

    // OpenCode is a built-in everywhere (launch, hooks via `atlas setup opencode`, crew engine, sign-in,
    // install), so it is listed. What it lacks is verified, not forgotten: no one-shot invocation with tools
    // off (commit messages would hand it the diff with tools on) and no session reader.
    #[test]
    fn opencode_is_built_in_with_hooks_and_without_the_two_capabilities_it_has_not_got() {
        let o = get("opencode").expect("opencode is built in");
        assert_eq!((o.bin.as_str(), o.setup.as_str()), ("opencode", "opencode"));
        assert_eq!(o.prompt_flag.as_deref(), Some("--prompt"));
        assert!(o.one_shot.is_none() && o.sessions_dir.is_none() && !o.can_ask);
        let ids: Vec<_> = all().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["omp", "claude", "codex", "opencode"]);
    }

    #[test]
    fn lookup_by_id_and_by_binary() {
        assert_eq!(get("claude").unwrap().setup, "claude-code");
        assert_eq!(by_bin("codex").unwrap().id, "codex");
        assert!(get("nope").is_none() && by_bin("nope").is_none());
    }

    #[test]
    fn argv_adds_yolo_flags_and_the_prompt_the_way_the_agent_takes_it() {
        assert_eq!(
            get("claude").unwrap().argv(true, Some("hi")),
            ["claude", "--dangerously-skip-permissions", "hi"]
        );
        assert_eq!(
            get("opencode").unwrap().argv(false, Some("hi")),
            ["opencode", "--prompt", "hi"]
        );
        assert_eq!(get("omp").unwrap().argv(false, Some("  ")), ["omp"]);
    }
}
