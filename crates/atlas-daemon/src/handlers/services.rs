use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use atlas_core::ports::db::ServerRepository;
use atlas_core::Result;

use crate::app::AppState;

/// Ensure the server's SSH client is registered, return its ID string.
async fn resolve_server(state: &Arc<AppState>, params: &Value) -> Result<String> {
    let server_id = params["server_id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("server_id required".into()))?;
    let uuid = Uuid::parse_str(server_id)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let server = state
        .server_repo
        .get_by_id(uuid)
        .await?
        .ok_or_else(|| atlas_core::AtlasError::NotFound("server not found".into()))?;

    // Register SSH client if not already
    let ssh_host = format!("{}@{}", server.user, server.host);
    state
        .server_manager
        .connect(&server.id.to_string(), &ssh_host)
        .await;

    Ok(server.id.to_string())
}

/// services.list — list running services on a server via systemctl
pub async fn list(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let server_id = resolve_server(state, &params).await?;
    let services = state.server_manager.list_services(&server_id).await?;
    Ok(serde_json::to_value(services).unwrap_or_default())
}

/// services.restart — restart a systemd unit
pub async fn restart(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let server_id = resolve_server(state, &params).await?;
    let unit = params["unit"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("unit required".into()))?;

    state
        .server_manager
        .restart_service(&server_id, unit)
        .await?;

    Ok(serde_json::json!({ "ok": true, "unit": unit }))
}

/// services.stop — stop a systemd unit
pub async fn stop(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let server_id = resolve_server(state, &params).await?;
    let unit = params["unit"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("unit required".into()))?;

    state.server_manager.stop_service(&server_id, unit).await?;

    Ok(serde_json::json!({ "ok": true, "unit": unit }))
}

/// services.status — get detailed status of a service via systemctl show
pub async fn status(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let server_id = resolve_server(state, &params).await?;
    let unit = params["unit"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("unit required".into()))?;

    let status = state
        .server_manager
        .service_status(&server_id, unit)
        .await?;

    Ok(serde_json::to_value(status).unwrap_or_default())
}

/// services.logs — get recent journal logs for a service
pub async fn logs(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let server_id = resolve_server(state, &params).await?;
    let unit = params["unit"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("unit required".into()))?;
    let lines = params["lines"].as_u64().unwrap_or(50) as u32;

    let log_output = state
        .server_manager
        .service_logs(&server_id, unit, lines)
        .await?;

    Ok(serde_json::json!({ "unit": unit, "lines": lines, "output": log_output }))
}
