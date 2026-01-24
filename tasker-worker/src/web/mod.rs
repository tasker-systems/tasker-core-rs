//! Worker Web API Module
//!
//! Provides REST endpoints for worker observability, health checking, and status monitoring.
//! Follows the same patterns as tasker-orchestration/src/web but tailored for worker operations.

use axum::http::StatusCode;
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
#[cfg(feature = "web-api")]
pub mod openapi;
pub mod routes;
pub mod state;

pub use state::{CircuitBreakerHealthProvider, WorkerWebState};

/// Create the worker web application with all routes and middleware
pub fn create_app(state: Arc<WorkerWebState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_millis(30000),
        ))
        .layer(cors);

    let app = Router::new()
        .merge(routes::health_routes())
        .merge(routes::metrics_routes())
        .merge(routes::template_routes())
        .merge(routes::config_routes())
        .merge(routes::docs_routes())
        .layer(middleware_stack)
        // TAS-150: Authentication middleware
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::authenticate_request,
        ))
        .with_state(state);

    info!("Worker web application created with all routes and middleware");
    app
}
