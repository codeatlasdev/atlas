use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use atlas_core::domain::session::{Session, SessionKind};
use atlas_core::ports::db::SessionRepository;
use atlas_core::{AtlasError, Result};

use crate::models::SessionRow;

pub struct SqliteSessionRepo {
    pool: SqlitePool,
}

impl SqliteSessionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_domain(row: SessionRow) -> Result<Session> {
        Ok(Session {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AtlasError::Database(e.to_string()))?,
            kind: row.kind.parse::<SessionKind>()?,
            server_id: row
                .server_id
                .map(|s| Uuid::parse_str(&s))
                .transpose()
                .map_err(|e| AtlasError::Database(e.to_string()))?,
            started_at: chrono::DateTime::parse_from_rfc3339(&row.started_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            ended_at: row.ended_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok()
            }),
            metadata: serde_json::from_str(&row.metadata).unwrap_or_default(),
        })
    }
}

#[async_trait]
impl SessionRepository for SqliteSessionRepo {
    async fn get_active(&self) -> Result<Vec<Session>> {
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT * FROM sessions WHERE ended_at IS NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        rows.into_iter().map(Self::row_to_domain).collect()
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Session>> {
        let row = sqlx::query_as::<_, SessionRow>("SELECT * FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        row.map(Self::row_to_domain).transpose()
    }

    async fn create(&self, session: &Session) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, kind, server_id, started_at, ended_at, metadata) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(session.id.to_string())
        .bind(session.kind.to_string())
        .bind(session.server_id.map(|id| id.to_string()))
        .bind(session.started_at.to_rfc3339())
        .bind(session.ended_at.map(|dt| dt.to_rfc3339()))
        .bind(session.metadata.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    async fn end_session(&self, id: Uuid) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE sessions SET ended_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }
}
