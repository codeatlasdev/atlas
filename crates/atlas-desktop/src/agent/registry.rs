//! Agents Atlas can launch in a terminal: the built-in ones (omp first; claude/codex share the slots) plus the
//! user's custom agents (#56, `customAgents` in `~/.atlas/desktop/settings.json`, any CLI).

use std::collections::BTreeMap;

use atlas_core::agents::{AGENTS, AgentDef};
use atlas_core::sys::CommandExt as _;
use serde_json::{Value, json};

/// A launchable agent, built-in or custom.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub id: String,
    pub label: String,
    /// Program name (looked up on the login-shell PATH) or absolute path.
    pub bin: String,
    /// Always passed, before the user's extra arguments (custom agents only).
    pub args: Vec<String>,
    pub yolo_args: Vec<String>,
    pub resume: Vec<String>,
    /// Extra environment (custom agents only); the `ATLAS_*` hook variables always win.
    pub env: BTreeMap<String, String>,
    pub custom: bool,
    /// The flag that carries the initial prompt into the TUI, e.g. `--prompt`.
    /// `None` means the prompt is appended as a positional argument.
    pub prompt_flag: Option<String>,
}

impl From<&AgentDef> for Agent {
    fn from(d: &AgentDef) -> Agent {
        let owned = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
        Agent {
            id: d.id.into(),
            label: d.label.into(),
            bin: d.bin.into(),
            args: Vec::new(),
            yolo_args: owned(d.yolo_args),
            resume: owned(d.resume),
            env: BTreeMap::new(),
            custom: false,
            prompt_flag: d.prompt_flag.map(str::to_string),
        }
    }
}

/// `customAgents` of settings.json, which has no schema: an entry is kept only with a slug id (`a-z0-9-`,
/// at most 32) that no built-in or earlier entry uses, a label and a command. Anything else is dropped.
pub fn customs(settings: &serde_json::Map<String, Value>) -> Vec<Agent> {
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut out: Vec<Agent> = Vec::new();
    for c in settings
        .get("customAgents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let s = |k: &str| {
            c.get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let (id, label, bin) = (s("id"), s("label"), s("command"));
        let slug = !id.is_empty()
            && id.len() <= 32
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let taken = AGENTS.iter().any(|a| a.id == id) || out.iter().any(|a| a.id == id);
        if !slug || taken || label.is_empty() || bin.is_empty() {
            continue;
        }
        let env = c
            .get("env")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .filter(|(k, _)| !k.is_empty() && !k.starts_with("ATLAS_"))
            .collect();
        out.push(Agent {
            id,
            label,
            bin,
            args: strings(&c["args"]),
            yolo_args: Vec::new(),
            resume: strings(&c["resume"]),
            env,
            custom: true,
            prompt_flag: None,
        });
    }
    out
}

/// Built-ins first, then the user's custom agents.
pub fn all() -> Vec<Agent> {
    let s = crate::shell_env::load_settings().extra;
    AGENTS.iter().map(Agent::from).chain(customs(&s)).collect()
}

pub fn find(id: &str) -> Option<Agent> {
    all().into_iter().find(|a| a.id == id)
}

/// Agent argv after the program (and omp's overlay): yolo flags, the agent's own args, the user's extra args,
/// then the resume argv (when a session is given and the agent knows how to resume) or the prompt.
pub fn tail_args(
    a: &Agent,
    yolo: bool,
    extra: Option<&str>,
    prompt: Option<String>,
    resume: Option<String>,
) -> Vec<String> {
    let mut argv = Vec::new();
    if yolo {
        argv.extend(a.yolo_args.iter().cloned());
    }
    argv.extend(a.args.iter().cloned());
    argv.extend(
        extra
            .into_iter()
            .flat_map(str::split_whitespace)
            .map(str::to_string),
    );
    match resume.filter(|r| !r.is_empty() && !a.resume.is_empty()) {
        Some(r) => argv.extend(a.resume.iter().map(|t| t.replace("{session}", &r))),
        None => {
            argv.extend(atlas_core::agents::prompt_argv(
                a.prompt_flag.as_deref(),
                prompt.as_deref(),
            ));
        }
    }
    argv
}

/// Resolve `bin` on the user's login-shell PATH (an absolute path is used as is when it exists).
pub fn which(bin: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(bin);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    atlas_core::paths::which_in(bin, &crate::shell_env::path_var()?)
}

