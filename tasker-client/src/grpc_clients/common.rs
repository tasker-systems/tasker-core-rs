//! Common utilities for gRPC clients.
//!
//! Provides shared functionality for channel management, authentication,
//! and error handling across gRPC clients.

use std::time::Duration;
use tonic::{
    metadata::{Ascii, MetadataKey, MetadataValue},
    service::Interceptor,
    transport::{Channel, Endpoint},
    Request, Status,
};
use tracing::debug;

use crate::error::ClientError;

/// Configuration for gRPC client authentication.
///
/// Supports multiple authentication methods:
/// - Bearer token (JWT or opaque token)
/// - API key (via custom header)
#[derive(Debug, Clone, Default)]
pub struct GrpcAuthConfig {
    /// Bearer token for Authorization header
    pub bearer_token: Option<String>,
    /// API key value
    pub api_key: Option<String>,
    /// Custom header name for API key (defaults to "x-api-key")
    pub api_key_header: Option<String>,
}

impl GrpcAuthConfig {
    /// Create auth config with bearer token
    #[must_use]
    pub fn with_bearer_token(token: impl Into<String>) -> Self {
        Self {
            bearer_token: Some(token.into()),
            ..Default::default()
        }
    }

    /// Create auth config with API key
    #[must_use]
    pub fn with_api_key(key: impl Into<String>) -> Self {
        Self {
            api_key: Some(key.into()),
            ..Default::default()
        }
    }

    /// Create auth config with API key and custom header
    #[must_use]
    pub fn with_api_key_header(key: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            api_key: Some(key.into()),
            api_key_header: Some(header.into()),
            ..Default::default()
        }
    }

    /// Check if any authentication is configured
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.bearer_token.is_some() || self.api_key.is_some()
    }
}

/// Configuration for gRPC clients.
#[derive(Debug, Clone)]
pub struct GrpcClientConfig {
    /// gRPC endpoint URL (e.g., "http://localhost:9090")
    pub endpoint: String,
    /// Request timeout
    pub timeout: Duration,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Authentication configuration
    pub auth: Option<GrpcAuthConfig>,
    /// TCP keepalive interval
    pub tcp_keepalive: Option<Duration>,
    /// HTTP/2 keepalive interval
    pub http2_keepalive_interval: Option<Duration>,
    /// HTTP/2 keepalive timeout
    pub http2_keepalive_timeout: Option<Duration>,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9090".to_string(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            auth: None,
            tcp_keepalive: Some(Duration::from_secs(30)),
            http2_keepalive_interval: Some(Duration::from_secs(30)),
            http2_keepalive_timeout: Some(Duration::from_secs(10)),
        }
    }
}

impl GrpcClientConfig {
    /// Create a new config with the given endpoint
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            ..Default::default()
        }
    }

    /// Set the request timeout
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the connection timeout
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set authentication configuration
    #[must_use]
    pub fn with_auth(mut self, auth: GrpcAuthConfig) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Build a tonic Channel from this configuration
    pub async fn connect(&self) -> Result<Channel, ClientError> {
        let mut endpoint = Endpoint::from_shared(self.endpoint.clone()).map_err(|e| {
            ClientError::ConfigError(format!("Invalid gRPC endpoint '{}': {}", self.endpoint, e))
        })?;

        endpoint = endpoint
            .timeout(self.timeout)
            .connect_timeout(self.connect_timeout);

        if let Some(keepalive) = self.tcp_keepalive {
            endpoint = endpoint.tcp_keepalive(Some(keepalive));
        }

        if let Some(interval) = self.http2_keepalive_interval {
            endpoint = endpoint.http2_keep_alive_interval(interval);
        }

        if let Some(timeout) = self.http2_keepalive_timeout {
            endpoint = endpoint.keep_alive_timeout(timeout);
        }

        debug!(endpoint = %self.endpoint, "Connecting to gRPC endpoint");

        endpoint
            .connect()
            .await
            .map_err(|e| ClientError::ServiceUnavailable {
                service: self.endpoint.clone(),
                reason: format!("Failed to connect: {}", e),
            })
    }
}

/// gRPC interceptor for adding authentication headers to requests.
///
/// This interceptor adds authentication metadata to outgoing gRPC requests
/// based on the configured authentication method.
#[derive(Debug, Clone)]
pub struct AuthInterceptor {
    auth: Option<GrpcAuthConfig>,
}

impl AuthInterceptor {
    /// Create a new auth interceptor with the given configuration
    #[must_use]
    pub fn new(auth: Option<GrpcAuthConfig>) -> Self {
        Self { auth }
    }

