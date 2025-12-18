//! Worker Web API Middleware
//!
//! Middleware stack for the worker web API including request ID generation,
//! tracing, and CORS handling. Simplified compared to orchestration since
//! workers typically don't need authentication or complex operational state.

pub mod request_id;

use axum::http::StatusCode;
use axum::middleware;
use axum::Router;
use std::{sync::Arc, time::Duration};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::web::state::WorkerWebState;

/// Request ID middleware struct for integration with axum middleware system
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct RequestIdMiddleware;

impl Default for RequestIdMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestIdMiddleware {
    /// Create a new request ID middleware instance
    pub fn new() -> Self {
        Self
    }
}

/// Apply the production middleware stack for worker router
///
/// Applies middleware in the correct order for production deployment:
/// 1. Request ID generation
/// 2. Tracing and logging  
/// 3. CORS handling
/// 4. Request timeout
pub fn apply_middleware_stack(router: Router<Arc<WorkerWebState>>) -> Router<Arc<WorkerWebState>> {
    router
        // Request ID generation (outermost)
        .layer(middleware::from_fn(request_id::add_request_id))
        // Request timeout
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        // CORS handling
        .layer(create_cors_layer())
        // Request tracing
        .layer(TraceLayer::new_for_http())
}

/// Apply the test middleware stack for worker router
///
/// Similar to production but with relaxed settings for testing:
/// - Longer timeouts
/// - Additional debug logging
#[cfg(feature = "test-utils")]
pub fn apply_test_middleware_stack(
    router: Router<Arc<WorkerWebState>>,
) -> Router<Arc<WorkerWebState>> {
    router
        .layer(middleware::from_fn(request_id::add_request_id))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(120))) // Longer timeout for tests
        .layer(create_cors_layer())
        .layer(TraceLayer::new_for_http())
}

/// Create CORS layer with appropriate settings for worker API
fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}
