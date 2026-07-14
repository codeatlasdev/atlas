use std::sync::Arc;

use atlas_agent::techlead::tech_lead_launch_config;
use atlas_agent_kiro::KiroAdapter;
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_core::Result;

use crate::app::AppState;
use crate::handlers::acp_handler::DaemonClientHandler;

#[derive(Deserialize)]
struct ChatParams {
    message: String,
    project_path: String,
}

/// The Tech Lead "chat" spawns a Kiro agent session via ACP.
/// If a Tech Lead session already exists, it sends the prompt to that session.
/// Returns the session_id so the app can subscribe to agent.event notifications.
pub async fn chat(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ChatParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;

    // Find existing Tech Lead session (active, kiro adapter)
    let existing = lm
        .list()
        .iter()
        .find(|s| {
            s.is_active()
                && (s.adapter_name == "kiro" || s.adapter_name == "kiro-techlead")
        })
        .map(|s| s.id.clone());

    let is_acp = existing
        .as_ref()
        .map(|id| lm.is_acp_session(id))
        .unwrap_or(false);

    drop(lm);

    if let Some(session_id) = existing {
        // Send message to existing session
        let lm = state.lifecycle_manager.lock().await;
        lm.send_prompt(&session_id, &p.message, &state.pty_manager)
            .await?;
        drop(lm);

        state.hooks.on_prompt_sent(&session_id, &p.message).await;

        Ok(json!({
            "session_id": session_id,
            "protocol": if is_acp { "acp" } else { "pty" },
            "action": "message_sent",
        }))
    } else {
        // Spawn new Tech Lead session via ACP
        let adapter = KiroAdapter::new();
        let config = tech_lead_launch_config(p.project_path.clone().into());

        let client_handler = Arc::new(DaemonClientHandler::new(
            p.project_path.into(),
            Arc::clone(&state.pty_manager),
        ));

        let mut lm = state.lifecycle_manager.lock().await;
        let session_id = lm
            .spawn_acp(&adapter, config, client_handler)
            .await?;
        drop(lm);

        state
            .hooks
            .on_session_started(&session_id, "kiro-techlead", &p.message)
            .await;

        Ok(json!({
            "session_id": session_id,
            "protocol": "acp",
            "action": "spawned",
        }))
    }
}
