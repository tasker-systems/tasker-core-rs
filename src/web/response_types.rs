//! # Web API Error Types
//!
//! Defines error types specific to the web API and their HTTP response conversions.
//! Leverages thiserror for structured error handling and Axum's IntoResponse for HTTP conversion.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

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
#[cfg(feature = "web-api")]
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
