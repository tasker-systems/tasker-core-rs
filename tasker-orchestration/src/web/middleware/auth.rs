//! # Authentication Middleware
//!
//! JWT-based authentication middleware for worker systems and API access.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::{debug, warn};

use crate::web::state::AppState;
use tasker_shared::types::auth::JwtAuthenticator;
use tasker_shared::types::web::ApiError;

/// Authentication middleware for protected endpoints
///
/// This middleware is applied selectively to endpoints that require authentication.
/// It validates JWT tokens and extracts worker claims for authorization.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // TAS-61: Use converted WebAuthConfig from AppState
    let auth_config = match &state.auth_config {
        Some(config) => config,
        None => {
            debug!("Authentication disabled (no auth config) - allowing request");
            return Ok(next.run(request).await);
        }
    };

    // Skip auth if disabled in configuration
    if !auth_config.enabled {
        debug!("Authentication disabled - allowing request");
        return Ok(next.run(request).await);
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("authorization")
        .ok_or_else(|| ApiError::auth_error("Missing authorization header"))?;

    let auth_str = auth_header
        .to_str()
        .map_err(|_| ApiError::auth_error("Invalid authorization header format"))?;

    // Extract Bearer token
    let token = extract_bearer_token(auth_str)?;

    // Create authenticator from configuration
    let authenticator = JwtAuthenticator::from_config(auth_config)
        .map_err(|e| ApiError::auth_error(format!("Auth configuration error: {e}")))?;

    // Validate token and extract claims
    let claims = authenticator.validate_worker_token(token).map_err(|e| {
        warn!(error = %e, "JWT validation failed");
        ApiError::auth_error("Invalid or expired token")
    })?;

    debug!(
        worker_id = %claims.sub,
        namespaces = ?claims.worker_namespaces,
        "Authenticated worker request"
    );

    // Add claims to request extensions for handlers to access
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

/// Optional authentication middleware
///
/// Similar to require_auth but allows requests through even without valid auth.
/// Used for endpoints that can work with or without authentication.
pub async fn optional_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // TAS-61: Use converted WebAuthConfig from AppState
    let auth_config = match &state.auth_config {
        Some(config) if config.enabled => config,
        _ => {
            // Auth disabled or not configured
            return next.run(request).await;
        }
    };

    // Try to extract and validate token, but don't fail if missing/invalid
    if let Some(auth_header) = request.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Ok(token) = extract_bearer_token(auth_str) {
                if let Ok(authenticator) = JwtAuthenticator::from_config(auth_config) {
                    if let Ok(claims) = authenticator.validate_worker_token(token) {
                        debug!(
                            worker_id = %claims.sub,
                            "Optional auth succeeded"
                        );
                        request.extensions_mut().insert(claims);
                    }
                }
            }
        }
    }

    next.run(request).await
}

/// Extract Bearer token from Authorization header
fn extract_bearer_token(auth_header: &str) -> Result<&str, ApiError> {
    if !auth_header.starts_with("Bearer ") {
        return Err(ApiError::auth_error(
            "Authorization header must use Bearer scheme",
        ));
    }

    let token = &auth_header[7..]; // Skip "Bearer " prefix
    if token.is_empty() {
        return Err(ApiError::auth_error("Empty Bearer token"));
    }

    Ok(token)
}

