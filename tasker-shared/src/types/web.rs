//! # Web API Error Types
//!
//! Defines error types specific to the web API and their HTTP response conversions.
//! Leverages thiserror for structured error handling and Axum's IntoResponse for HTTP conversion.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Auth failure context for gateway signaling.
///
/// Included in 401/403 response headers so upstream gateways (ALB, NLB, API Gateway)
/// can make informed rate-limiting decisions based on failure severity.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct AuthFailureContext {
    /// Machine-readable failure category
    pub reason: AuthFailureReason,
    /// Suggested severity for gateway action
    pub severity: AuthFailureSeverity,
}

/// Machine-readable auth failure reasons.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub enum AuthFailureReason {
    /// No credentials provided
    Missing,
    /// Token has expired
    Expired,
    /// Credentials are invalid (wrong key, bad signature)
    Invalid,
    /// Header contains non-UTF-8 bytes
    Malformed,
    /// Valid auth but insufficient permissions
    Forbidden,
}

/// Severity levels for auth failures (signals gateway rate-limiting behavior).
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub enum AuthFailureSeverity {
    /// Likely misconfiguration, not an attack (e.g., missing credentials, expired token)
    Low,
    /// Valid auth but wrong permissions
    Medium,
    /// Possible brute-force or fuzzing attempt (e.g., invalid credentials, malformed headers)
    High,
}

impl AuthFailureReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
            Self::Malformed => "malformed",
            Self::Forbidden => "forbidden",
        }
    }
}

impl AuthFailureSeverity {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::Low => None,
            Self::Medium => Some(5),
            Self::High => Some(60),
        }
    }
}

/// Web API specific errors with HTTP status code mappings
#[derive(Error, Debug)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub enum ApiError {
    #[error("Resource not found: {message}")]
    NotFound { message: String },

    #[error("Access denied")]
    Forbidden,

    #[error("Authentication required")]
    Unauthorized,

    #[error("Invalid request: {message}")]
    BadRequest { message: String },

    /// TAS-154: Conflict - resource already exists (e.g., duplicate identity hash)
    #[error("Conflict: {message}")]
    Conflict { message: String },

    #[error("Service temporarily unavailable")]
    ServiceUnavailable,

    #[error("Request timeout")]
    Timeout,

    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,

    /// TAS-75: System backpressure active - includes Retry-After header
    #[error("Service under backpressure: {reason}")]
    Backpressure {
        reason: String,
        retry_after_seconds: u64,
    },

    #[error("Database operation failed: {operation}")]
    DatabaseError { operation: String },

    #[error("JWT authentication failed: {reason}")]
    AuthenticationError {
        reason: String,
        /// Machine-readable failure context for gateway signaling headers
        failure_context: Option<AuthFailureContext>,
    },

    #[error("Authorization failed: {reason}")]
    AuthorizationError {
        reason: String,
        /// Machine-readable failure context for gateway signaling headers
        failure_context: Option<AuthFailureContext>,
    },

    #[error("Invalid UUID format: {uuid}")]
    InvalidUuid { uuid: String },

    #[error("JSON serialization/deserialization error")]
    JsonError,

    #[error("Internal server error")]
    Internal,
}

impl ApiError {
    /// Create a NotFound error with a custom message
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    /// Create a BadRequest error with a custom message
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    /// TAS-154: Create a Conflict error (409) for duplicate resources
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    /// Create a DatabaseError with operation context
    pub fn database_error(operation: impl Into<String>) -> Self {
        Self::DatabaseError {
            operation: operation.into(),
        }
    }

    /// Create an AuthenticationError with reason (no gateway signaling context).
    pub fn auth_error(reason: impl Into<String>) -> Self {
        Self::AuthenticationError {
            reason: reason.into(),
            failure_context: None,
        }
    }

    /// Create an AuthenticationError with gateway signaling context.
    ///
    /// Includes response headers that upstream gateways can use for rate-limiting.
    pub fn auth_error_with_context(
        reason: impl Into<String>,
        failure_reason: AuthFailureReason,
        severity: AuthFailureSeverity,
    ) -> Self {
        Self::AuthenticationError {
            reason: reason.into(),
            failure_context: Some(AuthFailureContext {
                reason: failure_reason,
                severity,
            }),
        }
    }

