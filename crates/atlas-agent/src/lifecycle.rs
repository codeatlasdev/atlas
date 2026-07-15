#![allow(unused)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use atlas_core::{AtlasError, Result};
use atlas_terminal::{PtyManager, SessionConfig};
use tokio::sync::broadcast;
use tracing::info;

use crate::acp::{AcpClientHandler, AcpSpawnConfig, AcpTransport, AgentEvent};
use crate::activity::ActivityState;
use crate::adapter::{AgentAdapter, LaunchConfig, PromptDelivery};
use crate::session::AgentSession;

/// Tracks ACP transport state for a session.
pub struct AcpSessionState {
    pub transport: AcpTransport,
    pub acp_session_id: String,
}

pub struct LifecycleManager {
    sessions: HashMap<String, AgentSession>,
    /// ACP transports keyed by Atlas session ID.
    acp_sessions: HashMap<String, AcpSessionState>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            acp_sessions: HashMap::new(),
        }
    }

    /// Spawn an agent via PTY (legacy path — for agents without ACP support).
    pub async fn spawn(
        &mut self,
        adapter: &dyn AgentAdapter,
        config: LaunchConfig,
        pty_manager: &PtyManager,
    ) -> Result<String> {
        let cmd_parts = adapter.launch_command(&config);
        let (shell, args) = match cmd_parts.split_first() {
            Some((first, rest)) => (first.clone(), rest.to_vec()),
            None => return Err(AtlasError::InvalidInput("empty launch command".to_string())),
        };

        let pty_config = SessionConfig {
            shell,
            args,
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
        info!(session_id = %session_id, adapter = adapter.name(), "agent session spawned (PTY)");

        Ok(session_id)
    }

    /// Spawn an agent via ACP protocol (structured, bidirectional).
    pub async fn spawn_acp(
        &mut self,
        adapter: &dyn AgentAdapter,
        config: LaunchConfig,
        client_handler: Arc<dyn AcpClientHandler>,
    ) -> Result<String> {
        let binary = adapter
            .resolve_binary()
            .await
            .ok_or_else(|| AtlasError::NotFound(format!("{} binary not found", adapter.name())))?;

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut args = vec!["acp".to_string()];
        // Pass --agent flag if specified
        if let Some(ref agent_name) = config.agent_name {
            args.push("--agent".to_string());
            args.push(agent_name.clone());
        }
        // Pass permission flags to the ACP process
        let perm_flags = adapter.permission_flags(&config.permission);
        args.extend(perm_flags);

        // Ensure PATH includes common dev tool locations
        let mut env = config.env.clone();
        if !env.contains_key("PATH") {
            if let Ok(path) = std::env::var("PATH") {
                env.insert("PATH".to_string(), path);
            }
        }

        let spawn_config = AcpSpawnConfig {
            binary,
            args,
            cwd: config.cwd.clone(),
            env,
            atlas_session_id: session_id.clone(),
        };

        let transport = AcpTransport::spawn(spawn_config, client_handler)
            .await
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;

        // Initialize ACP connection
        transport
            .initialize("atlas", env!("CARGO_PKG_VERSION"))
            .await
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;

        // Create ACP session
        let acp_session_id = transport
            .new_session(config.cwd.to_str().unwrap_or("/tmp"))
            .await
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;

        info!(
            session_id = %session_id,
            acp_session_id = %acp_session_id,
            adapter = adapter.name(),
            "agent session spawned (ACP)"
        );

        // Send initial prompt if provided (fire-and-forget — results come as events)
        if !config.prompt.is_empty() {
            let req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": transport.next_request_id(),
                "method": "session/prompt",
                "params": {
                    "sessionId": acp_session_id,
                    "prompt": [{"type": "text", "text": config.prompt}]
                }
            });
            let mut line = serde_json::to_string(&req).unwrap_or_default();
            line.push('\n');
            let _ = transport.writer_tx_clone().send(line);
        }

        let mut session = AgentSession::new(session_id.clone(), adapter.name().to_string());
        session.agent_name = config.agent_name.clone();
        session.title = config.title.clone();
        session.activity_state = ActivityState::Active;
        self.sessions.insert(session_id.clone(), session);

        self.acp_sessions.insert(
            session_id.clone(),
            AcpSessionState {
                transport,
                acp_session_id,
            },
        );

        Ok(session_id)
    }

    /// Check if a session is running via ACP.
    pub fn is_acp_session(&self, id: &str) -> bool {
        self.acp_sessions.contains_key(id)
    }

    /// Get a reference to the ACP transport for a session.
    pub fn get_acp_transport(&self, id: &str) -> Option<&AcpTransport> {
        self.acp_sessions.get(id).map(|s| &s.transport)
    }

    /// Subscribe to AgentEvents for an ACP session.
    pub fn subscribe_events(&self, id: &str) -> Option<broadcast::Receiver<AgentEvent>> {
        self.acp_sessions.get(id).map(|s| s.transport.subscribe())
    }

    /// Get the event sender for an ACP session (for wiring into notifications).
    pub fn event_sender(&self, id: &str) -> Option<broadcast::Sender<AgentEvent>> {
        self.acp_sessions.get(id).map(|s| s.transport.event_sender())
    }

    /// Send a prompt to an ACP session.
    pub async fn send_prompt_acp(&self, id: &str, prompt: &str) -> Result<()> {
        let acp = self
            .acp_sessions
            .get(id)
            .ok_or_else(|| AtlasError::NotFound(format!("ACP session {id}")))?;

        // Fire-and-forget: send the prompt request but don't wait for turn end.
        // The response (turn end) will be processed by the reader task and
        // emitted as a TurnEnd event via the broadcast channel.
        let session_id = acp.acp_session_id.clone();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": acp.transport.next_request_id(),
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": prompt}]
            }
        });
        let mut line = serde_json::to_string(&req)
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;
        line.push('\n');
        acp.transport
            .writer_tx_clone()
            .send(line)
            .map_err(|_| AtlasError::Io(std::io::Error::other("writer closed")))?;

        Ok(())
    }

    /// Cancel the current operation in an ACP session.
    pub async fn cancel_acp(&self, id: &str) -> Result<()> {
        let acp = self
            .acp_sessions
            .get(id)
            .ok_or_else(|| AtlasError::NotFound(format!("ACP session {id}")))?;

        acp.transport
            .cancel()
            .await
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;

        Ok(())
    }

    /// Respond to a permission request in an ACP session.
    pub async fn respond_permission(
        &self,
        id: &str,
        request_id: u64,
        option_id: &str,
    ) -> Result<()> {
        let acp = self
            .acp_sessions
            .get(id)
            .ok_or_else(|| AtlasError::NotFound(format!("ACP session {id}")))?;

        acp.transport
            .respond_permission(request_id, option_id)
            .await
            .map_err(|e| AtlasError::Io(std::io::Error::other(e)))?;

        Ok(())
    }

    pub fn list(&self) -> Vec<&AgentSession> {
        self.sessions.values().collect()
    }

    pub fn get(&self, id: &str) -> Option<&AgentSession> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentSession> {
        self.sessions.get_mut(id)
    }

    /// Iterate over ACP sessions (for reaper).
    pub fn acp_sessions_iter(&self) -> impl Iterator<Item = (&String, &AcpSessionState)> {
        self.acp_sessions.iter()
    }

    /// Remove a dead ACP session (transport cleanup).
    pub fn remove_acp_session(&mut self, id: &str) {
        self.acp_sessions.remove(id);
    }

    /// Garbage-collect sessions that ended before the cutoff time.
    pub fn gc_ended_sessions(&mut self, cutoff: chrono::DateTime<chrono::Utc>) {
        self.sessions.retain(|_, s| {
            match s.ended_at {
                Some(ended) => ended > cutoff, // keep if ended recently
                None => true, // keep active sessions
            }
        });
    }

    pub async fn stop(&mut self, id: &str, pty_manager: &PtyManager) -> Result<()> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| AtlasError::NotFound(format!("agent session {id}")))?;

        // Kill ACP transport if exists
        if let Some(acp) = self.acp_sessions.remove(id) {
            acp.transport.kill().await;
        }

        // Kill PTY session if exists
        if let Some(ref terminal_id) = session.terminal_session_id {
            pty_manager.kill_session(terminal_id).await?;
        }

        session.mark_ended();
        session.activity_state = ActivityState::Exited(0);
        info!(session_id = %id, "agent session stopped");
        Ok(())
    }

    /// Send prompt via PTY (legacy path).
    pub async fn send_prompt(
        &self,
        id: &str,
        prompt: &str,
        pty_manager: &PtyManager,
    ) -> Result<()> {
        // If it's an ACP session, use ACP path
        if self.is_acp_session(id) {
            return self.send_prompt_acp(id, prompt).await;
        }

        // Otherwise, PTY path
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
