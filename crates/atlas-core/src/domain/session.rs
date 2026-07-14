use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub kind: SessionKind,
    pub server_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Ssh,
    Ai,
}

impl std::fmt::Display for SessionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ssh => write!(f, "ssh"),
            Self::Ai => write!(f, "ai"),
        }
    }
}

impl std::str::FromStr for SessionKind {
    type Err = crate::AtlasError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "ssh" => Ok(Self::Ssh),
            "ai" => Ok(Self::Ai),
            other => Err(crate::AtlasError::InvalidInput(format!(
                "invalid session kind: {other}"
            ))),
        }
    }
}
