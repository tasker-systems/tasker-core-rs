//! # Authentication Middleware
//!
//! TAS-150: Unified authentication middleware using SecurityService.
//! Supports JWT bearer tokens and API key authentication.
//! Injects SecurityContext into request extensions for per-handler permission checks.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use tracing::{debug, warn};

use crate::web::state::AppState;
use tasker_shared::metrics::security as security_metrics;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::{ApiError, AuthFailureReason, AuthFailureSeverity};

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
    let security_service = match state.security_service() {
        Some(svc) if svc.is_enabled() => svc,
        _ => {
            // Auth disabled - inject permissive context
            request
                .extensions_mut()
                .insert(SecurityContext::disabled_context());
            return Ok(next.run(request).await);
        }
    };

    // Try Bearer token (reject non-UTF-8 headers explicitly)
    let bearer_token = match request.headers().get("authorization") {
        Some(h) => match h.to_str() {
            Ok(s) => s.strip_prefix("Bearer ").map(|t| t.to_string()),
            Err(_) => {
                warn!("Authorization header contains non-UTF-8 bytes");
                security_metrics::auth_failures_total()
                    .add(1, &[KeyValue::new("reason", "malformed")]);
                return Err(ApiError::auth_error_with_context(
                    "Malformed Authorization header",
                    AuthFailureReason::Malformed,
                    AuthFailureSeverity::High,
                ));
            }
        },
        None => None,
    };

    // Try API key (reject non-UTF-8 headers explicitly)
    let api_key_header = security_service.api_key_header().to_string();
    let api_key = match request.headers().get(api_key_header.as_str()) {
        Some(h) => match h.to_str() {
            Ok(s) => Some(s.to_string()),
            Err(_) => {
                warn!("API key header contains non-UTF-8 bytes");
                security_metrics::auth_failures_total()
                    .add(1, &[KeyValue::new("reason", "malformed")]);
                return Err(ApiError::auth_error_with_context(
                    "Malformed API key header",
                    AuthFailureReason::Malformed,
                    AuthFailureSeverity::High,
                ));
            }
        },
        None => None,
    };

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
            ApiError::auth_error_with_context(
                "Invalid or expired token",
                AuthFailureReason::Invalid,
                AuthFailureSeverity::High,
            )
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
            ApiError::auth_error_with_context(
                "Invalid API key",
                AuthFailureReason::Invalid,
                AuthFailureSeverity::High,
            )
        })?
    } else {
        warn!("Request missing authentication credentials");
        security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "missing")]);
        return Err(ApiError::auth_error_with_context(
            "Missing authentication credentials",
            AuthFailureReason::Missing,
            AuthFailureSeverity::Low,
        ));
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

    debug!(
        subject = %ctx.subject,
        method = ?ctx.auth_method,
        "Request authenticated"
    );

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
    if state.security_service().is_some() {
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

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token("Bearer abc123").unwrap(), "abc123");

        assert!(extract_bearer_token("Basic abc123").is_err());
        assert!(extract_bearer_token("Bearer ").is_err());
        assert!(extract_bearer_token("abc123").is_err());
    }
}
