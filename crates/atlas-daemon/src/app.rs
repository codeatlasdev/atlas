use std::sync::Arc;

use atlas_ai::AiRouter;
use atlas_agent::LifecycleManager;
use atlas_db::repos::{SqliteConfigRepo, SqliteServerRepo, SqliteSessionRepo, SqliteTaskRepo};
use atlas_terminal::PtyManager;
use tokio::sync::Mutex;

pub struct AppState {
    pub server_repo: Arc<SqliteServerRepo>,
    pub session_repo: Arc<SqliteSessionRepo>,
    #[allow(dead_code)]
    pub config_repo: Arc<SqliteConfigRepo>,
    pub task_repo: Arc<SqliteTaskRepo>,
    pub ai_router: Arc<AiRouter>,
    pub pty_manager: Arc<PtyManager>,
    pub lifecycle_manager: Arc<Mutex<LifecycleManager>>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool) -> Arc<Self> {
        let ai_router = AiRouter::new();
        let pty_manager = PtyManager::new();
        let lifecycle_manager = LifecycleManager::new();

        Arc::new(Self {
            server_repo: Arc::new(SqliteServerRepo::new(pool.clone())),
            session_repo: Arc::new(SqliteSessionRepo::new(pool.clone())),
            config_repo: Arc::new(SqliteConfigRepo::new(pool.clone())),
            task_repo: Arc::new(SqliteTaskRepo::new(pool)),
            ai_router: Arc::new(ai_router),
            pty_manager: Arc::new(pty_manager),
            lifecycle_manager: Arc::new(Mutex::new(lifecycle_manager)),
        })
    }
}
