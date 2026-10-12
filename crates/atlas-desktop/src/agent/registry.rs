//! Agents Atlas can launch in a terminal. omp first; claude/codex share the slots.

use serde_json::{Value, json};

pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub bin: &'static str,
    pub yolo_args: &'static [&'static str],
    pub resume: fn(&str) -> Vec<String>,
}

pub const AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "omp",
        label: "omp",
        bin: "omp",
        yolo_args: &["--auto-approve"],
        resume: |s| vec!["--resume".into(), s.into()],
    },
    AgentDef {
        id: "claude",
        label: "Claude Code",
        bin: "claude",
        yolo_args: &["--dangerously-skip-permissions"],
        resume: |s| vec!["--resume".into(), s.into()],
    },
    AgentDef {
        id: "codex",
        label: "Codex",
        bin: "codex",
        yolo_args: &["--dangerously-bypass-approvals-and-sandbox"],
        resume: |s| vec!["resume".into(), s.into()],
    },
];

pub fn get(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

/// Resolve `bin` on the user's login-shell PATH.
pub fn which(bin: &str) -> Option<std::path::PathBuf> {
    atlas_core::paths::which_in(bin, &crate::shell_env::path_var()?)
}

/// `[{id, label, installed, path, version}]`; version via `<bin> --version` (3 s).
#[tauri::command]
pub async fn agent_list() -> Vec<Value> {
    let mut out = Vec::new();
    for a in AGENTS {
        let path = tauri::async_runtime::spawn_blocking(move || which(a.bin))
            .await
            .ok()
            .flatten();
        let version = match &path {
            Some(p) => {
                let fut = tokio::process::Command::new(p)
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
        }));
    }
    out
}