    /// Create an AuthorizationError with reason (no gateway signaling context).
    pub fn authorization_error(reason: impl Into<String>) -> Self {
        Self::AuthorizationError {
            reason: reason.into(),
            failure_context: None,
        }
    }

    /// Create an AuthorizationError with gateway signaling context.
    pub fn authorization_error_with_context(
        reason: impl Into<String>,
        severity: AuthFailureSeverity,
    ) -> Self {
        Self::AuthorizationError {
            reason: reason.into(),
            failure_context: Some(AuthFailureContext {
                reason: AuthFailureReason::Forbidden,
                severity,
            }),
        }
    }

    /// Create an InvalidUuid error
    pub fn invalid_uuid(uuid: impl Into<String>) -> Self {
        Self::InvalidUuid { uuid: uuid.into() }
    }

    pub fn internal_server_error(_message: impl Into<String>) -> Self {
        Self::Internal
    }

    /// TAS-75: Create a backpressure error with Retry-After header support
    ///
    /// # Arguments
    /// * `reason` - Human-readable reason for backpressure (e.g., "command_channel_saturated")
    /// * `retry_after_seconds` - Suggested wait time before retry
    pub fn backpressure(reason: impl Into<String>, retry_after_seconds: u64) -> Self {
        Self::Backpressure {
            reason: reason.into(),
            retry_after_seconds,
        }
    }

    /// TAS-75: Create a backpressure error for channel saturation
    pub fn channel_saturated(channel_name: &str, saturation_percent: f64) -> Self {
        // Calculate retry-after based on saturation level
        // Higher saturation = longer wait time
        let retry_after = if saturation_percent >= 95.0 {
            30 // Critical: wait 30 seconds
        } else if saturation_percent >= 90.0 {
            15 // High: wait 15 seconds
        } else {
            5 // Degraded: wait 5 seconds
        };

        Self::Backpressure {
            reason: format!(
                "Channel '{}' saturated ({:.1}%)",
                channel_name, saturation_percent
            ),
            retry_after_seconds: retry_after,
        }
    }

    /// TAS-75: Create a backpressure error for circuit breaker with Retry-After
    pub fn circuit_breaker_with_retry(recovery_timeout_seconds: u64) -> Self {
        Self::Backpressure {
            reason: "circuit_breaker_open".to_string(),
            retry_after_seconds: recovery_timeout_seconds,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // TAS-75: Special handling for Backpressure to include Retry-After header
        if let ApiError::Backpressure {
            reason,
            retry_after_seconds,
        } = &self
        {
            let error_response = json!({
                "error": {
                    "code": "BACKPRESSURE",
                    "message": format!("Service under backpressure: {}", reason),
                    "retry_after_seconds": retry_after_seconds
                }
            });

            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(
                    axum::http::header::RETRY_AFTER,
                    retry_after_seconds.to_string(),
                )],
                Json(error_response),
            )
                .into_response();
        }

        let (status_code, error_code, message) = match &self {
            ApiError::NotFound { message: msg } => {
                (StatusCode::NOT_FOUND, "NOT_FOUND", msg.as_str())
            }

            ApiError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", "Access denied"),

            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Authentication required",
            ),

            ApiError::BadRequest { message } => {
                (StatusCode::BAD_REQUEST, "BAD_REQUEST", message.as_str())
            }

            // TAS-154: 409 Conflict for duplicate identity hash
            ApiError::Conflict { message } => (StatusCode::CONFLICT, "CONFLICT", message.as_str()),

