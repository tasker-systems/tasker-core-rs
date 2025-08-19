//! # Authentication Middleware
//!
//! JWT-based authentication middleware for worker systems and API access.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use tracing::{debug, warn};

use crate::web::auth::JwtAuthenticator;
use crate::web::errors::ApiError;
use crate::web::state::AppState;

/// Authentication middleware for protected endpoints
///
/// This middleware is applied selectively to endpoints that require authentication.
/// It validates JWT tokens and extracts worker claims for authorization.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Skip auth if disabled in configuration
    if !state.config.auth.enabled {
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
    let authenticator = JwtAuthenticator::from_config(&state.config.auth)
        .map_err(|e| ApiError::auth_error(format!("Auth configuration error: {e}")))?;

    // Validate token and extract claims
    let claims = authenticator.validate_worker_token(token)
        .map_err(|e| {
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
    if !state.config.auth.enabled {
        return next.run(request).await;
    }

    // Try to extract and validate token, but don't fail if missing/invalid
    if let Some(auth_header) = request.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Ok(token) = extract_bearer_token(auth_str) {
                if let Ok(authenticator) = JwtAuthenticator::from_config(&state.config.auth) {
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
        return Err(ApiError::auth_error("Authorization header must use Bearer scheme"));
    }

    let token = &auth_header[7..]; // Skip "Bearer " prefix
    if token.is_empty() {
        return Err(ApiError::auth_error("Empty Bearer token"));
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(
            extract_bearer_token("Bearer abc123").unwrap(),
            "abc123"
        );

        assert!(extract_bearer_token("Basic abc123").is_err());
        assert!(extract_bearer_token("Bearer ").is_err());
        assert!(extract_bearer_token("abc123").is_err());
    }
}