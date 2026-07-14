use std::sync::Arc;

use atlas_agent::{AgentAdapter, LaunchConfig, PromptDelivery};
use atlas_agent_kiro::KiroAdapter;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::handlers::acp_handler::DaemonClientHandler;

type Result<T> = atlas_core::Result<T>;

#[derive(Deserialize)]
struct SpawnParams {
    adapter: String,
    prompt: String,
    cwd: String,
    #[serde(default = "default_permission")]
    permission: String,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    /// Force PTY mode even if adapter supports ACP.
    #[serde(default)]
    force_pty: bool,
}

fn default_permission() -> String {
    "autonomous".to_string()
}

#[derive(Deserialize)]
struct SessionIdParams {
    session_id: String,
}

#[derive(Deserialize)]
struct PromptParams {
    session_id: String,
    prompt: String,
}

#[derive(Deserialize)]
struct PermissionParams {
    session_id: String,
    request_id: u64,
    option_id: String,
}

fn parse_permission(s: &str) -> atlas_agent::PermissionMode {
    match s {
        "supervised" => atlas_agent::PermissionMode::Supervised,
        "readonly" => atlas_agent::PermissionMode::ReadOnly,
        _ => atlas_agent::PermissionMode::Autonomous,
    }
}

pub async fn spawn(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SpawnParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let adapter: Box<dyn AgentAdapter> = match p.adapter.as_str() {
        "kiro" => Box::new(KiroAdapter::new()),
        other => {
            return Err(atlas_core::AtlasError::InvalidInput(format!(
                "unknown adapter: {other}"
            )));
        }
    };

    let config = LaunchConfig {
        prompt: p.prompt.clone(),
        cwd: p.cwd.clone().into(),
        permission: parse_permission(&p.permission),
        env: p.env,
        agent_name: None,
    };

    let adapter_name = p.adapter.clone();
    let prompt_text = p.prompt;
    let use_acp = !p.force_pty && adapter.prompt_delivery() == PromptDelivery::Acp;

    let mut lm = state.lifecycle_manager.lock().await;

    let session_id = if use_acp {
        // Release lock BEFORE spawning — spawn_acp does network I/O
        drop(lm);

        let client_handler = Arc::new(DaemonClientHandler::new(
            p.cwd.clone().into(),
            Arc::clone(&state.pty_manager),
        ));

        // spawn_acp is called outside the lock — it does process spawn + init + session
        let mut lm = state.lifecycle_manager.lock().await;
        lm.spawn_acp(adapter.as_ref(), config, client_handler)
            .await?
    } else {
        lm.spawn(adapter.as_ref(), config, &state.pty_manager)
            .await?
    };

    let lm = state.lifecycle_manager.lock().await;
    let is_acp = lm.is_acp_session(&session_id);
    drop(lm);

    state
        .hooks
        .on_session_started(&session_id, &adapter_name, &prompt_text)
        .await;

    Ok(json!({
        "session_id": session_id,
        "protocol": if is_acp { "acp" } else { "pty" },
    }))
}

pub async fn list(state: &Arc<AppState>, _params: Value) -> Result<Value> {
    let lm = state.lifecycle_manager.lock().await;
    let sessions: Vec<_> = lm
        .list()
        .iter()
        .map(|s| {
            let is_acp = lm.is_acp_session(&s.id);
            json!({
                "id": s.id,
                "adapter": s.adapter_name,
                "terminal_session_id": s.terminal_session_id,
                "protocol": if is_acp { "acp" } else { "pty" },
                "activity_state": format!("{:?}", s.activity_state),
                "started_at": s.started_at.to_rfc3339(),
                "ended_at": s.ended_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Value::Array(sessions))
}

pub async fn status(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;
    let session = lm.get(&p.session_id).ok_or_else(|| {
        atlas_core::AtlasError::NotFound(format!("agent session {}", p.session_id))
    })?;
    let is_acp = lm.is_acp_session(&p.session_id);

    Ok(json!({
        "id": session.id,
        "adapter": session.adapter_name,
        "terminal_session_id": session.terminal_session_id,
        "protocol": if is_acp { "acp" } else { "pty" },
        "activity_state": format!("{:?}", session.activity_state),
        "started_at": session.started_at.to_rfc3339(),
        "ended_at": session.ended_at.map(|t| t.to_rfc3339()),
    }))
}

pub async fn stop(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let mut lm = state.lifecycle_manager.lock().await;
    let adapter_name = lm
        .get(&p.session_id)
        .map(|s| s.adapter_name.clone())
        .unwrap_or_default();
    lm.stop(&p.session_id, &state.pty_manager).await?;
    drop(lm);

    state
        .hooks
        .on_session_ended(&p.session_id, &adapter_name, 0)
        .await;

    Ok(json!({ "ok": true }))
}

pub async fn prompt(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: PromptParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;
    lm.send_prompt(&p.session_id, &p.prompt, &state.pty_manager)
        .await?;
    drop(lm);

    state.hooks.on_prompt_sent(&p.session_id, &p.prompt).await;

    Ok(json!({ "ok": true }))
}

/// Cancel the current ACP operation.
pub async fn cancel(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;
    lm.cancel_acp(&p.session_id).await?;

    Ok(json!({ "ok": true }))
}

/// Respond to a permission request.
pub async fn permission_respond(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: PermissionParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;
    lm.respond_permission(&p.session_id, p.request_id, &p.option_id)
        .await?;

    Ok(json!({ "ok": true }))
}
