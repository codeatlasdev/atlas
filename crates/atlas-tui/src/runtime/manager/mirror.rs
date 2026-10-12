//! Mirror mode: a `ServiceManager` that follows a remote runner instead of
//! supervising anything. An attached TUI keeps using the very same `App` and
//! views; only the data source differs. Service facts (note, exit code, restarts,
//! …) come from the runner's stream and are served by the usual accessors, and
//! every method that would touch the machine (status file, spawn) is a no-op.

use std::collections::HashMap;

use super::ServiceManager;
use crate::runtime::service::ServiceState;
use crate::runtime::tunnel::TunnelStatus;

/// What the runner knows about one service beyond its state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MirrorFacts {
    pub exit_code: Option<i32>,
    pub restarts: u32,
    pub note: Option<String>,
    pub completed_ok: bool,
    pub paused_by: Option<String>,
    pub deselected: bool,
}

#[derive(Debug, Default)]
pub(crate) struct MirrorState {
    pub(crate) facts: HashMap<String, MirrorFacts>,
    pub(crate) tunnel: Option<TunnelStatus>,
}

impl ServiceManager {
    /// Turn this manager into a read-only mirror of a remote runner.
    pub fn into_mirror(mut self) -> Self {
        self.mirror = Some(MirrorState::default());
        self
    }

    pub fn is_mirror(&self) -> bool {
        self.mirror.is_some()
    }

    /// Replace one service's state and facts with the runner's.
    pub fn mirror_set(&mut self, name: &str, state: ServiceState, facts: MirrorFacts) {
        if let Some(s) = self.services.get_mut(name) {
            s.status.state = state;
            s.deselected = facts.deselected;
        }
        if let Some(m) = self.mirror.as_mut() {
            m.facts.insert(name.to_string(), facts);
        }
    }

    pub fn mirror_set_tunnel(&mut self, tunnel: Option<TunnelStatus>) {
        if let Some(m) = self.mirror.as_mut() {
            m.tunnel = tunnel;
        }
    }

    /// Mirror facts for `name` (None when this is a real manager).
    pub(crate) fn mirror_facts(&self, name: &str) -> Option<&MirrorFacts> {
        self.mirror.as_ref().and_then(|m| m.facts.get(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectConfig, ServiceDef};

    fn cfg() -> ProjectConfig {
        ProjectConfig {
            name: "t".into(),
            services: HashMap::from([(
                "api".to_string(),
                ServiceDef {
                    command: Some("true".into()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn mirror_serves_remote_facts_through_the_usual_accessors() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut m = ServiceManager::new(std::env::temp_dir(), &cfg(), tx).into_mirror();
        assert!(m.is_mirror());
        m.mirror_set(
            "api",
            ServiceState::Failed,
            MirrorFacts {
                exit_code: Some(3),
                restarts: 2,
                note: Some("restarting in 4s (2/5)".into()),
                completed_ok: false,
                paused_by: None,
                deselected: true,
            },
        );
        assert_eq!(m.get_state("api"), Some(ServiceState::Failed));
        assert_eq!(m.exit_code("api"), Some(3));
        assert_eq!(m.restart_count("api"), 2);
        assert_eq!(
            m.service_note("api").as_deref(),
            Some("restarting in 4s (2/5)")
        );
        assert!(m.is_deselected("api"));
        assert!(!m.completed_ok("api"));
    }

    #[test]
    fn mirror_never_writes_the_status_file() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let m = ServiceManager::new(dir.path().to_path_buf(), &cfg(), tx).into_mirror();
        m.write_status_file();
        assert!(!dir.path().join(".logs/.tui-status.json").exists());
    }
}
