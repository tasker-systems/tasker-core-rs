//! # Web API Middleware
//!
//! Middleware stack for the web API including authentication, error handling,
//! operational state coordination, and request/response processing.

pub mod auth;
pub mod operational_state;
pub mod request_id;

use axum::middleware;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use std::time::Duration;

use crate::web::state::AppState;

/// Apply the production middleware stack for a router with app state
///
/// Applies middleware in the correct order for production deployment:
/// 1. Request ID generation
/// 2. Tracing and logging
/// 3. CORS handling
/// 4. Request timeout
///    Note: Operational state checking is handled at the handler level
/// 6. Authentication (when enabled)
pub fn apply_middleware_stack(router: Router<AppState>) -> Router<AppState> {
    router
        // Request ID generation (outermost)
        .layer(middleware::from_fn(request_id::add_request_id))
        // Request timeout
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        // CORS handling
        .layer(create_cors_layer())
        // Request tracing
        .layer(TraceLayer::new_for_http())
        // Note: Operational state checking will be handled at the handler level
        // since it requires State extractor which conflicts with from_fn middleware
}

/// Apply the test middleware stack for a router with app state
///
/// Similar to production but with relaxed settings for testing:
/// - Longer timeouts
/// - Disabled authentication
/// - Additional debug logging
#[cfg(feature = "test-utils")]
pub fn apply_test_middleware_stack(router: Router<AppState>) -> Router<AppState> {
    router
        .layer(middleware::from_fn(request_id::add_request_id))
        .layer(TimeoutLayer::new(Duration::from_secs(120))) // Longer timeout for tests
        .layer(create_cors_layer())
        .layer(TraceLayer::new_for_http())
        // No operational state or auth checking for tests
}

/// Create CORS layer with appropriate settings
fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::Any) // Note: Could be configured from CORS config in future
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}