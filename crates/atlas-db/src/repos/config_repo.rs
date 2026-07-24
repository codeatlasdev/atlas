use async_trait::async_trait;
use sqlx::SqlitePool;

use atlas_core::ports::db::ConfigRepository;
use atlas_core::{AtlasError, Result};

use crate::models::ConfigRow;

pub struct SqliteConfigRepo {
    pool: SqlitePool,
}

impl SqliteConfigRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConfigRepository for SqliteConfigRepo {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, ConfigRow>("SELECT * FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(row.map(|r| r.value))
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO config (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
        )
        .bind(key)
        .bind(value)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM config WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }
}
