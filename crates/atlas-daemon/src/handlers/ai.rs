use std::sync::Arc;

use serde_json::Value;

use atlas_core::ports::ai::{AiProvider, Conversation, Message, Role};
use atlas_core::Result;

use crate::app::AppState;

pub async fn chat(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let model = params["model"]
        .as_str()
        .unwrap_or("claude-sonnet-4-20250514")
        .to_string();

    let content = params["message"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("message required".into()))?;

    let conversation = Conversation {
        messages: vec![Message {
            role: Role::User,
            content: content.to_string(),
        }],
        model,
    };

    let response = state.ai_router.chat(&conversation).await?;
    Ok(serde_json::to_value(response).unwrap_or_default())
}
