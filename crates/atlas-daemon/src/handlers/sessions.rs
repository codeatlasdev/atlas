use std::sync::Arc;

use serde_json::Value;

use atlas_core::ports::db::SessionRepository;
use atlas_core::Result;

use crate::app::AppState;

pub async fn list(state: &Arc<AppState>) -> Result<Value> {
    let sessions = state.session_repo.get_active().await?;
    Ok(serde_json::to_value(sessions).unwrap_or_default())
}