    /// Create an interceptor with no authentication
    #[must_use]
    pub fn none() -> Self {
        Self { auth: None }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(ref auth) = self.auth {
            let metadata = request.metadata_mut();

            // Bearer token takes precedence
            if let Some(ref token) = auth.bearer_token {
                let value = format!("Bearer {}", token)
                    .parse::<MetadataValue<_>>()
                    .map_err(|e| Status::internal(format!("Invalid bearer token: {}", e)))?;
                metadata.insert("authorization", value);
            } else if let Some(ref api_key) = auth.api_key {
                // Use custom header or default to x-api-key
                let header_name = auth
                    .api_key_header
                    .as_deref()
                    .unwrap_or("x-api-key")
                    .to_lowercase();

                let value = api_key
                    .parse::<MetadataValue<_>>()
                    .map_err(|e| Status::internal(format!("Invalid API key: {}", e)))?;

                // gRPC metadata keys must be lowercase ASCII
                let key: MetadataKey<Ascii> = header_name
                    .parse()
                    .map_err(|e| Status::internal(format!("Invalid header name: {}", e)))?;
                metadata.insert(key, value);
            }
        }

        Ok(request)
    }
}

/// Convert tonic Status to ClientError
impl From<Status> for ClientError {
    fn from(status: Status) -> Self {
        match status.code() {
            tonic::Code::NotFound => ClientError::TaskNotFound {
                task_id: status.message().to_string(),
            },
            tonic::Code::Unauthenticated => ClientError::AuthError(status.message().to_string()),
            tonic::Code::PermissionDenied => {
                ClientError::AuthError(format!("Permission denied: {}", status.message()))
            }
            tonic::Code::InvalidArgument => ClientError::InvalidInput(status.message().to_string()),
            tonic::Code::Unavailable => ClientError::ServiceUnavailable {
                service: "gRPC".to_string(),
                reason: status.message().to_string(),
            },
            tonic::Code::DeadlineExceeded => ClientError::Timeout {
                operation: status.message().to_string(),
            },
            tonic::Code::Cancelled => {
                ClientError::Internal(format!("Request cancelled: {}", status.message()))
            }
            tonic::Code::ResourceExhausted => ClientError::ServiceUnavailable {
                service: "gRPC".to_string(),
                reason: format!("Resource exhausted: {}", status.message()),
            },
            _ => ClientError::ApiError {
                status: status.code() as u16,
                message: status.message().to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_auth_config_bearer_token() {
        let auth = GrpcAuthConfig::with_bearer_token("test-token");
        assert!(auth.is_configured());
        assert_eq!(auth.bearer_token, Some("test-token".to_string()));
        assert!(auth.api_key.is_none());
    }

    #[test]
    fn test_grpc_auth_config_api_key() {
        let auth = GrpcAuthConfig::with_api_key("secret-key");
        assert!(auth.is_configured());
        assert_eq!(auth.api_key, Some("secret-key".to_string()));
        assert!(auth.bearer_token.is_none());
    }

    #[test]
    fn test_grpc_auth_config_api_key_with_header() {
        let auth = GrpcAuthConfig::with_api_key_header("secret-key", "X-Custom-Key");
        assert!(auth.is_configured());
        assert_eq!(auth.api_key, Some("secret-key".to_string()));
        assert_eq!(auth.api_key_header, Some("X-Custom-Key".to_string()));
    }

    #[test]
    fn test_grpc_client_config_default() {
        let config = GrpcClientConfig::default();
        assert_eq!(config.endpoint, "http://localhost:9090");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_grpc_client_config_builder() {
        let config = GrpcClientConfig::new("http://custom:9090")
            .with_timeout(Duration::from_secs(60))
            .with_auth(GrpcAuthConfig::with_bearer_token("token"));

        assert_eq!(config.endpoint, "http://custom:9090");
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(config.auth.is_some());
    }

    #[test]
    fn test_status_to_client_error_not_found() {
        let status = Status::not_found("task-123");
        let error: ClientError = status.into();
        assert!(matches!(error, ClientError::TaskNotFound { .. }));
    }

    #[test]
    fn test_status_to_client_error_unauthenticated() {
        let status = Status::unauthenticated("Invalid token");
        let error: ClientError = status.into();
        assert!(matches!(error, ClientError::AuthError(_)));
    }

    #[test]
    fn test_status_to_client_error_unavailable() {
        let status = Status::unavailable("Service down");
        let error: ClientError = status.into();
        assert!(matches!(error, ClientError::ServiceUnavailable { .. }));
    }

    #[test]
    fn test_status_to_client_error_timeout() {
        let status = Status::deadline_exceeded("Request timed out");
        let error: ClientError = status.into();
        assert!(matches!(error, ClientError::Timeout { .. }));
    }
}
