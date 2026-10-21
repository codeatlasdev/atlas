//! Agent status, owned by `atlas hostd` (`atlas_host::agents`): these commands
//! forward to the hosts, and `host.rs` re-emits their `agent_status` pushes as
//! `agent://status {pane_key, status}`.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::host::HostHub;

/// Remember which tab runs `agent` in `cwd` (fallback for hooks without a pane key).
/// Routed by `cwd`, so a tab on an SSH host binds on that host.
#[tauri::command]
pub async fn agent_bind(
    hub: tauri::State<'_, Arc<HostHub>>,
    pane: String,
    agent: String,
    cwd: String,
) -> Result<(), String> {
    hub.call(
        "agent_bind",
        json!({"pane": pane, "agent": agent, "cwd": cwd}),
    )
    .await
    .map(|_| ())
}

/// The pane's process exited: back to Idle. The frontend knows pty → pane, not the host.
#[tauri::command]
pub async fn agent_pane_exited(
    hub: tauri::State<'_, Arc<HostHub>>,
    pane: String,
) -> Result<(), String> {
    hub.call_all("agent_pane_exited", json!({"pane": pane}))
        .await;
    Ok(())
}

/// Installed agents and whether their hooks are on, from the local hostd (the hooks report
/// to this computer): `{agents: [{id, label, setup, state}]}`.
#[tauri::command]
pub async fn agent_integrations(hub: tauri::State<'_, Arc<HostHub>>) -> Result<Value, String> {
    hub.call("agent_integrations", json!({})).await
}

/// Every host's panes, merged (pane keys are tab UUIDs, unique across hosts).
#[tauri::command]
pub async fn agent_status_all(hub: tauri::State<'_, Arc<HostHub>>) -> Result<Value, String> {
    let mut all = Map::new();
    for v in hub.call_all("agent_status_all", json!({})).await {
        if let Some(p) = v["panes"].as_object() {
            all.extend(p.clone());
        }
    }
    Ok(Value::Object(all))
}
