use std::sync::Arc;

use atlas_ai::AiRouter;
use atlas_agent::LifecycleManager;
use atlas_db::repos::{SqliteConfigRepo, SqliteServerRepo, SqliteSessionRepo, SqliteTaskRepo};
use atlas_memory::MemoryEngine;
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
    pub memory_engine: Arc<Mutex<MemoryEngine>>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool) -> Arc<Self> {
        let ai_router = AiRouter::new();
        let pty_manager = PtyManager::new();
        let lifecycle_manager = LifecycleManager::new();

        let memory_path = dirs_home().join("memory.redb");
        let memory_engine = MemoryEngine::open(&memory_path)
            .expect("failed to open memory engine");

        Arc::new(Self {
            server_repo: Arc::new(SqliteServerRepo::new(pool.clone())),
            session_repo: Arc::new(SqliteSessionRepo::new(pool.clone())),
            config_repo: Arc::new(SqliteConfigRepo::new(pool.clone())),
            task_repo: Arc::new(SqliteTaskRepo::new(pool)),
            ai_router: Arc::new(ai_router),
            pty_manager: Arc::new(pty_manager),
            lifecycle_manager: Arc::new(Mutex::new(lifecycle_manager)),
            memory_engine: Arc::new(Mutex::new(memory_engine)),
        })
    }
}

fn dirs_home() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let atlas_dir = std::path::PathBuf::from(home).join(".atlas");
    std::fs::create_dir_all(&atlas_dir).ok();
    atlas_dir
}
