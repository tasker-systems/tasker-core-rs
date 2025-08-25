//! # Request ID Middleware
//!
//! Generates unique request IDs for tracing and debugging.
//! Reused from orchestration web API for consistency.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

/// Add request ID middleware
///
/// Generates a unique request ID for each HTTP request and adds it to:
/// - Response headers as `X-Request-ID`
/// - Request extensions for use by handlers
/// - Tracing context for log correlation
pub async fn add_request_id(mut request: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();

    // Add request ID to request extensions for handlers to access
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    // Add to tracing span for log correlation
    let span = tracing::Span::current();
    span.record("request_id", &request_id);

    // Process the request
    let mut response = next.run(request).await;

    // Add request ID to response headers
    response
        .headers_mut()
        .insert("x-request-id", request_id.parse().unwrap());

    response
}

/// Request ID wrapper for extension storage
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// Get the request ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}