            ApiError::ServiceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                "Service temporarily unavailable",
            ),

            ApiError::Timeout => (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", "Request timeout"),

            ApiError::CircuitBreakerOpen => (
                StatusCode::SERVICE_UNAVAILABLE,
                "CIRCUIT_BREAKER_OPEN",
                "Service temporarily unavailable",
            ),

            // Note: This branch is unreachable due to early return above,
            // but kept for exhaustive match pattern
            ApiError::Backpressure { reason, .. } => (
                StatusCode::SERVICE_UNAVAILABLE,
                "BACKPRESSURE",
                reason.as_str(),
            ),

            ApiError::DatabaseError { operation } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                operation.as_str(),
            ),

            ApiError::AuthenticationError {
                reason,
                failure_context,
            } => {
                let error_response = json!({
                    "error": {
                        "code": "AUTHENTICATION_FAILED",
                        "message": reason
                    }
                });
                let mut response = (StatusCode::UNAUTHORIZED, Json(error_response)).into_response();
                if let Some(ctx) = failure_context {
                    let headers = response.headers_mut();
                    headers.insert(
                        "x-auth-failure-reason",
                        ctx.reason.as_str().parse().unwrap(),
                    );
                    headers.insert(
                        "x-auth-failure-severity",
                        ctx.severity.as_str().parse().unwrap(),
                    );
                    if let Some(retry_after) = ctx.severity.retry_after_seconds() {
                        headers.insert(
                            axum::http::header::RETRY_AFTER,
                            retry_after.to_string().parse().unwrap(),
                        );
                    }
                }
                return response;
            }

            ApiError::AuthorizationError {
                reason,
                failure_context,
            } => {
                let error_response = json!({
                    "error": {
                        "code": "AUTHORIZATION_FAILED",
                        "message": reason
                    }
                });
                let mut response = (StatusCode::FORBIDDEN, Json(error_response)).into_response();
                if let Some(ctx) = failure_context {
                    let headers = response.headers_mut();
                    headers.insert(
                        "x-auth-failure-reason",
                        ctx.reason.as_str().parse().unwrap(),
                    );
                    headers.insert(
                        "x-auth-failure-severity",
                        ctx.severity.as_str().parse().unwrap(),
                    );
                    if let Some(retry_after) = ctx.severity.retry_after_seconds() {
                        headers.insert(
                            axum::http::header::RETRY_AFTER,
                            retry_after.to_string().parse().unwrap(),
                        );
                    }
                }
                return response;
            }

            ApiError::InvalidUuid { uuid } => {
                (StatusCode::BAD_REQUEST, "INVALID_UUID", uuid.as_str())
            }

            ApiError::JsonError => (StatusCode::BAD_REQUEST, "JSON_ERROR", "Invalid JSON format"),

            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error",
            ),
        };

        let error_response = json!({
            "error": {
                "code": error_code,
                "message": message
            }
        });

        (status_code, Json(error_response)).into_response()
    }
}

/// Convert sqlx errors to API errors
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => ApiError::not_found("Database record not found"),
            sqlx::Error::Database(_) => ApiError::database_error("Database operation failed"),
            sqlx::Error::PoolTimedOut => ApiError::Timeout,
            _ => ApiError::database_error("Database error"),
        }
    }
}

/// Convert UUID parse errors to API errors
impl From<uuid::Error> for ApiError {
    fn from(_: uuid::Error) -> Self {
        ApiError::invalid_uuid("Invalid UUID format")
    }
}

/// Convert JSON errors to API errors
impl From<serde_json::Error> for ApiError {
    fn from(_: serde_json::Error) -> Self {
        ApiError::JsonError
    }
}

/// Convert JWT errors to API errors
impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::InvalidToken => {
                ApiError::auth_error("Invalid JWT token")
            }
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                ApiError::auth_error("Token has expired")
            }
            jsonwebtoken::errors::ErrorKind::InvalidAudience => {
                ApiError::auth_error("Invalid token audience")
            }
            jsonwebtoken::errors::ErrorKind::InvalidIssuer => {
                ApiError::auth_error("Invalid token issuer")
            }
            _ => ApiError::auth_error("Authentication failed"),
        }
    }
}

/// Result type alias for web API operations
pub type ApiResult<T> = Result<T, ApiError>;

/// Database operation types for smart pool selection
#[derive(Debug, Clone, Copy)]
pub enum DbOperationType {
    /// Write operations that need dedicated pool (task creation, cancellation)
    WebWrite,
    /// High-priority web operations requiring dedicated resources
    WebCritical,
    /// Read-only operations that can use shared orchestration pool
    ReadOnly,
    /// Analytics and reporting queries
    Analytics,
}

