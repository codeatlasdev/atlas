use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdService {
    pub id: Uuid,
    pub server_id: Uuid,
    pub name: String,
    pub unit_name: String,
    pub state: ServiceState,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Running,
    Stopped,
    Failed,
    Restarting,
    Unknown,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Failed => write!(f, "failed"),
            Self::Restarting => write!(f, "restarting"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for ServiceState {
    type Err = crate::AtlasError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "running" | "active" => Ok(Self::Running),
            "stopped" | "inactive" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "restarting" | "activating" => Ok(Self::Restarting),
            _ => Ok(Self::Unknown),
        }
    }
}
