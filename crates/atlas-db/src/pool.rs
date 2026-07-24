use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use atlas_core::AtlasError;

pub async fn create_pool(db_path: &Path) -> atlas_core::Result<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AtlasError::Database(format!("failed to create db directory: {e}")))?;
    }

    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let options = SqliteConnectOptions::from_str(&db_url)
        .map_err(|e| AtlasError::Database(e.to_string()))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> atlas_core::Result<()> {
    let migration_001 = include_str!("../migrations/001_initial.sql");
    let migration_002 = include_str!("../migrations/002_kanban.sql");

    sqlx::raw_sql(migration_001)
        .execute(pool)
        .await
        .map_err(|e| AtlasError::Database(format!("migration 001 failed: {e}")))?;

    sqlx::raw_sql(migration_002)
        .execute(pool)
        .await
        .map_err(|e| AtlasError::Database(format!("migration 002 failed: {e}")))?;

    tracing::info!("database migrations applied successfully");
    Ok(())
}
