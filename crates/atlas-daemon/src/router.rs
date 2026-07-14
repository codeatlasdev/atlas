use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::AppState;
use crate::handlers;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

pub async fn dispatch(state: &Arc<AppState>, raw: &str) -> Response {
    let req: Request = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(e) => {
            return Response {
                id: None,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("parse error: {e}"),
                }),
            };
        }
    };

    let id = req.id.clone();

    let result = match req.method.as_str() {
        "servers.list" => handlers::servers::list(state).await,
        "servers.add" => handlers::servers::add(state, req.params).await,
        "servers.remove" => handlers::servers::remove(state, req.params).await,
        "servers.status" => handlers::servers::status(state, req.params).await,
        "services.list" => handlers::services::list(state, req.params).await,
        "services.restart" => handlers::services::restart(state, req.params).await,
        "sessions.list" => handlers::sessions::list(state).await,
        "ai.chat" => handlers::ai::chat(state, req.params).await,
        "terminal.create" => handlers::terminal::create(state, req.params).await,
        "terminal.attach" => handlers::terminal::attach(state, req.params).await,
        "terminal.input" => handlers::terminal::input(state, req.params).await,
        "terminal.resize" => handlers::terminal::resize(state, req.params).await,
        "terminal.kill" => handlers::terminal::kill(state, req.params).await,
        "terminal.list" => handlers::terminal::list(state, req.params).await,
        "agent.spawn" => handlers::agents::spawn(state, req.params).await,
        "agent.list" => handlers::agents::list(state, req.params).await,
        "agent.status" => handlers::agents::status(state, req.params).await,
        "agent.stop" => handlers::agents::stop(state, req.params).await,
        "agent.prompt" => handlers::agents::prompt(state, req.params).await,
        _ => Err(atlas_core::AtlasError::InvalidInput(format!(
            "unknown method: {}",
            req.method
        ))),
    };

    match result {
        Ok(value) => Response {
            id,
            result: Some(value),
            error: None,
        },
        Err(e) => Response {
            id,
            result: None,
            error: Some(RpcError {
                code: -32000,
                message: e.to_string(),
            }),
        },
    }
}
