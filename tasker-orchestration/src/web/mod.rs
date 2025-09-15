//! # Web API Module (TAS-28)
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
    // Extract timeout from configuration
    let request_timeout = std::time::Duration::from_millis(app_state.config.request_timeout_ms);

    let router = Router::new()
        // API v1 routes - versioned endpoints
        .nest("/v1", routes::api_v1_routes())
        // Health and metrics at root level (Kubernetes standard)
        .merge(routes::health_routes())
        .merge(routes::docs_routes());

    router
        // Apply production middleware stack which includes CORS, auth, timeouts, etc.
        .layer(axum::middleware::from_fn(
            middleware::request_id::add_request_id,
        ))
        .layer(tower_http::timeout::TimeoutLayer::new(request_timeout))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Apply conditional authentication middleware based on route configuration
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::auth::conditional_auth,
        ))
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
    // Use longer timeout for tests (configured value + 90 seconds buffer)
    let test_timeout = std::time::Duration::from_millis(app_state.config.request_timeout_ms)
        + std::time::Duration::from_secs(90);

    let mut router = Router::new()
        .nest("/v1", routes::api_v1_routes())
        .merge(routes::health_routes());

    // API documentation routes (conditionally included)
    #[cfg(feature = "web-api")]
    {
        router = router.merge(routes::docs_routes());
    }

    router
        // Apply test middleware stack which includes CORS with relaxed settings
        .layer(axum::middleware::from_fn(
            middleware::request_id::add_request_id,
        ))
        .layer(tower_http::timeout::TimeoutLayer::new(test_timeout))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Apply conditional authentication middleware (typically disabled in test config)
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::auth::conditional_auth,
        ))
        .with_state(app_state)
}
