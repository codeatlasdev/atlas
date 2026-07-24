use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub status: ServerStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Online,
    Offline,
    Unreachable,
    Unknown,
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Unreachable => write!(f, "unreachable"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for ServerStatus {
    type Err = crate::AtlasError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "online" => Ok(Self::Online),
            "offline" => Ok(Self::Offline),
            "unreachable" => Ok(Self::Unreachable),
            "unknown" => Ok(Self::Unknown),
            other => Err(crate::AtlasError::InvalidInput(format!(
                "invalid server status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub unit_name: String,
    pub state: super::service::ServiceState,
    pub enabled: bool,
}
