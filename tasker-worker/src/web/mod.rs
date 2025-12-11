//! Worker Web API Module
//!
//! Provides REST endpoints for worker observability, health checking, and status monitoring.
//! Follows the same patterns as tasker-orchestration/src/web but tailored for worker operations.

use axum::Router;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::info;

pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod state;

pub use state::{CircuitBreakerHealthProvider, WorkerWebState};

/// Create the worker web application with all routes and middleware
pub fn create_app(state: Arc<WorkerWebState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let middleware = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_millis(30000))) // 30 second timeout
        .layer(cors);

    let app = Router::new()
        .merge(routes::health_routes())
        .merge(routes::metrics_routes())
        .merge(routes::template_routes())
        .merge(routes::config_routes())
        .layer(middleware)
        .with_state(state);

    info!("Worker web application created with all routes and middleware");
    app
}
