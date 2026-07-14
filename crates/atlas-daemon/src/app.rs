use std::sync::Arc;

use atlas_agent::AgentHooks;
use atlas_ai::AiRouter;
use atlas_ai::providers::{claude::ClaudeProvider, openai::OpenAiProvider, ollama::OllamaProvider};
use atlas_agent::LifecycleManager;
use atlas_db::repos::{SqliteConfigRepo, SqliteServerRepo, SqliteSessionRepo, SqliteTaskRepo};
use atlas_memory::MemoryEngine;
use atlas_server::ServerManager;
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
    pub server_manager: Arc<ServerManager>,
    pub hooks: AgentHooks,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool) -> Arc<Self> {
        let mut ai_router = AiRouter::new();
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            ai_router.register("claude", Arc::new(ClaudeProvider::new(key)));
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            ai_router.register("openai", Arc::new(OpenAiProvider::new(key)));
        }
        ai_router.register("ollama", Arc::new(OllamaProvider::new(None)));

        let pty_manager = PtyManager::new();
        let lifecycle_manager = LifecycleManager::new();

        let memory_path = dirs_home().join("memory.redb");
        let memory_engine = match MemoryEngine::open(&memory_path) {
            Ok(engine) => engine,
            Err(_) => {
                // If DB is locked (another daemon running), try removing and recreating
                tracing::warn!("Memory engine locked, attempting fresh database...");
                let _ = std::fs::remove_file(&memory_path);
                MemoryEngine::open(&memory_path)
                    .expect("failed to create fresh memory engine")
            }
        };
        let memory_engine = Arc::new(Mutex::new(memory_engine));

        let hooks = AgentHooks::new(memory_engine.clone());

        Arc::new(Self {
            server_repo: Arc::new(SqliteServerRepo::new(pool.clone())),
            session_repo: Arc::new(SqliteSessionRepo::new(pool.clone())),
            config_repo: Arc::new(SqliteConfigRepo::new(pool.clone())),
            task_repo: Arc::new(SqliteTaskRepo::new(pool)),
            ai_router: Arc::new(ai_router),
            pty_manager: Arc::new(pty_manager),
            lifecycle_manager: Arc::new(Mutex::new(lifecycle_manager)),
            memory_engine,
            server_manager: Arc::new(ServerManager::new()),
            hooks,
        })
    }
}

fn dirs_home() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let atlas_dir = std::path::PathBuf::from(home).join(".atlas");
    std::fs::create_dir_all(&atlas_dir).ok();
    atlas_dir
}