/// Database pool configuration for web operations
#[derive(Debug, Clone)]
pub struct DatabasePoolConfig {
    pub web_api_pool_size: u32,
    pub web_api_max_connections: u32,
    pub web_api_connection_timeout_seconds: u64,
    pub web_api_idle_timeout_seconds: u64,
}

// TAS-61: Removed CorsConfig - middleware uses hardcoded tower_http::cors::Any
// See: tasker-orchestration/src/web/middleware/mod.rs:create_cors_layer()

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub jwt_private_key: String,
    pub jwt_public_key: String,
    pub jwt_token_expiry_hours: u64,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub api_key_header: String,
    pub protected_routes: std::collections::HashMap<String, RouteAuthConfig>,
}

/// Authentication configuration for a specific route
#[derive(Debug, Clone)]
pub struct RouteAuthConfig {
    /// Type of authentication required ("bearer", "api_key")
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}

impl AuthConfig {
    /// Check if a route requires authentication
    pub fn route_requires_auth(&self, method: &str, path: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let route_key = format!("{method} {path}");

        // Check exact match first
        if let Some(config) = self.protected_routes.get(&route_key) {
            return config.required;
        }

        // Check for pattern matches (basic support for path parameters)
        for (pattern, config) in &self.protected_routes {
            if config.required && self.route_matches_pattern(&route_key, pattern) {
                return true;
            }
        }

        false
    }

    /// Get authentication type for a route
    pub fn auth_type_for_route(&self, method: &str, path: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let route_key = format!("{method} {path}");

        // Check exact match first
        if let Some(config) = self.protected_routes.get(&route_key) {
            if config.required {
                return Some(config.auth_type.clone());
            }
        }

        // Check for pattern matches
        for (pattern, config) in &self.protected_routes {
            if config.required && self.route_matches_pattern(&route_key, pattern) {
                return Some(config.auth_type.clone());
            }
        }

        None
    }

    /// Simple pattern matching for route paths with parameters
    /// Supports basic {param} patterns like "/v1/tasks/{task_uuid}"
    fn route_matches_pattern(&self, route: &str, pattern: &str) -> bool {
        let route_parts: Vec<&str> = route.split_whitespace().collect();
        let pattern_parts: Vec<&str> = pattern.split_whitespace().collect();

        if route_parts.len() != 2 || pattern_parts.len() != 2 {
            return false;
        }

        // Method must match exactly
        if route_parts[0] != pattern_parts[0] {
            return false;
        }

        // Path matching with parameter support
        let route_path_segments: Vec<&str> = route_parts[1].split('/').collect();
        let pattern_path_segments: Vec<&str> = pattern_parts[1].split('/').collect();

        if route_path_segments.len() != pattern_path_segments.len() {
            return false;
        }

        for (route_segment, pattern_segment) in
            route_path_segments.iter().zip(pattern_path_segments.iter())
        {
            // If pattern segment is a parameter (starts and ends with {}), it matches any value
            if pattern_segment.starts_with('{') && pattern_segment.ends_with('}') {
                continue;
            }
            // Otherwise, segments must match exactly
            if route_segment != pattern_segment {
                return false;
            }
        }

        true
    }
}

// TAS-61: Removed RateLimitConfig - no rate limiting middleware implemented
// Note: ErrorCategory::RateLimit and BackoffHintType::RateLimit are different and still used

/// Resilience configuration
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    pub circuit_breaker_enabled: bool,
    // TAS-61: Removed request_timeout_seconds - timeout hardcoded in middleware (30s)
    // TAS-61: Removed max_concurrent_requests - no concurrency limiting implemented
}

#[derive(Debug, Clone, PartialEq)]
pub enum SystemOperationalState {
    Normal,
    GracefulShutdown,
    Emergency,
    Stopped,
    Startup,
}

/// Standard health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthStatus {
    pub status: String,
    pub namespaces: Vec<NamespaceHealth>,
    pub system_metrics: WorkerSystemMetrics,
    pub uptime_seconds: u64,
    pub worker_id: String,
    pub worker_type: String,
}

/// Health status for individual namespaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceHealth {
    pub namespace: String,
    pub queue_depth: u64,
    pub health_status: String,
    pub queue_metrics: pgmq_notify::types::QueueMetrics,
}

