use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api::handlers::{
    get_task_status, get_workflow_results, get_workflow_status, health_check, submit_workflow,
    visualize_workflow, AppState,
};

pub fn create_router(app_state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/workflows", post(submit_workflow))
        .route("/workflows/:id", get(get_workflow_status))
        .route("/workflows/:id/results", get(get_workflow_results))
        .route("/workflows/visualize", post(visualize_workflow))
        .route("/tasks/:id", get(get_task_status))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(app_state)
}
