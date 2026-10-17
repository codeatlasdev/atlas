//! argv + env for an agent terminal. The frontend creates the pty/tab (it owns
//! tabs); the hook env lets the agent's status extension find the app.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::registry;

fn desktop_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".atlas/desktop")
}

pub fn hook_endpoint_file() -> PathBuf {
    desktop_dir().join("hook-endpoint.env")
}

/// Overlay that keeps omp from resuming an unrelated session in a fresh tab.
fn omp_fresh_config() -> Result<PathBuf, String> {
    let p = desktop_dir().join("omp-fresh.yml");
    if !p.exists() {
        crate::registry::write_atomic(&p, "autoResume: false\n")?;
    }
    Ok(p)
}

#[tauri::command]
pub async fn agent_command(
    hub: tauri::State<'_, std::sync::Arc<crate::host::HostHub>>,
    agent: String,
    tab_id: String,
    worktree_id: String,
    prompt: Option<String>,
    resume: Option<String>,
) -> Result<Value, String> {
    let def = registry::find(&agent).ok_or_else(|| format!("unknown agent {agent}"))?;
    let entry = crate::registry::load()?
        .into_iter()
        .find(|e| e.id == worktree_id);
    let remote = entry.as_ref().filter(|e| e.is_remote());
    // On an SSH host the agent comes from the user's login-shell PATH there, and
    // the hook endpoint / omp overlay live in that host's ~/.atlas/desktop.
    let (prefix, bin, endpoint, overlay) = match remote {
        Some(e) => {
            let host = e.host();
            let dir = format!("{}/.atlas/desktop", hub.home(host).await?);
            let overlay = format!("{dir}/omp-fresh.yml");
            if def.id == "omp" {
                use base64::Engine;
                hub.mkdir(host, &dir).await?;
                hub.call(
                    "fs_write",
                    json!({"host": host, "path": overlay,
                           "data": base64::engine::general_purpose::STANDARD.encode("autoResume: false\n")}),
                )
                .await?;
            }
            (
                vec![
                    "bash".to_string(),
                    "-lc".into(),
                    r#"exec "$@""#.into(),
                    "atlas-agent".into(),
                ],
                def.bin.to_string(),
                format!("{dir}/hook-endpoint.env"),
                overlay,
            )
        }
        None => (
            Vec::new(),
            registry::which(&def.bin)
                .ok_or_else(|| format!("{} is not installed (not on PATH)", def.bin))?
                .to_string_lossy()
                .into_owned(),
            hook_endpoint_file().to_string_lossy().into_owned(),
            omp_fresh_config()?.to_string_lossy().into_owned(),
        ),
    };
    // Settings → Agents: `agentPermissions` (`yolo`|`manual`), `agentArgs.<id>`.
    let s = crate::shell_env::load_settings().extra;
    let yolo = s.get("agentPermissions").and_then(Value::as_str) == Some("yolo");
    let extra = s.get("agentArgs").and_then(Value::as_object);
    let mut argv = prefix;
    argv.push(bin);
    if def.id == "omp" {
        argv.push(format!("--config={overlay}"));
    }
    argv.extend(registry::tail_args(
        &def,
        yolo,
        extra.and_then(|m| m.get(&def.id)).and_then(Value::as_str),
        prompt,
        resume,
    ));
    let token = atlas_host::paths::new_id();
    let mut env: serde_json::Map<String, Value> =
        def.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
    for (k, v) in [
        ("ATLAS_HOOK_ENDPOINT", endpoint),
        ("ATLAS_PANE_KEY", tab_id.clone()),
        ("ATLAS_TAB_ID", tab_id),
        ("ATLAS_WORKTREE_ID", worktree_id),
        ("ATLAS_LAUNCH_TOKEN", token),
        ("ATLAS_AGENT", def.id.clone()),
    ] {
        env.insert(k.into(), json!(v));
    }
    Ok(json!({"argv": argv, "env": env, "label": def.label}))
}
