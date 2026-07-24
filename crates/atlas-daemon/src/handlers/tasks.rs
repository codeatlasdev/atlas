use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use atlas_core::domain::task::{Task, TaskPriority, TaskStatus};
use atlas_core::Result;

use crate::app::AppState;

pub async fn list(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let project_path = params["project_path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("project_path required".into()))?;

    let tasks = state.task_repo.list_by_project(project_path).await?;
    Ok(serde_json::to_value(tasks).unwrap_or_default())
}

pub async fn create(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let project_path = params["project_path"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("project_path required".into()))?;
    let title = params["title"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("title required".into()))?;

    let description = params["description"].as_str().unwrap_or("");
    let priority_str = params["priority"].as_str().unwrap_or("medium");
    let priority: TaskPriority = priority_str
        .parse()
        .unwrap_or(TaskPriority::Medium);
    let labels: Vec<String> = params["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let now = chrono::Utc::now();
    let task = Task {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        description: description.to_string(),
        status: TaskStatus::Todo,
        priority,
        assigned_agent: None,
        created_at: now,
        updated_at: now,
        labels,
        branch: None,
        pr_url: None,
    };

    state.task_repo.create(project_path, &task).await?;
    Ok(serde_json::to_value(&task).unwrap_or_default())
}

pub async fn update_status(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let id = params["id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("id required".into()))?;
    let status_str = params["status"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("status required".into()))?;
    let status: TaskStatus = status_str.parse()?;

    state.task_repo.update_status(id, status).await?;
    Ok(json!({ "ok": true }))
}

pub async fn assign(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let id = params["id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("id required".into()))?;
    let agent_id = params["agent_id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("agent_id required".into()))?;

    state.task_repo.assign_agent(id, agent_id).await?;
    Ok(json!({ "ok": true }))
}

pub async fn delete(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let id = params["id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("id required".into()))?;

    state.task_repo.delete(id).await?;
    Ok(json!({ "ok": true }))
}
