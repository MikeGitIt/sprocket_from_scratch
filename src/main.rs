use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use sprocket::api::{create_router, AppState};

#[tokio::main]
async fn main() {
    // Initialize logging
    init_logging();

    info!("Starting Sprocket Workflow Execution Engine");

    // Get database URL from environment or use default
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:sprocket.db".to_string());

    info!("Using database: {}", database_url);

    // Initialize application state
    let app_state = match AppState::new(&database_url).await {
        Ok(state) => Arc::new(RwLock::new(state)),
        Err(e) => {
            error!("Failed to initialize application state: {}", e);
            std::process::exit(1);
        }
    };

    // Create router
    let app = create_router(app_state);

    // Bind to address
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Server listening on {}", addr);

    // Start server
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to address: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}

fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sprocket=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
