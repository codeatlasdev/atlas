use std::sync::Arc;

use serde_json::{json, Value};

use atlas_core::Result;

use crate::app::AppState;

pub async fn chat(_state: &Arc<AppState>, params: Value) -> Result<Value> {
    let message = params["message"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("message required".into()))?;
    let _project_path = params["project_path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("project_path required".into()))?;

    // Stub — real AI integration in a future phase.
    // Returns an acknowledgment with the message echoed back.
    Ok(json!({
        "role": "assistant",
        "content": format!("[Tech Lead stub] Received: {message}"),
    }))
}
