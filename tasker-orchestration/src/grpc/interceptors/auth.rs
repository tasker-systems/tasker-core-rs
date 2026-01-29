//! Authentication for gRPC services.
//!
//! This module provides async authentication that mirrors the REST auth middleware,
//! extracting credentials from gRPC metadata and validating them using SecurityService.
//!
//! ## Why not a tonic interceptor?
//!
//! Tonic's sync interceptor API (`interceptor()`) cannot perform async operations.
//! Real authentication requires async operations like:
//! - Database lookups for API key validation
//! - JWKS fetches for JWT validation
//! - External auth provider calls
//!
//! A sync interceptor that only checks header presence (without validation) would be
//! security theater. Instead, we do proper async authentication per-handler via
//! [`AuthInterceptor::authenticate()`].

use std::sync::Arc;
use tasker_shared::types::{SecurityContext, SecurityService};
use tonic::{Request, Status};

/// Extension key for SecurityContext in gRPC requests.
///
/// After authentication, the SecurityContext is inserted into request extensions
/// and can be retrieved by service handlers.
pub const SECURITY_CONTEXT_KEY: &str = "security-context";

/// Authentication interceptor for gRPC services.
///
/// Extracts Bearer tokens or API keys from gRPC metadata and validates them
/// using the SecurityService. On success, inserts a SecurityContext into
/// request extensions.
#[derive(Clone, Debug)]
pub struct AuthInterceptor {
    security_service: Option<Arc<SecurityService>>,
}

impl AuthInterceptor {
    /// Create a new auth interceptor.
    pub fn new(security_service: Option<Arc<SecurityService>>) -> Self {
        Self { security_service }
    }

    /// Check if authentication is enabled.
    pub fn is_enabled(&self) -> bool {
        self.security_service
            .as_ref()
            .map(|s| s.is_enabled())
            .unwrap_or(false)
    }

    /// Authenticate a request and return the SecurityContext.
    ///
    /// This is the core authentication logic, separated for reuse.
    pub async fn authenticate<T>(&self, request: &Request<T>) -> Result<SecurityContext, Status> {
        let security_service = match &self.security_service {
            Some(svc) if svc.is_enabled() => svc,
            _ => {
                // Auth disabled - return permissive context
                return Ok(SecurityContext::disabled_context());
            }
        };

        // Extract Bearer token from authorization metadata
        let bearer_token = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.strip_prefix("Bearer ")
                    .or_else(|| s.strip_prefix("bearer "))
            })
            .map(|t| t.to_string());

        // Extract API key from x-api-key metadata
        let api_key = request
            .metadata()
            .get(security_service.api_key_header())
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Try Bearer token first, then API key
        if let Some(token) = bearer_token {
            security_service
                .authenticate_bearer(&token)
                .await
                .map_err(|e| {
                    tracing::warn!(error = %e, "Bearer token authentication failed");
                    Status::unauthenticated("Invalid or expired credentials")
                })
        } else if let Some(key) = api_key {
            // authenticate_api_key is synchronous (no await needed)
            security_service.authenticate_api_key(&key).map_err(|e| {
                tracing::warn!(error = %e, "API key authentication failed");
                Status::unauthenticated("Invalid or expired credentials")
            })
        } else {
            // Auth required but no credentials provided
            Err(Status::unauthenticated(
                "Authentication required. Provide Bearer token or API key.",
            ))
        }
    }
}

// Note: Tonic's sync interceptor API cannot perform async operations like
// database lookups or JWT validation. Authentication is handled per-handler
// via AuthInterceptor::authenticate() which is async.
//
// If a tower-based async middleware is needed in the future, consider using
// tower::ServiceBuilder with a custom async layer.

/// Helper trait for extracting SecurityContext from gRPC request extensions.
pub trait SecurityContextExt {
    /// Get the SecurityContext from request extensions.
    fn security_context(&self) -> Option<&SecurityContext>;
}

impl<T> SecurityContextExt for Request<T> {
    fn security_context(&self) -> Option<&SecurityContext> {
        self.extensions().get::<SecurityContext>()
    }
}
