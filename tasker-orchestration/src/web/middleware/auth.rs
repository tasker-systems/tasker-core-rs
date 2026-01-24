//! # Authentication Middleware
//!
//! TAS-150: Unified authentication middleware using SecurityService.
//! Supports JWT bearer tokens and API key authentication.
//! Injects SecurityContext into request extensions for per-handler permission checks.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use tracing::{info, warn};

use crate::web::state::AppState;
use tasker_shared::metrics::security as security_metrics;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::ApiError;

/// Authentication middleware that authenticates all requests.
///
/// Behavior:
/// - If SecurityService is absent or disabled → injects `SecurityContext::disabled_context()`
/// - If Bearer token present → validates via JWT/JWKS
/// - If API key header present → validates via API key registry
/// - Neither credential present when auth is enabled → 401
///
/// Always injects a `SecurityContext` into request extensions so handlers can
/// perform permission checks.
pub async fn authenticate_request(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let security_service = match &state.security_service {
        Some(svc) if svc.is_enabled() => svc,
        _ => {
            // Auth disabled - inject permissive context
            request
                .extensions_mut()
                .insert(SecurityContext::disabled_context());
            return Ok(next.run(request).await);
        }
    };

    // Try Bearer token
    let bearer_token = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| t.to_string());

    // Try API key
    let api_key_header = security_service.api_key_header().to_string();
    let api_key = request
        .headers()
        .get(api_key_header.as_str())
        .and_then(|h| h.to_str().ok())
        .map(|k| k.to_string());

    let ctx = if let Some(token) = bearer_token {
        let start = std::time::Instant::now();
        let result = security_service.authenticate_bearer(&token).await;
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        security_metrics::jwt_verification_duration().record(
            duration_ms,
            &[KeyValue::new(
                "result",
                if result.is_ok() { "success" } else { "failure" },
            )],
        );

        result.map_err(|e| {
            warn!(error = %e, "Bearer token authentication failed");
            security_metrics::auth_requests_total().add(
                1,
                &[
                    KeyValue::new("method", "jwt"),
                    KeyValue::new("result", "failure"),
                ],
            );
            security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "invalid")]);
            ApiError::auth_error("Invalid or expired token")
        })?
    } else if let Some(key) = api_key {
        security_service.authenticate_api_key(&key).map_err(|e| {
            warn!(error = %e, "API key authentication failed");
            security_metrics::auth_requests_total().add(
                1,
                &[
                    KeyValue::new("method", "api_key"),
                    KeyValue::new("result", "failure"),
                ],
            );
            security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "invalid")]);
            ApiError::auth_error("Invalid API key")
        })?
    } else {
        warn!("Request missing authentication credentials");
        security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "missing")]);
        return Err(ApiError::auth_error("Missing authentication credentials"));
    };

    let method_label = match &ctx.auth_method {
        tasker_shared::types::security::AuthMethod::Jwt => "jwt",
        tasker_shared::types::security::AuthMethod::ApiKey { .. } => "api_key",
        tasker_shared::types::security::AuthMethod::Disabled => "disabled",
    };

    security_metrics::auth_requests_total().add(
        1,
        &[
            KeyValue::new("method", method_label),
            KeyValue::new("result", "success"),
        ],
    );

    info!(
        subject = %ctx.subject,
        method = ?ctx.auth_method,
        "Request authenticated"
    );

    // Also insert legacy WorkerClaims for backward compat with existing extractors
    let legacy_claims = tasker_shared::types::auth::TokenClaims {
        sub: ctx.subject.clone(),
        worker_namespaces: vec!["*".to_string()],
        iss: ctx.issuer.clone().unwrap_or_default(),
        aud: String::new(),
        exp: ctx.expires_at.unwrap_or(0),
        iat: chrono::Utc::now().timestamp(),
        permissions: ctx.permissions.clone(),
    };
    request.extensions_mut().insert(legacy_claims);
    request.extensions_mut().insert(ctx);

    Ok(next.run(request).await)
}

/// Extract Bearer token from Authorization header (utility for tests).
#[cfg(test)]
fn extract_bearer_token(auth_header: &str) -> Result<&str, ApiError> {
    if !auth_header.starts_with("Bearer ") {
        return Err(ApiError::auth_error(
            "Authorization header must use Bearer scheme",
        ));
    }

    let token = &auth_header[7..];
    if token.is_empty() {
        return Err(ApiError::auth_error("Empty Bearer token"));
    }

    Ok(token)
}

/// Conditional authentication middleware (legacy, kept for backward compatibility).
///
/// Delegates to `authenticate_request` for routes requiring auth.
/// For routes not in the protected_routes config, injects a disabled context.
pub async fn conditional_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    use axum::response::IntoResponse;

    // If security service is configured, use the new middleware
    if state.security_service.is_some() {
        match authenticate_request(State(state), request, next).await {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    } else {
        // No security service - inject disabled context and proceed
        let mut request = request;
        request
            .extensions_mut()
            .insert(SecurityContext::disabled_context());
        next.run(request).await
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
                required: false,
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

        assert!(auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789"));
        assert!(auth_config
            .route_requires_auth("PATCH", "/v1/tasks/123-456-789/workflow_steps/step-123"));
        assert!(!auth_config.route_requires_auth("POST", "/v1/tasks"));
        assert!(!auth_config.route_requires_auth("GET", "/v1/tasks"));
        assert!(!auth_config.route_requires_auth("GET", "/health"));
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
            enabled: false,
            jwt_private_key: "test_key".to_string(),
            jwt_public_key: "test_public_key".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_issuer: "test_issuer".to_string(),
            jwt_audience: "test_audience".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes,
        };

        assert!(!auth_config.route_requires_auth("DELETE", "/v1/tasks/123-456-789"));
        assert!(!auth_config.route_requires_auth("POST", "/v1/tasks"));
    }
}
