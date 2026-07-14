use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use atlas_core::domain::project;
use atlas_core::Result;

use crate::app::AppState;

pub async fn load(_state: &Arc<AppState>, params: Value) -> Result<Value> {
    let path_str = params["path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("path required".into()))?;

    let path = PathBuf::from(path_str);
    let config = project::load_project(&path)?;
    Ok(serde_json::to_value(&config).unwrap_or_default())
}

pub async fn services_start(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    // Stub — real implementation in a future phase
    Ok(json!({ "ok": true, "message": "services start not yet implemented" }))
}

pub async fn services_stop(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    // Stub — real implementation in a future phase
    Ok(json!({ "ok": true, "message": "services stop not yet implemented" }))
}
