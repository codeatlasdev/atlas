use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type MemoryId = String;
pub type EntityId = String;
pub type SessionId = String;
pub type Timestamp = i64; // Unix millis

// ─── Builder impls ─────────────────────────────────────────────────────

impl Event {
    pub fn new(event_type: EventType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            event_type,
            session_id: None,
            agent_name: None,
            data: serde_json::Value::Null,
        }
    }

    pub fn with_session(mut self, id: &str) -> Self {
        self.session_id = Some(id.to_string());
        self
    }

    pub fn with_agent(mut self, name: &str) -> Self {
        self.agent_name = Some(name.to_string());
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

impl Memory {
    pub fn new_experience(content: String, source: MemorySource) -> Self {
        Self::new_with_type(content, MemoryType::Experience, source)
    }

    pub fn new_fact(content: String, source: MemorySource) -> Self {
        Self::new_with_type(content, MemoryType::Fact, source)
    }

    pub fn new_decision(content: String, source: MemorySource) -> Self {
        Self::new_with_type(content, MemoryType::Decision, source)
    }

    fn new_with_type(content: String, memory_type: MemoryType, source: MemorySource) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            memory_type,
            source,
            embedding: None,
            metadata: HashMap::new(),
            valid_from: now,
            valid_until: None,
            created_at: now,
            heat: 1.0,
            access_count: 0,
            last_accessed: now,
        }
    }
}

impl MemorySource {
    pub fn agent(session_id: &str, name: &str) -> Self {
        Self {
            session_id: Some(session_id.to_string()),
            agent_name: Some(name.to_string()),
            project_path: None,
        }
    }

    pub fn system() -> Self {
        Self {
            session_id: None,
            agent_name: None,
            project_path: None,
        }
    }
}

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
