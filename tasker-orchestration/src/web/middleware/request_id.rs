//! # Request ID Middleware
//!
//! Generates unique request IDs for tracing and debugging.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_creation() {
        let id = RequestId("test-request-123".to_string());
        assert_eq!(id.as_str(), "test-request-123");
    }

    #[test]
    fn test_request_id_as_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = RequestId(uuid_str.to_string());
        assert_eq!(id.as_str(), uuid_str);
    }

    #[test]
    fn test_request_id_clone() {
        let original = RequestId("original-id".to_string());
        let cloned = original.clone();
        assert_eq!(original.as_str(), cloned.as_str());
    }

    #[test]
    fn test_request_id_debug() {
        let id = RequestId("debug-test".to_string());
        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_request_id_empty_string() {
        let id = RequestId(String::new());
        assert_eq!(id.as_str(), "");
    }
}
