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

pub async fn chat(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ChatParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    // Find existing Tech Lead session (by agent_name, NOT adapter_name)
    let existing = {
        let lm = state.lifecycle_manager.lock().await;
        lm.list()
            .iter()
            .find(|s| {
                s.is_active()
                    && s.agent_name.as_deref() == Some("atlas-techlead")
            })
            .map(|s| (s.id.clone(), lm.is_acp_session(&s.id)))
    };

    if let Some((session_id, is_acp)) = existing {
        // Existing Tech Lead session — send message
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
        // Spawn new Tech Lead session — prompt empty (sent after subscribe)
        let adapter = KiroAdapter::new();
        let project_path = p.project_path.clone();
        let config = LaunchConfig {
            prompt: String::new(), // Empty — Swift sends after subscribing
            cwd: p.project_path.into(),
            permission: PermissionMode::Autonomous,
            env: Default::default(),
            agent_name: Some("atlas-techlead".to_string()),
            title: Some("Tech Lead".to_string()),
        };

        let client_handler = Arc::new(DaemonClientHandler::new(
            project_path.clone().into(),
            Arc::clone(&state.pty_manager),
        ));

        // Drop lock before I/O-heavy spawn
        let mut lm = state.lifecycle_manager.lock().await;
        let session_id = lm
            .spawn_acp(&adapter, config, client_handler)
            .await?;
        drop(lm);

        state
            .hooks
            .on_session_started(&session_id, "kiro", &p.message)
            .await;

        Ok(json!({
            "session_id": session_id,
            "protocol": "acp",
            "action": "spawned",
            "pending_prompt": format!(
                "Projeto: {}\n\n{}",
                project_path, p.message
            ),
        }))
    }
}
