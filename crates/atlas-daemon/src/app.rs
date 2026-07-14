use std::sync::Arc;

use atlas_ai::AiRouter;
use atlas_db::repos::{SqliteConfigRepo, SqliteServerRepo, SqliteSessionRepo};

pub struct AppState {
    pub server_repo: Arc<SqliteServerRepo>,
    pub session_repo: Arc<SqliteSessionRepo>,
    #[allow(dead_code)]
    pub config_repo: Arc<SqliteConfigRepo>,
    pub ai_router: Arc<AiRouter>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool) -> Arc<Self> {
        let ai_router = AiRouter::new();

        Arc::new(Self {
            server_repo: Arc::new(SqliteServerRepo::new(pool.clone())),
            session_repo: Arc::new(SqliteSessionRepo::new(pool.clone())),
            config_repo: Arc::new(SqliteConfigRepo::new(pool)),
            ai_router: Arc::new(ai_router),
        })
    }
}
