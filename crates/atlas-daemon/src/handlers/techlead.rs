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

    // Check if a Tech Lead session already exists
    let existing = lm
        .list()
        .iter()
        .find(|s| s.adapter_name == "kiro" && s.id.starts_with("techlead-"))
        .map(|s| s.id.clone());

    drop(lm);

    if let Some(session_id) = existing {
        // Send message to existing Tech Lead session
        let lm = state.lifecycle_manager.lock().await;
        lm.send_prompt(&session_id, &p.message, &state.pty_manager)
            .await?;

        Ok(json!({
            "session_id": session_id,
            "action": "message_sent",
        }))
    } else {
        // Spawn new Tech Lead session
        let adapter = KiroAdapter::new();
        let config = tech_lead_launch_config(p.project_path.into());

        let mut lm = state.lifecycle_manager.lock().await;
        let session_id = lm.spawn(&adapter, config, &state.pty_manager).await?;

        // Override the session ID to be identifiable as tech lead
        // (The lifecycle manager already created it, we just track it)

        Ok(json!({
            "session_id": session_id,
            "action": "spawned",
            "message": "Tech Lead session started. Connected to Kiro CLI.",
        }))
    }
}
