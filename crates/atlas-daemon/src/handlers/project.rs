use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use atlas_core::domain::project::{self, ProjectConfig};
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

pub async fn detect(_state: &Arc<AppState>, params: Value) -> Result<Value> {
    let path_str = params["path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("path required".into()))?;

    let detection = project::detect_project(std::path::Path::new(path_str));
    serde_json::to_value(&detection)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))
}

pub async fn generate_yaml(_state: &Arc<AppState>, params: Value) -> Result<Value> {
    let path_str = params["path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("path required".into()))?;

    let config = serde_json::from_value::<ProjectConfig>(params["config"].clone())
        .map_err(|e| atlas_core::AtlasError::InvalidInput(format!("invalid config: {e}")))?;

    let yaml_content = serde_yaml::to_string(&config)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(format!("yaml serialization: {e}")))?;

    let yaml_path = std::path::Path::new(path_str).join("atlas.yaml");
    std::fs::write(&yaml_path, &yaml_content)?;

    Ok(json!({ "ok": true, "path": yaml_path.to_string_lossy() }))
}

pub async fn services_start(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    Ok(json!({ "ok": true, "message": "services start not yet implemented" }))
}

pub async fn services_stop(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    Ok(json!({ "ok": true, "message": "services stop not yet implemented" }))
}
