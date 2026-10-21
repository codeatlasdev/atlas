//! Whether each installed agent reports to Atlas: `atlas setup <agent> --check`, asked of
//! this binary. The one source for Atlas Desktop's prompt and Atlas Mobile's notice.

use std::time::Duration;

use atlas_core::sys::CommandExt;
use serde_json::{Value, json};

/// (id, label, `atlas setup` name): the agents Atlas has hooks for.
const AGENTS: &[(&str, &str, &str)] = &[
    ("omp", "omp", "omp"),
    ("claude", "Claude Code", "claude-code"),
    ("codex", "Codex", "codex"),
];

/// `{agents: [{id, label, setup, state: ok|missing|unknown}]}` for the agents on the login
/// `PATH`, in [`AGENTS`] order; the checks run in parallel.
pub async fn all() -> Value {
    let exe = std::env::current_exe().ok();
    let checks = AGENTS.iter().map(|&(id, label, setup)| {
        let exe = exe.clone();
        tokio::spawn(async move {
            let found = tokio::task::spawn_blocking(move || atlas_core::login_env::which(id))
                .await
                .ok()
                .flatten();
            found?;
            let state = match exe {
                Some(exe) => check(&exe, setup).await,
                None => "unknown",
            };
            Some(json!({"id": id, "label": label, "setup": setup, "state": state}))
        })
    });
    let mut out = Vec::new();
    for c in checks.collect::<Vec<_>>() {
        if let Ok(Some(v)) = c.await {
            out.push(v);
        }
    }
    json!({ "agents": out })
}

async fn check(exe: &std::path::Path, setup: &str) -> &'static str {
    let status = tokio::process::Command::new(exe)
        .args(["setup", setup, "--check"])
        .no_window()
        .envs(atlas_core::login_env::vars())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(Duration::from_secs(10), status).await {
        Ok(Ok(s)) if s.code() == Some(0) => "ok",
        Ok(Ok(s)) if s.code() == Some(1) => "missing",
        _ => "unknown",
    }
}
