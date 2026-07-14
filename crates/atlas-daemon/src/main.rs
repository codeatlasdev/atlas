use anyhow::Result;
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

    tracing::info!(path = %config.socket_path.display(), "starting unix socket server");
    socket::serve(&config.socket_path, state).await?;

    Ok(())
}
