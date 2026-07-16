//! Project Blackboard — shared memory for inter-agent communication.
//!
//! Agents write findings, questions, decisions, and progress to the blackboard.
//! Other agents read entries to coordinate without direct messaging.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app::AppState;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardEntry {
    pub id: String,
    pub project_path: String,
    pub author: String,
    pub entry_type: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// In-memory blackboard storage, keyed by project path.
pub struct Blackboard {
    entries: Vec<BlackboardEntry>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn write(&mut self, entry: BlackboardEntry) -> String {
        let id = entry.id.clone();
        self.entries.push(entry);
        id
    }

    pub fn read(
        &self,
        project_path: &str,
        entry_type: Option<&str>,
        tags: Option<&[String]>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Vec<&BlackboardEntry> {
        self.entries
            .iter()
            .filter(|e| e.project_path == project_path)
            .filter(|e| entry_type.map_or(true, |t| e.entry_type == t))
            .filter(|e| {
                tags.map_or(true, |t| t.iter().any(|tag| e.tags.contains(tag)))
            })
            .filter(|e| since.map_or(true, |s| e.created_at >= s))
            .rev() // newest first
            .take(limit)
            .collect()
    }

    pub fn entry_count(&self, project_path: &str) -> usize {
        self.entries.iter().filter(|e| e.project_path == project_path).count()
    }
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

// ─── RPC Handlers ───────────────────────────────────────────────────────────

type Result<T> = atlas_core::Result<T>;

#[derive(Deserialize)]
struct WriteParams {
    project_path: String,
    author: String,
    #[serde(rename = "type")]
    entry_type: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct ReadParams {
    project_path: String,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    /// ISO 8601 timestamp — only entries after this time
    since: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

pub async fn write_entry(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: WriteParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let entry = BlackboardEntry {
        id: Uuid::new_v4().to_string(),
        project_path: p.project_path,
        author: p.author,
        entry_type: p.entry_type,
        content: p.content,
        tags: p.tags,
        created_at: Utc::now(),
    };

    let id = {
        let mut bb = state.blackboard.lock().await;
        bb.write(entry)
    };

    Ok(json!({ "id": id, "ok": true }))
}

pub async fn read_entries(state: &Arc<AppState>, params: Value) -> Result<Value> {
    let p: ReadParams = serde_json::from_value(params)
        .map_err(|e| atlas_core::AtlasError::InvalidInput(e.to_string()))?;

    let since = p.since.and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let bb = state.blackboard.lock().await;
    let entries = bb.read(
        &p.project_path,
        p.entry_type.as_deref(),
        p.tags.as_deref(),
        since,
        p.limit,
    );

    let result: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "author": e.author,
                "type": e.entry_type,
                "content": e.content,
                "tags": e.tags,
                "created_at": e.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({ "entries": result, "count": result.len() }))
}
