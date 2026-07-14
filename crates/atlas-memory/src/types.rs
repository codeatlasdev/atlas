use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type MemoryId = String;
pub type EntityId = String;
pub type SessionId = String;
pub type Timestamp = i64; // Unix millis

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub content: String,
    pub memory_type: MemoryType,
    pub source: MemorySource,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub created_at: Timestamp,
    pub heat: f64,
    pub access_count: u32,
    pub last_accessed: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Fact,
    Decision,
    Experience,
    Observation,
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySource {
    pub session_id: Option<SessionId>,
    pub agent_name: Option<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub timestamp: Timestamp,
    pub event_type: EventType,
    pub session_id: Option<SessionId>,
    pub agent_name: Option<String>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    SessionStarted,
    SessionEnded,
    MemoryCreated,
    MemoryUpdated,
    MemoryEvicted,
    TaskCreated,
    TaskCompleted,
    AgentMessage,
    ToolCall,
    FileModified,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub entity_type: EntityType,
    pub name: String,
    pub properties: HashMap<String, String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityType {
    File,
    Function,
    Agent,
    Task,
    Decision,
    Concept,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub source: EntityId,
    pub target: EntityId,
    pub relation_type: RelationType,
    pub weight: f64,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationType {
    ModifiedBy,
    DependsOn,
    RelatedTo,
    CreatedFor,
    Implements,
    References,
    ConflictsWith,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub time_range: Option<(Timestamp, Timestamp)>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub memory: Memory,
    pub score: f64,
    pub source: SearchSource,
}

#[derive(Debug, Clone)]
pub enum SearchSource {
    Vector,
    Graph,
    Temporal,
    Keyword,
}