/// System metrics for the worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSystemMetrics {
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
    pub active_commands: u32,
    pub total_commands_processed: u64,
    pub error_rate_percent: f64,
    pub last_activity: Option<DateTime<Utc>>,
}

/// Simple health check responses for Kubernetes probes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// Prometheus-compatible metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub metrics: HashMap<String, MetricValue>,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
}

/// Individual metric value with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub metric_type: String,
    pub labels: HashMap<String, String>,
    pub help: String,
}

/// Standard error response following orchestration patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub request_id: Option<String>,
}

// =============================================================================
// TAS-65: Domain Event Statistics for Test Observability
// =============================================================================

// Re-export canonical stats types from metrics::worker
pub use crate::metrics::worker::{EventRouterStats, InProcessEventBusStats};

/// Domain event statistics response for /metrics/events endpoint
///
/// Composite type that combines event router and in-process bus statistics.
/// Exposes event routing and delivery statistics for E2E test verification.
/// This allows tests to verify that domain events were actually published
/// via durable (PGMQ) and/or fast (in-process) delivery paths.
///
/// # Test Matrix Coverage
///
/// | Metric | Verifies |
/// |--------|----------|
/// | `router.durable_routed` | Durable events published to PGMQ |
/// | `router.fast_routed` | Fast events dispatched in-memory |
/// | `router.broadcast_routed` | Events sent to both paths |
/// | `in_process_bus.total_events_dispatched` | Total fast events processed |
/// | `in_process_bus.rust_handler_dispatches` | Rust subscribers received events |
/// | `in_process_bus.ffi_channel_dispatches` | FFI subscribers received events |
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainEventStats {
    /// Event router statistics
    pub router: EventRouterStats,
    /// In-process event bus statistics
    pub in_process_bus: InProcessEventBusStats,
    /// Timestamp when stats were captured
    pub captured_at: DateTime<Utc>,
    /// Worker ID for correlation
    pub worker_id: String,
}

// =============================================================================
// TAS-75: Backpressure Unit Tests
// =============================================================================

#[cfg(test)]
mod backpressure_tests {
    use super::*;

    // =========================================================================
    // ApiError::Backpressure Helper Method Tests
    // =========================================================================

