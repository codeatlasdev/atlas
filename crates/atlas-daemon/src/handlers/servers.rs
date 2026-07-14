use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use atlas_core::domain::server::{Server, ServerStatus};
use atlas_core::ports::db::ServerRepository;
use atlas_core::Result;

use crate::app::AppState;

pub async fn list(state: &Arc<AppState>) -> Result<Value> {
    let servers = state.server_repo.get_all().await?;
    Ok(serde_json::to_value(servers).unwrap_or_default())
}

pub async fn add(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("name required".into()))?;
    let host = params["host"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("host required".into()))?;
    let user = params["user"].as_str().unwrap_or("root");
    let port = params["port"].as_u64().unwrap_or(22) as u16;

    let now = chrono::Utc::now();
    let server = Server {
        id: Uuid::new_v4(),
        name: name.to_string(),
        host: host.to_string(),
        user: user.to_string(),
        port,
        status: ServerStatus::Unknown,
        created_at: now,
        updated_at: now,
    };

    state.server_repo.create(&server).await?;

    // Register SSH connection using the host (relies on ~/.ssh/config)
    let ssh_host = format!("{user}@{host}");
    state
        .server_manager
        .connect(&server.id.to_string(), &ssh_host)
        .await;

    Ok(serde_json::to_value(&server).unwrap_or_default())
}

pub async fn remove(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let id = params["id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("id required".into()))?;
    let uuid =
        Uuid::parse_str(id).map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    state.server_manager.disconnect(id).await;
    state.server_repo.delete(uuid).await?;

    Ok(Value::Bool(true))
}

pub async fn status(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let id = params["id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("id required".into()))?;
    let uuid =
        Uuid::parse_str(id).map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let mut server = state
        .server_repo
        .get_by_id(uuid)
        .await?
        .ok_or_else(|| atlas_core::AtlasError::NotFound("server not found".into()))?;

    // Ensure SSH client is registered
    ensure_connected(state, &server).await;

    // Check actual SSH connectivity
    let reachable = state.server_manager.check_status(id).await.unwrap_or(false);
    server.status = if reachable {
        ServerStatus::Online
    } else {
        ServerStatus::Unreachable
    };
    server.updated_at = chrono::Utc::now();

    state.server_repo.update(&server).await?;

    Ok(serde_json::to_value(&server).unwrap_or_default())
}

/// Ensure the server has an SSH client registered in the manager.
async fn ensure_connected(state: &Arc<AppState>, server: &Server) -> () {
    let id = server.id.to_string();
    let ssh_host = format!("{}@{}", server.user, server.host);
    state.server_manager.connect(&id, &ssh_host).await;
}
