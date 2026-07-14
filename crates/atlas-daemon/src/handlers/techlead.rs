use std::sync::Arc;

use atlas_agent::techlead::tech_lead_launch_config;
use atlas_agent_kiro::KiroAdapter;
use serde::Deserialize;
use serde_json::{json, Value};

use atlas_core::Result;

use crate::app::AppState;

#[derive(Deserialize)]
struct ChatParams {
    message: String,
    project_path: String,
}

/// The Tech Lead "chat" spawns a Kiro agent session with the Tech Lead config.
/// If a Tech Lead session already exists for this project, it sends the message
/// to that existing session (via terminal write).
pub async fn chat(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ChatParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;

    // Find any existing Tech Lead session (by adapter name containing "techlead")
    let existing = lm
        .list()
        .iter()
        .find(|s| s.adapter_name == "kiro" || s.adapter_name == "kiro-techlead")
        .map(|s| (s.id.clone(), s.terminal_session_id.clone()));

    drop(lm);

    if let Some((session_id, terminal_id)) = existing {
        // Send message to existing Tech Lead session via terminal write
        let lm = state.lifecycle_manager.lock().await;
        lm.send_prompt(&session_id, &p.message, &state.pty_manager)
            .await?;
        drop(lm);

        state.hooks.on_prompt_sent(&session_id, &p.message).await;

        Ok(json!({
            "session_id": session_id,
            "terminal_session_id": terminal_id,
            "action": "message_sent",
        }))
    } else {
        // Spawn new Tech Lead session
        let adapter = KiroAdapter::new();
        let config = tech_lead_launch_config(p.project_path.into());

        let mut lm = state.lifecycle_manager.lock().await;
        let session_id = lm.spawn(&adapter, config, &state.pty_manager).await?;

        // Get the terminal_session_id immediately after spawn
        let terminal_id = lm
            .get(&session_id)
            .and_then(|s| s.terminal_session_id.clone());
        drop(lm);

        state
            .hooks
            .on_session_started(&session_id, "kiro-techlead", &p.message)
            .await;

        Ok(json!({
            "session_id": session_id,
            "terminal_session_id": terminal_id,
            "action": "spawned",
        }))
    }
}
