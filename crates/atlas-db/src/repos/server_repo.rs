use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use atlas_core::domain::server::{Server, ServerStatus};
use atlas_core::ports::db::ServerRepository;
use atlas_core::{AtlasError, Result};

use crate::models::ServerRow;

pub struct SqliteServerRepo {
    pool: SqlitePool,
}

impl SqliteServerRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_domain(row: ServerRow) -> Result<Server> {
        Ok(Server {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AtlasError::Database(e.to_string()))?,
            name: row.name,
            host: row.host,
            user: row.user,
            port: row.port as u16,
            status: row.status.parse::<ServerStatus>()?,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

#[async_trait]
impl ServerRepository for SqliteServerRepo {
    async fn get_all(&self) -> Result<Vec<Server>> {
        let rows = sqlx::query_as::<_, ServerRow>("SELECT * FROM servers")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        rows.into_iter().map(Self::row_to_domain).collect()
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Server>> {
        let row = sqlx::query_as::<_, ServerRow>("SELECT * FROM servers WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        row.map(Self::row_to_domain).transpose()
    }

    async fn create(&self, server: &Server) -> Result<()> {
        sqlx::query(
            "INSERT INTO servers (id, name, host, user, port, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(server.id.to_string())
        .bind(&server.name)
        .bind(&server.host)
        .bind(&server.user)
        .bind(server.port as i64)
        .bind(server.status.to_string())
        .bind(server.created_at.to_rfc3339())
        .bind(server.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, server: &Server) -> Result<()> {
        sqlx::query(
            "UPDATE servers SET name = ?, host = ?, user = ?, port = ?, status = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&server.name)
        .bind(&server.host)
        .bind(&server.user)
        .bind(server.port as i64)
        .bind(server.status.to_string())
        .bind(server.updated_at.to_rfc3339())
        .bind(server.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM servers WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }
}
