//! # Web API Module
//!
//! Axum-based REST API for the Tasker orchestration system.
//! Provides HTTP endpoints for worker system integration and administrative operations.
//!
//! ## Architecture Overview
//!
//! - **Dedicated Database Pool**: Separate connection pool for web operations to prevent resource contention
//! - **JWT Authentication**: RSA public/private key authentication for worker systems
//! - **Circuit Breaker**: Web-specific circuit breaker for database operations
//! - **Operational State Integration**: Respects orchestration system operational state
//!
//! ## Core Components
//!
//! - [`routes`] - HTTP route definitions and organization
//! - [`handlers`] - Request handlers for different endpoint groups
//! - [`middleware`] - Authentication, error handling, operational state middleware
//! - [`state`] - Shared application state and database pools
//! - JWT authentication and authorization (in middleware module)
//! - Web-specific error types and responses (in handlers module)
//! - [`circuit_breaker`] - Database circuit breaker implementation

pub mod circuit_breaker;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;

use axum::Router;
use state::AppState;

/// Create the main Axum application with all routes and middleware
///
/// This is the entry point for the web API, setting up:
/// - All route definitions
/// - Middleware stack (auth, timeouts, CORS, etc.)
/// - Shared application state
///
/// # Arguments
/// * `app_state` - Shared application state including database pools and configuration
///
/// # Returns
/// * `Router` - Configured Axum router ready for serving
pub fn create_app(app_state: AppState) -> Router {
    // TAS-61: V2 uses u32 for request_timeout_ms, cast to u64 for Duration::from_millis
    let request_timeout =
        std::time::Duration::from_millis(app_state.config.request_timeout_ms as u64);

    // Public routes - never require auth (Kubernetes probes, monitoring, docs)
    let public_routes = Router::new()
        .merge(routes::health_routes())
        .merge(routes::docs_routes());

    // Protected routes - auth middleware applied
    let mut protected_routes = Router::new().nest("/v1", routes::api_v1_routes());

    if app_state.config.config_endpoint_enabled {
        protected_routes = protected_routes.merge(routes::config_routes());
    }

    let protected_routes = protected_routes.layer(axum::middleware::from_fn_with_state(
        app_state.clone(),
        middleware::auth::conditional_auth,
    ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(axum::middleware::from_fn(
            middleware::request_id::add_request_id,
        ))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(app_state)
}

/// Create a test version of the app for integration testing
///
/// Similar to `create_app` but with test-specific configurations:
/// - Disabled authentication for easier testing
/// - Test database pools
/// - Longer timeout for test operations
/// - Additional debugging middleware
#[cfg(feature = "test-utils")]
pub fn create_test_app(app_state: AppState) -> Router {
    // TAS-61: V2 uses u32 for request_timeout_ms, cast to u64 for Duration::from_millis
    // Use longer timeout for tests (configured value + 90 seconds buffer)
    let test_timeout = std::time::Duration::from_millis(app_state.config.request_timeout_ms as u64)
        + std::time::Duration::from_secs(90);

    // Public routes - never require auth (health probes, docs)
    let mut public_routes = Router::new().merge(routes::health_routes());

    #[cfg(feature = "web-api")]
    {
        public_routes = public_routes.merge(routes::docs_routes());
    }

    // Protected routes - auth middleware applied
    let mut protected_routes = Router::new().nest("/v1", routes::api_v1_routes());

    if app_state.config.config_endpoint_enabled {
        protected_routes = protected_routes.merge(routes::config_routes());
    }

    let protected_routes = protected_routes.layer(axum::middleware::from_fn_with_state(
        app_state.clone(),
        middleware::auth::conditional_auth,
    ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(axum::middleware::from_fn(
            middleware::request_id::add_request_id,
        ))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            test_timeout,
        ))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(app_state)
}
