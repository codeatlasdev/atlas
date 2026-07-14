#![allow(unused)]

use std::collections::HashMap;
use std::io::Write;

use atlas_core::{AtlasError, Result};
use atlas_terminal::{PtyManager, SessionConfig};
use tracing::info;

use crate::activity::ActivityState;
use crate::adapter::{AgentAdapter, LaunchConfig};
use crate::session::AgentSession;

pub struct LifecycleManager {
    sessions: HashMap<String, AgentSession>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub async fn spawn(
        &mut self,
        adapter: &dyn AgentAdapter,
        config: LaunchConfig,
        pty_manager: &PtyManager,
    ) -> Result<String> {
        let cmd_parts = adapter.launch_command(&config);
        let shell = cmd_parts.join(" ");

        let pty_config = SessionConfig {
            shell,
            rows: 24,
            cols: 80,
            cwd: config.cwd.clone(),
            env: config.env.clone(),
        };

        let terminal_session_id = pty_manager.create_session(pty_config).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let mut session = AgentSession::new(session_id.clone(), adapter.name().to_string());
        session.terminal_session_id = Some(terminal_session_id);
        session.activity_state = ActivityState::Active;

        self.sessions.insert(session_id.clone(), session);
        info!(session_id = %session_id, adapter = adapter.name(), "agent session spawned");

        Ok(session_id)
    }

    pub fn list(&self) -> Vec<&AgentSession> {
        self.sessions.values().collect()
    }

    pub fn get(&self, id: &str) -> Option<&AgentSession> {
        self.sessions.get(id)
    }

    pub async fn stop(&mut self, id: &str, pty_manager: &PtyManager) -> Result<()> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| AtlasError::NotFound(format!("agent session {id}")))?;

        if let Some(ref terminal_id) = session.terminal_session_id {
            pty_manager.kill_session(terminal_id).await?;
        }

        session.mark_ended();
        session.activity_state = ActivityState::Exited(0);
        info!(session_id = %id, "agent session stopped");
        Ok(())
    }

    pub async fn send_prompt(
        &self,
        id: &str,
        prompt: &str,
        pty_manager: &PtyManager,
    ) -> Result<()> {
        let session = self
            .sessions
            .get(id)
            .ok_or_else(|| AtlasError::NotFound(format!("agent session {id}")))?;

        let terminal_id = session
            .terminal_session_id
            .as_ref()
            .ok_or_else(|| AtlasError::NotFound("no terminal session linked".to_string()))?;

        let mut input = prompt.as_bytes().to_vec();
        input.push(b'\n');
        pty_manager.write_input(terminal_id, &input).await?;

        Ok(())
    }

    pub fn update_activity(&mut self, id: &str, state: ActivityState) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.activity_state = state;
        }
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}
