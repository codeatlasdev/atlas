use std::sync::Arc;

use atlas_terminal::SessionConfig;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::AppState;

type Result<T> = atlas_core::Result<T>;

#[derive(Deserialize)]
struct CreateParams {
    shell: String,
    #[serde(default)]
    args: Vec<String>,
    rows: u16,
    cols: u16,
    cwd: String,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct SessionIdParams {
    session_id: String,
}

#[derive(Deserialize)]
struct InputParams {
    session_id: String,
    data: String, // base64
}

#[derive(Deserialize)]
struct ResizeParams {
    session_id: String,
    rows: u16,
    cols: u16,
}

pub async fn create(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: CreateParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let config = SessionConfig {
        shell: p.shell,
        args: p.args,
        rows: p.rows,
        cols: p.cols,
        cwd: p.cwd.into(),
        env: p.env,
    };

    let session_id = state.pty_manager.create_session(config).await?;
    Ok(json!({ "session_id": session_id }))
}

pub async fn attach(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let (scrollback, _rx) = state.pty_manager.attach(&p.session_id).await?;
    let encoded = BASE64.encode(&scrollback);

    Ok(json!({
        "session_id": p.session_id,
        "scrollback": encoded,
    }))
}

pub async fn input(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: InputParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let data = BASE64
        .decode(&p.data)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(format!("invalid base64: {e}")))?;

    state.pty_manager.write_input(&p.session_id, &data).await?;
    Ok(json!({ "ok": true }))
}

pub async fn resize(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ResizeParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    state
        .pty_manager
        .resize(&p.session_id, p.rows, p.cols)
        .await?;
    Ok(json!({ "ok": true }))
}

pub async fn kill(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: SessionIdParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    state.pty_manager.kill_session(&p.session_id).await?;
    Ok(json!({ "ok": true }))
}

pub async fn list(state: &Arc<AppState>, _params: Value) -> Result<Value> {
    let sessions = state.pty_manager.list_sessions().await;
    let value = serde_json::to_value(&sessions)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;
    Ok(value)
}
