use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Failed,
    Disabled,
    /// Intentionally paused by an atlas action (e.g. iOS/Android dev).
    /// Different from Stopped (error) — health checks skip it, no critical alert.
    Suspended,
    /// The process is alive but stopped answering its liveness probe
    /// (`failure_threshold` misses in a row after it was healthy). Distinct from
    /// `Failed`: nothing exited, the service is hung.
    Unhealthy,
    /// Liveness passes but the service is not usable: its `ready` probe fails, or
    /// the same error keeps repeating in its log.
    Degraded,
}

impl ServiceState {
    /// Lowercase name: the form of the status file, the runner wire and MCP.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
            Self::Suspended => "suspended",
            Self::Unhealthy => "unhealthy",
            Self::Degraded => "degraded",
        }
    }

    /// Up, so dependents may use it: `Running`, or `Degraded` (it answers, just not well).
    pub fn is_up(&self) -> bool {
        matches!(self, Self::Running | Self::Degraded)
    }
}

impl std::str::FromStr for ServiceState {
    type Err = ();

    /// Inverse of the lowercase serialization used in the status file.
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "stopped" => Self::Stopped,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "failed" => Self::Failed,
            "disabled" => Self::Disabled,
            "suspended" => Self::Suspended,
            "unhealthy" => Self::Unhealthy,
            "degraded" => Self::Degraded,
            _ => return Err(()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub state: ServiceState,
    pub port: Option<u16>,
    pub health_url: Option<String>,
    pub pid: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_state_transitions() {
        let mut status = ServiceStatus {
            name: "api".to_string(),
            state: ServiceState::Stopped,
            port: Some(3000),
            health_url: Some("http://localhost:3000/health".to_string()),
            pid: None,
        };

        assert_eq!(status.state, ServiceState::Stopped);

        status.state = ServiceState::Starting;
        assert_eq!(status.state, ServiceState::Starting);

        status.state = ServiceState::Running;
        status.pid = Some(1234);
        assert_eq!(status.state, ServiceState::Running);
        assert_eq!(status.pid, Some(1234));

        status.state = ServiceState::Failed;
        assert_eq!(status.state, ServiceState::Failed);

        status.state = ServiceState::Stopped;
        status.pid = None;
        assert_eq!(status.state, ServiceState::Stopped);
        assert_eq!(status.pid, None);
    }
}