/// Conditional authentication middleware that applies auth based on route configuration
///
/// This middleware checks the configuration for each route to determine if authentication
/// is required. Only routes marked as requiring auth in the protected_routes configuration
/// will have authentication enforced.
pub async fn conditional_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // TAS-61: Use converted WebAuthConfig from AppState
    let auth_config = match &state.auth_config {
        Some(config) => config,
        None => {
            // No auth config - allow all requests
            return next.run(request).await;
        }
    };

    // Extract method and path from request
    let method = request.method().to_string();
    let path = request.uri().path();

    // Check if this route requires authentication according to configuration
    if !auth_config.route_requires_auth(&method, path) {
        // Route doesn't require auth, proceed without authentication
        debug!(
            method = %method,
            path = %path,
            "Route does not require authentication - allowing request"
        );
        return next.run(request).await;
    }

    // Route requires authentication - delegate to the standard auth middleware
    debug!(
        method = %method,
        path = %path,
        auth_type = auth_config.auth_type_for_route(&method, path).unwrap_or_else(|| "unknown".to_string()),
        "Route requires authentication - applying auth middleware"
    );

    match require_auth(State(state), request, next).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tasker_shared::types::web::{AuthConfig, RouteAuthConfig};

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token("Bearer abc123").unwrap(), "abc123");

        assert!(extract_bearer_token("Basic abc123").is_err());
        assert!(extract_bearer_token("Bearer ").is_err());
        assert!(extract_bearer_token("abc123").is_err());
    }

    #[test]
    fn test_auth_config_route_requires_auth() {
        let mut protected_routes = HashMap::new();
        protected_routes.insert(
            "DELETE /v1/tasks/{task_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );
        protected_routes.insert(
            "PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );
        protected_routes.insert(
            "POST /v1/tasks".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: false, // Not required
            },
        );

        let auth_config = AuthConfig {
            enabled: true,
            jwt_private_key: "test_key".to_string(),
            jwt_public_key: "test_public_key".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_issuer: "test_issuer".to_string(),
            jwt_audience: "test_audience".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes,
        };

        // Test routes that require auth
        assert!(auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789"));
        assert!(auth_config
            .route_requires_auth("PATCH", "/v1/tasks/123-456-789/workflow_steps/step-123"));

        // Test route that doesn't require auth (configured as optional)
        assert!(!auth_config.route_requires_auth("POST", "/v1/tasks"));

        // Test routes that are not configured
        assert!(!auth_config.route_requires_auth("GET", "/v1/tasks"));
        assert!(!auth_config.route_requires_auth("GET", "/health"));

        // Test wrong method
        assert!(!auth_config.route_requires_auth("GET", "/v1/tasks/123-456-789"));
    }

    #[test]
    fn test_auth_config_with_disabled_auth() {
        let mut protected_routes = HashMap::new();
        protected_routes.insert(
            "DELETE /v1/tasks/{task_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );

        let auth_config = AuthConfig {
            enabled: false, // Auth disabled globally
            jwt_private_key: "test_key".to_string(),
            jwt_public_key: "test_public_key".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_issuer: "test_issuer".to_string(),
            jwt_audience: "test_audience".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes,
        };

        // Should return false for all routes when auth is disabled globally
        assert!(!auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789"));
        assert!(!auth_config.route_requires_auth("POST", "/v1/tasks"));
    }

    #[test]
    fn test_auth_type_for_route() {
        let mut protected_routes = HashMap::new();
        protected_routes.insert(
            "DELETE /v1/tasks/{task_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );
        protected_routes.insert(
            "GET /v1/analytics/performance".to_string(),
            RouteAuthConfig {
                auth_type: "api_key".to_string(),
                required: true,
            },
        );

        let auth_config = AuthConfig {
            enabled: true,
            jwt_private_key: "test_key".to_string(),
            jwt_public_key: "test_public_key".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_issuer: "test_issuer".to_string(),
            jwt_audience: "test_audience".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes,
        };

        assert_eq!(
            auth_config.auth_type_for_route("DELETE", "/v1/tasks/123-456-789"),
            Some("bearer".to_string())
        );
        assert_eq!(
            auth_config.auth_type_for_route("GET", "/v1/analytics/performance"),
            Some("api_key".to_string())
        );
        assert_eq!(auth_config.auth_type_for_route("GET", "/v1/tasks"), None);
    }

    #[test]
    fn test_route_pattern_matching_via_public_interface() {
        // Test pattern matching through the public interface by setting up
        // protected routes with patterns and testing if actual routes match
        let mut protected_routes = HashMap::new();
        protected_routes.insert(
            "DELETE /v1/tasks/{task_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );
        protected_routes.insert(
            "PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}".to_string(),
            RouteAuthConfig {
                auth_type: "bearer".to_string(),
                required: true,
            },
        );

        let auth_config = AuthConfig {
            enabled: true,
            jwt_private_key: "test_key".to_string(),
            jwt_public_key: "test_public_key".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_issuer: "test_issuer".to_string(),
            jwt_audience: "test_audience".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes,
        };

        // Test that pattern matching works through the public interface
        // Basic pattern matching with UUIDs
        assert!(auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789"));
        assert!(auth_config.route_requires_auth("DELETE", "/v1/tasks/uuid-abc-123"));

        // Complex pattern matching with multiple parameters
        assert!(auth_config
            .route_requires_auth("PATCH", "/v1/tasks/123-456-789/workflow_steps/step-abc-123"));
        assert!(auth_config.route_requires_auth(
            "PATCH",
            "/v1/tasks/different-uuid/workflow_steps/different-step-id"
        ));

        // Test that wrong methods don't match
        assert!(!auth_config.route_requires_auth("GET", "/v1/tasks/123-456-789"));
        assert!(!auth_config.route_requires_auth("POST", "/v1/tasks/123-456-789"));

        // Test that wrong paths don't match
        assert!(!auth_config.route_requires_auth("DELETE", "/v1/tasks"));
        assert!(!auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789/invalid"));
    }
}
