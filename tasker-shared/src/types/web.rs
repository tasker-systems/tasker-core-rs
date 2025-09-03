//! # Web API Error Types
//!
//! Defines error types specific to the web API and their HTTP response conversions.
//! Leverages thiserror for structured error handling and Axum's IntoResponse for HTTP conversion.

use crate::config::{ConfigManager, WebConfig};
use crate::{TaskerError, TaskerResult};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;
use tracing::{debug, error, info};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Web API specific errors with HTTP status code mappings
#[derive(Error, Debug)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub enum ApiError {
    #[error("Resource not found")]
    NotFound,

    #[error("Access denied")]
    Forbidden,

    #[error("Authentication required")]
    Unauthorized,

    #[error("Invalid request: {message}")]
    BadRequest { message: String },

    #[error("Service temporarily unavailable")]
    ServiceUnavailable,

    #[error("Request timeout")]
    Timeout,

    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,

    #[error("Database operation failed: {operation}")]
    DatabaseError { operation: String },

    #[error("JWT authentication failed: {reason}")]
    AuthenticationError { reason: String },

    #[error("Authorization failed: {reason}")]
    AuthorizationError { reason: String },

    #[error("Invalid UUID format: {uuid}")]
    InvalidUuid { uuid: String },

    #[error("JSON serialization/deserialization error")]
    JsonError,

    #[error("Internal server error")]
    Internal,
}

impl ApiError {
    /// Create a BadRequest error with a custom message
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    /// Create a DatabaseError with operation context
    pub fn database_error(operation: impl Into<String>) -> Self {
        Self::DatabaseError {
            operation: operation.into(),
        }
    }

    /// Create an AuthenticationError with reason
    pub fn auth_error(reason: impl Into<String>) -> Self {
        Self::AuthenticationError {
            reason: reason.into(),
        }
    }

    /// Create an AuthorizationError with reason
    pub fn authorization_error(reason: impl Into<String>) -> Self {
        Self::AuthorizationError {
            reason: reason.into(),
        }
    }

    /// Create an InvalidUuid error
    pub fn invalid_uuid(uuid: impl Into<String>) -> Self {
        Self::InvalidUuid { uuid: uuid.into() }
    }

    pub fn internal_server_error(_message: impl Into<String>) -> Self {
        Self::Internal
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status_code, error_code, message) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", "Resource not found"),

            ApiError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", "Access denied"),

            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Authentication required",
            ),

            ApiError::BadRequest { message } => {
                (StatusCode::BAD_REQUEST, "BAD_REQUEST", message.as_str())
            }

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

            ApiError::DatabaseError { operation } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                operation.as_str(),
            ),

            ApiError::AuthenticationError { reason } => (
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_FAILED",
                reason.as_str(),
            ),

            ApiError::AuthorizationError { reason } => (
                StatusCode::FORBIDDEN,
                "AUTHORIZATION_FAILED",
                reason.as_str(),
            ),

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
            sqlx::Error::RowNotFound => ApiError::NotFound,
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

/// CORS configuration
#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
}

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

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub per_client_limit: bool,
}

/// Resilience configuration
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    pub circuit_breaker_enabled: bool,
    pub request_timeout_seconds: u64,
    pub max_concurrent_requests: u32,
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

/// Worker status response with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatusResponse {
    pub worker_id: String,
    pub worker_type: String,
    pub status: String,
    pub uptime_seconds: u64,
    pub configuration: WorkerConfigurationStatus,
    pub capabilities: WorkerCapabilities,
    pub performance_metrics: WorkerPerformanceMetrics,
    pub registered_handlers: Vec<RegisteredHandler>,
}

/// Configuration status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigurationStatus {
    pub environment: String,
    pub database_connected: bool,
    pub event_system_enabled: bool,
    pub supported_namespaces: Vec<String>,
}

/// Worker capabilities information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub can_process_steps: bool,
    pub can_initialize_tasks: bool,
    pub event_publishing_enabled: bool,
    pub health_monitoring_enabled: bool,
    pub supported_message_types: Vec<String>,
}

/// Performance metrics for the worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPerformanceMetrics {
    pub steps_processed_total: u64,
    pub steps_processed_success: u64,
    pub steps_processed_failed: u64,
    pub average_processing_time_ms: f64,
    pub queue_processing_rate: f64,
    pub last_step_processed: Option<DateTime<Utc>>,
    pub error_details: Vec<RecentError>,
}

/// Information about registered step handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredHandler {
    pub namespace: String,
    pub handler_class: String,
    pub version: String,
    pub last_used: Option<DateTime<Utc>>,
    pub success_count: u64,
    pub failure_count: u64,
}

/// Recent error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentError {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub message: String,
    pub namespace: Option<String>,
    pub step_name: Option<String>,
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
