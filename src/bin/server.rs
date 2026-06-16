//! ACE self-hosted server binary

use ace_tool::server::{api, config::ServerConfig, db};
use anyhow::Result;
use std::sync::Arc;
use tower_sessions::{cookie::{SameSite, time::Duration}, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting ACE self-hosted server");

    // Load configuration
    let config = Arc::new(ServerConfig::from_env()?);
    tracing::info!("Bind address: {}", config.bind_addr);
    tracing::info!("Database path: {:?}", config.db_path);

    // Setup database
    let pool = db::setup_database(&config.db_path).await?;
    tracing::info!("Database initialized");

    // Setup session store
    let session_store = SqliteStore::new(pool.clone());
    session_store.migrate().await?;

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Set to true in production with HTTPS
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::hours(1)));

    // Build app state
    let app_state = api::AppState {
        pool,
        config: config.clone(),
    };

    // Build router
    let app = api::build_router(app_state).layer(session_layer);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("Server listening on {}", config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
