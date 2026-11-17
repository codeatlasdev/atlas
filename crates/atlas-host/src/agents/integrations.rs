//! Whether each installed agent reports to Atlas: `atlas setup <agent> --check`, asked of
//! this binary. The one source for Atlas Desktop's prompt and Atlas Mobile's notice.

use std::time::Duration;

use atlas_core::sys::CommandExt;
use serde_json::{Value, json};

/// `{agents: [{id, label, setup, state: ok|missing|other_version|unknown}]}` for the built-in agents
/// (`data/agents.json`, every one has `atlas setup <setup>` hooks) on the login `PATH`, in registry order;
/// the checks run in parallel.
pub async fn all() -> Value {
    let exe = std::env::current_exe().ok();
    let checks = atlas_core::agents::all().iter().map(|def| {
        let exe = exe.clone();
        tokio::spawn(async move {
            let found = tokio::task::spawn_blocking(move || atlas_core::login_env::which(&def.bin))
                .await
                .ok()
                .flatten();
            found?;
            let state = match exe {
                Some(exe) => check(&exe, &def.setup).await,
                None => "unknown",
            };
            Some(json!({"id": def.id, "label": def.label, "setup": def.setup, "state": state}))
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
    // tree-access: hostd's own binary, to install the user's agent hooks
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
        // Hooks on, written by another atlas (e.g. the installed one while Atlas Dev runs).
        Ok(Ok(s)) if s.code() == Some(2) => "other_version",
        _ => "unknown",
    }
}