/// `[{id, label, installed, path, version, custom}]`; version via `<bin> --version` (3 s, built-ins only:
/// an arbitrary CLI may not know `--version`).
#[tauri::command]
pub async fn agent_list() -> Vec<Value> {
    let mut out = Vec::new();
    for a in all() {
        let bin = a.bin.clone();
        let path = tauri::async_runtime::spawn_blocking(move || which(&bin))
            .await
            .ok()
            .flatten();
        let version = match path.as_ref().filter(|_| !a.custom) {
            Some(p) => {
                let fut = tokio::process::Command::new(p)
                    .no_window()
                    .arg("--version")
                    .envs(crate::shell_env::vars_async().await)
                    .stdin(std::process::Stdio::null())
                    .output();
                tokio::time::timeout(std::time::Duration::from_secs(3), fut)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .trim()
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
            }
            None => None,
        };
        out.push(json!({
            "id": a.id,
            "label": a.label,
            "installed": path.is_some(),
            "path": path,
            "version": version,
            "custom": a.custom,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::agents::AGENTS;

    fn settings(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    // #56: settings.json is hand-editable and has no schema; a bad entry must never shadow a built-in, break
    // the list or reach the launcher with an empty command.
    #[test]
    fn custom_agents_keep_only_valid_unique_entries() {
        let s = settings(json!({"customAgents": [
            {"id": "gemini", "label": "Gemini CLI", "command": "gemini", "args": ["--x", 1],
             "resume": ["-s", "{session}"], "env": {"FOO": "1", "ATLAS_AGENT": "spoof", "N": 2}},
            {"id": "claude", "label": "Fake", "command": "evil"},
            {"id": "opencode", "label": "Built-in shadow attempt", "command": "x"},
            {"id": "gemini", "label": "Dup", "command": "x"},
            {"id": "Bad Id", "label": "x", "command": "x"},
            {"id": "nocmd", "label": "x", "command": "  "},
            {"id": "nolabel", "command": "x"},
            "garbage"
        ]}));
        let c = customs(&s);
        assert_eq!(c.len(), 1, "{c:?}");
        let a = &c[0];
        assert_eq!(
            (a.id.as_str(), a.label.as_str(), a.bin.as_str()),
            ("gemini", "Gemini CLI", "gemini")
        );
        assert_eq!(a.args, ["--x"]);
        assert_eq!(
            a.env,
            BTreeMap::from([("FOO".into(), "1".into())]),
            "ATLAS_* and non-strings dropped"
        );
        assert!(a.custom && a.yolo_args.is_empty());
        assert!(customs(&settings(json!({"customAgents": "nope"}))).is_empty());
    }

    /// Contract with the frontend: `__fixtures__/custom-agent.json` is the REAL output of the Settings form
    /// (`customAgentEntry`, pinned by `customAgents.test.ts`). Every field it writes must reach the launcher.
    #[test]
    fn the_settings_form_output_launches() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/custom-agent.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let c = customs(v.as_object().unwrap());
        assert_eq!(c.len(), 1);
        let a = &c[0];
        assert_eq!(
            (a.id.as_str(), a.label.as_str(), a.bin.as_str()),
            ("gemini-cli", "Gemini CLI", "gemini")
        );
        assert_eq!(a.env, BTreeMap::from([("FOO".into(), "1".into())]));
        assert_eq!(
            tail_args(a, false, None, None, Some("S".into())),
            ["--model", "gemini-2.5-pro", "--session", "S"]
        );
    }

    #[test]
    fn tail_args_order_resume_and_prompt() {
        let omp = Agent::from(&AGENTS[0]);
        assert_eq!(
            tail_args(&omp, true, Some(" -v  --x "), Some("hi".into()), None),
            ["--auto-approve", "-v", "--x", "hi"]
        );
        let codex = Agent::from(&AGENTS[2]);
        assert_eq!(
            tail_args(&codex, false, None, Some("hi".into()), Some("S1".into())),
            ["resume", "S1"]
        );
        assert_eq!(
            tail_args(&codex, false, None, Some("   ".into()), Some(String::new())),
            Vec::<String>::new()
        );

        let s = settings(json!({"customAgents": [
            {"id": "oc", "label": "OC", "command": "oc", "args": ["run"], "resume": ["--session={session}"]},
            {"id": "plain", "label": "P", "command": "p"}
        ]}));
        let c = customs(&s);
        // A custom agent never gets yolo flags it did not declare.
        assert_eq!(
            tail_args(&c[0], true, Some("-q"), None, Some("S2".into())),
            ["run", "-q", "--session=S2"]
        );
        // No resume template: resuming falls back to the prompt instead of passing a bare session id.
        assert_eq!(
            tail_args(&c[1], false, None, Some("go".into()), Some("S3".into())),
            ["go"]
        );
    }

    #[test]
    fn tail_args_prompt_flag() {
        // opencode uses --prompt for the initial prompt (not positional)
        let opencode = Agent::from(&AGENTS[3]);
        assert_eq!(
            tail_args(&opencode, false, None, Some("p".into()), None),
            ["--prompt", "p"]
        );
        // claude uses positional prompt
        let claude = Agent::from(&AGENTS[1]);
        assert_eq!(
            tail_args(&claude, false, None, Some("p".into()), None),
            ["p"]
        );
        // empty prompt with prompt_flag: no args emitted
        assert_eq!(
            tail_args(&opencode, false, None, Some("   ".into()), None),
            Vec::<String>::new()
        );
        // resume takes priority over prompt_flag
        assert_eq!(
            tail_args(&opencode, false, None, Some("p".into()), Some("S1".into())),
            ["--session", "S1"]
        );
    }
}
