use atlas_memory::types::{Event, EventType, Memory, MemorySource};
use atlas_memory::MemoryEngine;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AgentHooks {
    memory: Arc<Mutex<MemoryEngine>>,
}

impl AgentHooks {
    pub fn new(memory: Arc<Mutex<MemoryEngine>>) -> Self {
        Self { memory }
    }

    /// Called when an agent session starts
    pub async fn on_session_started(&self, session_id: &str, adapter: &str, prompt: &str) {
        let mut engine = self.memory.lock().await;

        let event = Event::new(EventType::SessionStarted)
            .with_session(session_id)
            .with_agent(adapter)
            .with_data(serde_json::json!({"prompt": prompt}));
        let _ = engine.append_event(event);

        let memory = Memory::new_experience(
            format!("Agent {} started: {}", adapter, truncate(prompt, 100)),
            MemorySource::agent(session_id, adapter),
        );
        let _ = engine.store_memory(memory);
    }

    /// Called when an agent session ends
    pub async fn on_session_ended(&self, session_id: &str, adapter: &str, exit_code: i32) {
        let mut engine = self.memory.lock().await;

        let event = Event::new(EventType::SessionEnded)
            .with_session(session_id)
            .with_agent(adapter)
            .with_data(serde_json::json!({"exit_code": exit_code}));
        let _ = engine.append_event(event);

        let memory = Memory::new_experience(
            format!("Agent {} ended with exit code {}", adapter, exit_code),
            MemorySource::agent(session_id, adapter),
        );
        let _ = engine.store_memory(memory);
    }

    /// Called when a message is sent to an agent
    pub async fn on_prompt_sent(&self, session_id: &str, prompt: &str) {
        let mut engine = self.memory.lock().await;

        let event = Event::new(EventType::AgentMessage)
            .with_session(session_id)
            .with_data(serde_json::json!({"direction": "user_to_agent", "content": prompt}));
        let _ = engine.append_event(event);
    }

    /// Called when a task is assigned to an agent
    pub async fn on_task_assigned(&self, task_id: &str, agent_session: &str) {
        let mut engine = self.memory.lock().await;

        let event = Event::new(EventType::Custom("task_assigned".into()))
            .with_session(agent_session)
            .with_data(serde_json::json!({"task_id": task_id}));
        let _ = engine.append_event(event);
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