    #[test]
    fn test_backpressure_helper_creates_correct_variant() {
        let error = ApiError::backpressure("test_reason", 42);

        match error {
            ApiError::Backpressure {
                reason,
                retry_after_seconds,
            } => {
                assert_eq!(reason, "test_reason");
                assert_eq!(retry_after_seconds, 42);
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    #[test]
    fn test_circuit_breaker_with_retry_creates_correct_variant() {
        let error = ApiError::circuit_breaker_with_retry(30);

        match error {
            ApiError::Backpressure {
                reason,
                retry_after_seconds,
            } => {
                assert_eq!(reason, "circuit_breaker_open");
                assert_eq!(retry_after_seconds, 30);
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    // =========================================================================
    // Retry-After Calculation Tests at Saturation Thresholds
    // =========================================================================

    #[test]
    fn test_channel_saturated_critical_threshold_30s() {
        // Critical: >= 95% saturation should have 30s retry
        let error = ApiError::channel_saturated("test_channel", 95.0);

        match error {
            ApiError::Backpressure {
                reason,
                retry_after_seconds,
            } => {
                assert!(reason.contains("test_channel"));
                assert!(reason.contains("95.0%"));
                assert_eq!(
                    retry_after_seconds, 30,
                    "Critical saturation (95%+) should be 30s"
                );
            }
            _ => panic!("Expected Backpressure variant"),
        }

        // Also test 100%
        let error_100 = ApiError::channel_saturated("test_channel", 100.0);
        match error_100 {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(retry_after_seconds, 30, "100% saturation should be 30s");
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    #[test]
    fn test_channel_saturated_high_threshold_15s() {
        // High: >= 90% but < 95% saturation should have 15s retry
        let error = ApiError::channel_saturated("test_channel", 90.0);

        match error {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(
                    retry_after_seconds, 15,
                    "High saturation (90-95%) should be 15s"
                );
            }
            _ => panic!("Expected Backpressure variant"),
        }

        // Test boundary at 94.9%
        let error_94 = ApiError::channel_saturated("test_channel", 94.9);
        match error_94 {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(retry_after_seconds, 15, "94.9% saturation should be 15s");
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    #[test]
    fn test_channel_saturated_degraded_threshold_5s() {
        // Degraded: >= 80% but < 90% saturation should have 5s retry
        let error = ApiError::channel_saturated("test_channel", 80.0);

        match error {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(
                    retry_after_seconds, 5,
                    "Degraded saturation (80-90%) should be 5s"
                );
            }
            _ => panic!("Expected Backpressure variant"),
        }

        // Test at 89.9%
        let error_89 = ApiError::channel_saturated("test_channel", 89.9);
        match error_89 {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(retry_after_seconds, 5, "89.9% saturation should be 5s");
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    #[test]
    fn test_channel_saturated_below_threshold_still_5s() {
        // Below 80% still gets 5s as minimum (this case shouldn't normally
        // be called, but the function handles it gracefully)
        let error = ApiError::channel_saturated("test_channel", 50.0);

        match error {
            ApiError::Backpressure {
                retry_after_seconds,
                ..
            } => {
                assert_eq!(retry_after_seconds, 5, "Below 80% should default to 5s");
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    #[test]
    fn test_channel_saturated_reason_format() {
        let error = ApiError::channel_saturated("orchestration_command", 87.5);

        match error {
            ApiError::Backpressure { reason, .. } => {
                assert!(
                    reason.contains("orchestration_command"),
                    "Reason should contain channel name"
                );
                assert!(
                    reason.contains("87.5%"),
                    "Reason should contain saturation percentage"
                );
            }
            _ => panic!("Expected Backpressure variant"),
        }
    }

    // =========================================================================
    // Response Formatting Tests (IntoResponse)
    // =========================================================================

    #[test]
    fn test_backpressure_response_status_code() {
        let error = ApiError::backpressure("test_reason", 15);
        let response = error.into_response();

        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "Backpressure should return 503 Service Unavailable"
        );
    }

    #[test]
    fn test_backpressure_response_retry_after_header() {
        let error = ApiError::backpressure("test_reason", 42);
        let response = error.into_response();

        let retry_after = response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .expect("Response should have Retry-After header");

        assert_eq!(
            retry_after.to_str().unwrap(),
            "42",
            "Retry-After header should match retry_after_seconds"
        );
    }

    #[test]
    fn test_backpressure_response_retry_after_at_thresholds() {
        // Test 5s threshold
        let error_5s = ApiError::channel_saturated("ch", 85.0);
        let response_5s = error_5s.into_response();
        let retry_5s = response_5s
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .unwrap();
        assert_eq!(retry_5s.to_str().unwrap(), "5");

        // Test 15s threshold
        let error_15s = ApiError::channel_saturated("ch", 92.0);
        let response_15s = error_15s.into_response();
        let retry_15s = response_15s
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .unwrap();
        assert_eq!(retry_15s.to_str().unwrap(), "15");

        // Test 30s threshold
        let error_30s = ApiError::channel_saturated("ch", 97.0);
        let response_30s = error_30s.into_response();
        let retry_30s = response_30s
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .unwrap();
        assert_eq!(retry_30s.to_str().unwrap(), "30");
    }

    #[test]
    fn test_circuit_breaker_open_response() {
        // The old CircuitBreakerOpen variant should still work
        let error = ApiError::CircuitBreakerOpen;
        let response = error.into_response();

        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "CircuitBreakerOpen should return 503"
        );

        // Note: CircuitBreakerOpen doesn't have Retry-After (use circuit_breaker_with_retry for that)
        assert!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .is_none(),
            "CircuitBreakerOpen variant should not have Retry-After header"
        );
    }

    #[test]
    fn test_backpressure_error_display() {
        let error = ApiError::backpressure("channel_saturated", 15);
        let display = format!("{}", error);

        assert!(
            display.contains("backpressure"),
            "Display should mention backpressure"
        );
        assert!(
            display.contains("channel_saturated"),
            "Display should include reason"
        );
    }
}
