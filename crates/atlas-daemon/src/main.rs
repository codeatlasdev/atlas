use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

mod app;
mod handlers;
mod router;
mod socket;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("atlas=info".parse()?))
        .init();

    tracing::info!("atlas-daemon starting");

    let config = atlas_core::domain::config::AppConfig::default();
    let pool = atlas_db::create_pool(&config.db_path).await?;
    atlas_db::run_migrations(&pool).await?;

    let state = app::AppState::new(pool);

    // Spawn ACP session reaper — detects dead processes every 10s
    let reaper_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            reap_dead_sessions(&reaper_state).await;
        }
    });

    tracing::info!(path = %config.socket_path.display(), "starting unix socket server");
    socket::serve(&config.socket_path, state).await?;

    Ok(())
}

/// Check all ACP sessions and mark dead ones as ended.
async fn reap_dead_sessions(state: &std::sync::Arc<app::AppState>) {
    let mut lm = state.lifecycle_manager.lock().await;

    // Collect dead session IDs
    let mut dead_ids = Vec::new();
    for (id, acp_state) in lm.acp_sessions_iter() {
        if !acp_state.transport.is_alive().await {
            dead_ids.push(id.clone());
        }
    }

    // Mark dead sessions
    for id in &dead_ids {
        if let Some(session) = lm.get_mut(id) {
            if session.is_active() {
                tracing::info!(session_id = %id, "reaper: session process died, marking ended");
                session.activity_state = atlas_agent::ActivityState::Exited(1);
                session.mark_ended();
            }
        }
    }

    // Remove dead ACP transports
    for id in dead_ids {
        lm.remove_acp_session(&id);
    }

    // GC: remove sessions ended more than 5 minutes ago
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
    lm.gc_ended_sessions(cutoff);
}
