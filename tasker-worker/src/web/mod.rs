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

    let common_middleware = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_millis(30000),
        ))
        .layer(cors);

    // Public routes - never require auth (Kubernetes probes, metrics, docs)
    let public_routes = Router::new()
        .merge(routes::health_routes())
        .merge(routes::metrics_routes())
        .merge(routes::docs_routes());

    // Protected routes - auth middleware applied
    let mut protected_routes = Router::new().merge(routes::template_routes());

    if state.config.config_endpoint_enabled {
        protected_routes = protected_routes.merge(routes::config_routes());
    }

    let protected_routes = protected_routes.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::auth::authenticate_request,
    ));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(common_middleware)
        .with_state(state);

    info!("Worker web application created with all routes and middleware");
    app
}
