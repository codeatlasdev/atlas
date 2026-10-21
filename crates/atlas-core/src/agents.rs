//! Built-in agent definitions: id, label, binary, yolo flags, resume argv and prompt flag.
//!
//! These are the agents Atlas knows how to launch and integrate. The desktop's
//! `agent::registry` builds its full `Agent` type from these definitions, adding
//! custom-agent support, `which` resolution and `tail_args`.

/// A built-in agent definition.
pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub bin: &'static str,
    pub yolo_args: &'static [&'static str],
    /// Resume argv; `{session}` is replaced by the session id (or file, for omp).
    pub resume: &'static [&'static str],
    /// The flag that carries the initial prompt into the TUI, e.g. `--prompt`.
    /// `None` means the prompt is positional (appended as a bare argument).
    pub prompt_flag: Option<&'static str>,
}

pub const AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "omp",
        label: "omp",
        bin: "omp",
        yolo_args: &["--auto-approve"],
        resume: &["--resume", "{session}"],
        prompt_flag: None,
    },
    AgentDef {
        id: "claude",
        label: "Claude Code",
        bin: "claude",
        yolo_args: &["--dangerously-skip-permissions"],
        resume: &["--resume", "{session}"],
        prompt_flag: None,
    },
    AgentDef {
        id: "codex",
        label: "Codex",
        bin: "codex",
        yolo_args: &["--dangerously-bypass-approvals-and-sandbox"],
        resume: &["resume", "{session}"],
        prompt_flag: None,
    },
    AgentDef {
        id: "opencode",
        label: "OpenCode",
        bin: "opencode",
        yolo_args: &["--auto"],
        resume: &["--session", "{session}"],
        prompt_flag: Some("--prompt"),
    },
];

/// Look up a built-in agent by id.
pub fn get(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}
