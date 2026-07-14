use std::sync::Arc;

use atlas_agent::LaunchConfig;
use atlas_agent_kiro::KiroAdapter;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::AppState;

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

    let adapter: Box<dyn atlas_agent::AgentAdapter> = match p.adapter.as_str() {
        "kiro" => Box::new(KiroAdapter::new()),
        other => {
            return Err(atlas_core::AtlasError::InvalidInput(format!(
                "unknown adapter: {other}"
            )));
        }
    };

    let config = LaunchConfig {
        prompt: p.prompt.clone(),
        cwd: p.cwd.into(),
        permission: parse_permission(&p.permission),
        env: p.env,
    };

    let adapter_name = p.adapter.clone();
    let prompt_text = p.prompt;

    let mut lm = state.lifecycle_manager.lock().await;
    let session_id = lm.spawn(adapter.as_ref(), config, &state.pty_manager).await?;
    drop(lm);

    state
        .hooks
        .on_session_started(&session_id, &adapter_name, &prompt_text)
        .await;

    Ok(json!({ "session_id": session_id }))
}

pub async fn list(state: &Arc<AppState>, _params: Value) -> Result<Value> {
    let lm = state.lifecycle_manager.lock().await;
    let sessions: Vec<_> = lm.list().iter().map(|s| {
        json!({
            "id": s.id,
            "adapter": s.adapter_name,
            "terminal_session_id": s.terminal_session_id,
            "activity_state": format!("{:?}", s.activity_state),
            "started_at": s.started_at.to_rfc3339(),
            "ended_at": s.ended_at.map(|t| t.to_rfc3339()),
        })
    }).collect();

    Ok(Value::Array(sessions))
}

pub async fn status(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let lm = state.lifecycle_manager.lock().await;
    let session = lm
        .get(&p.session_id)
        .ok_or_else(|| atlas_core::AtlasError::NotFound(format!("agent session {}", p.session_id)))?;

    Ok(json!({
        "id": session.id,
        "adapter": session.adapter_name,
        "terminal_session_id": session.terminal_session_id,
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
