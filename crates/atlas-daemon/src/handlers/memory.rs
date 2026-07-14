use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::app::AppState;

pub async fn store(state: &Arc<AppState>, params: Value) -> atlas_core::Result<Value> {
    let content = params["content"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("missing content".into()))?;
    let memory_type_str = params["memory_type"].as_str().unwrap_or("fact");
    let session_id = params["session_id"].as_str().map(String::from);
    let agent_name = params["agent_name"].as_str().map(String::from);
    let project_path = params["project_path"].as_str().map(String::from);

    let memory_type = match memory_type_str {
        "fact" => atlas_memory::MemoryType::Fact,
        "decision" => atlas_memory::MemoryType::Decision,
        "experience" => atlas_memory::MemoryType::Experience,
        "observation" => atlas_memory::MemoryType::Observation,
        "task" => atlas_memory::MemoryType::Task,
        _ => atlas_memory::MemoryType::Fact,
    };

    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();

    let memory = atlas_memory::Memory {
        id: id.clone(),
        content: content.to_string(),
        memory_type,
        source: atlas_memory::MemorySource {
            session_id,
            agent_name,
            project_path,
        },
        embedding: None,
        metadata: HashMap::new(),
        valid_from: now,
        valid_until: None,
        created_at: now,
        heat: 1.0,
        access_count: 0,
        last_accessed: now,
    };

    let mut engine = state.memory_engine.lock().await;
    let stored_id = engine.store_memory(memory)?;

    Ok(serde_json::json!({ "id": stored_id }))
}

pub async fn search(state: &Arc<AppState>, params: Value) -> atlas_core::Result<Value> {
    let text = params["text"].as_str().map(String::from);
    let limit = params["limit"].as_u64().unwrap_or(10) as usize;

    let memory_types = params["memory_types"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| match s {
                "fact" => atlas_memory::MemoryType::Fact,
                "decision" => atlas_memory::MemoryType::Decision,
                "experience" => atlas_memory::MemoryType::Experience,
                "observation" => atlas_memory::MemoryType::Observation,
                "task" => atlas_memory::MemoryType::Task,
                _ => atlas_memory::MemoryType::Fact,
            })
            .collect()
    });

    let query = atlas_memory::SearchQuery {
        text,
        embedding: None,
        memory_types,
        time_range: None,
        limit,
    };

    let engine = state.memory_engine.lock().await;
    let results = engine.search(query)?;

    let json_results: Vec<Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "content": r.memory.content,
                "memory_type": r.memory.memory_type,
                "score": r.score,
                "created_at": r.memory.created_at,
            })
        })
        .collect();

    Ok(serde_json::json!({ "results": json_results }))
}

pub async fn events(state: &Arc<AppState>, params: Value) -> atlas_core::Result<Value> {
    let since = params["since"].as_i64().unwrap_or(0);

    let engine = state.memory_engine.lock().await;
    let events = engine.get_events(since)?;

    let json_events: Vec<Value> = events
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "timestamp": e.timestamp,
                "event_type": e.event_type,
                "session_id": e.session_id,
                "agent_name": e.agent_name,
            })
        })
        .collect();

    Ok(serde_json::json!({ "events": json_events }))
}

pub async fn graph_entities(state: &Arc<AppState>, params: Value) -> atlas_core::Result<Value> {
    let entity_id = params["entity_id"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("missing entity_id".into()))?;
    let depth = params["depth"].as_u64().unwrap_or(2) as u32;

    let engine = state.memory_engine.lock().await;
    let entities = engine.get_related(entity_id, depth)?;

    let json_entities: Vec<Value> = entities
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "name": e.name,
                "entity_type": e.entity_type,
                "properties": e.properties,
            })
        })
        .collect();

    Ok(serde_json::json!({ "entities": json_entities }))
}

pub async fn graph_relate(state: &Arc<AppState>, params: Value) -> atlas_core::Result<Value> {
    let source = params["source"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("missing source".into()))?;
    let target = params["target"]
        .as_str()
        .ok_or_else(|| atlas_core::AtlasError::InvalidInput("missing target".into()))?;
    let relation_type_str = params["relation_type"].as_str().unwrap_or("related_to");

    let relation_type = match relation_type_str {
        "modified_by" => atlas_memory::RelationType::ModifiedBy,
        "depends_on" => atlas_memory::RelationType::DependsOn,
        "related_to" => atlas_memory::RelationType::RelatedTo,
        "created_for" => atlas_memory::RelationType::CreatedFor,
        "implements" => atlas_memory::RelationType::Implements,
        "references" => atlas_memory::RelationType::References,
        "conflicts_with" => atlas_memory::RelationType::ConflictsWith,
        _ => atlas_memory::RelationType::RelatedTo,
    };

    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();

    let rel = atlas_memory::Relationship {
        id: id.clone(),
        source: source.to_string(),
        target: target.to_string(),
        relation_type,
        weight: params["weight"].as_f64().unwrap_or(1.0),
        created_at: now,
    };

    let engine = state.memory_engine.lock().await;
    engine.add_relationship(rel)?;

    Ok(serde_json::json!({ "id": id }))
}
