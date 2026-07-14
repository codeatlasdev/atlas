use std::sync::Arc;

use serde_json::Value;

use atlas_core::Result;

use crate::app::AppState;

pub async fn list(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    // TODO: implement via ServerManager
    Ok(Value::Array(vec![]))
}

pub async fn restart(_state: &Arc<AppState>, _params: Value) -> Result<Value> {
    // TODO: implement via ServerManager
    Ok(Value::Bool(true))
}
