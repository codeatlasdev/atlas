use std::sync::Arc;

use atlas_agent::{LaunchConfig, PermissionMode};
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
/// On first spawn, passes the Tech Lead steering context + MCP server config.
/// On subsequent messages, just sends the prompt to the existing session.
pub async fn chat(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ChatParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;

    // Find existing active Tech Lead session
    let existing = lm
        .list()
        .iter()
        .find(|s| s.is_active() && s.adapter_name == "kiro")
        .map(|s| s.id.clone());

    let is_acp = existing
        .as_ref()
        .map(|id| lm.is_acp_session(id))
        .unwrap_or(false);

    drop(lm);

    if let Some(session_id) = existing {
        // Existing session — just send the message
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
        // New session — spawn with Tech Lead agent config
        let adapter = KiroAdapter::new();
        let config = LaunchConfig {
            prompt: String::new(),
            cwd: p.project_path.clone().into(),
            permission: PermissionMode::Autonomous,
            env: Default::default(),
            agent_name: Some("atlas-techlead".to_string()),
        };

        let client_handler = Arc::new(DaemonClientHandler::new(
            p.project_path.clone().into(),
            Arc::clone(&state.pty_manager),
        ));

        let mut lm = state.lifecycle_manager.lock().await;
        let session_id = lm
            .spawn_acp(&adapter, config, client_handler)
            .await?;
        drop(lm);

        state
            .hooks
            .on_session_started(&session_id, "kiro", &p.message)
            .await;

        // The initial prompt is just the user's message.
        // The Tech Lead personality/steering comes from the Kiro agent config
        // (~/.kiro/agents/atlas-techlead.json → prompts/atlas-techlead.md)
        Ok(json!({
            "session_id": session_id,
            "protocol": "acp",
            "action": "spawned",
            "initial_prompt": p.message,
        }))
    }
}
