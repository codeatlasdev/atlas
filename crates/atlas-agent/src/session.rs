#![allow(unused)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::activity::ActivityState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub adapter_name: String,
    /// Kiro agent config name (e.g., "atlas-techlead", "default")
    pub agent_name: Option<String>,
    pub terminal_session_id: Option<String>,
    pub activity_state: ActivityState,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl AgentSession {
    pub fn new(id: String, adapter_name: String) -> Self {
        Self {
            id,
            adapter_name,
            agent_name: None,
            terminal_session_id: None,
            activity_state: ActivityState::Idle,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }

    pub fn mark_ended(&mut self) {
        self.ended_at = Some(Utc::now());
    }
}
