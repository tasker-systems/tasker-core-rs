//! # Worker Authentication Middleware
//!
//! TAS-150: Authentication middleware for the worker web API.
//! Same pattern as orchestration but adapted for worker's Arc<WorkerWebState>.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use tasker_shared::metrics::security as security_metrics;
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::{AuthMethod, SecurityContext};
use tasker_shared::types::web::ApiError;
use tracing::{info, warn};

use crate::web::state::WorkerWebState;

/// Authentication middleware for the worker web API.
///
/// If SecurityService is absent or disabled, injects a disabled context.
/// Otherwise validates Bearer token or API key.
pub async fn authenticate_request(
    State(state): State<Arc<WorkerWebState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let security_service = match &state.security_service {
        Some(svc) if svc.is_enabled() => svc,
        _ => {
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
            &[KeyValue::new("result", if result.is_ok() { "success" } else { "failure" })],
        );

        result.map_err(|e| {
            warn!(error = %e, "Worker: Bearer token authentication failed");
            security_metrics::auth_requests_total().add(1, &[
                KeyValue::new("method", "jwt"),
                KeyValue::new("result", "failure"),
            ]);
            security_metrics::auth_failures_total().add(1, &[
                KeyValue::new("reason", "invalid"),
            ]);
            ApiError::auth_error("Invalid or expired token")
        })?
    } else if let Some(key) = api_key {
        security_service.authenticate_api_key(&key).map_err(|e| {
            warn!(error = %e, "Worker: API key authentication failed");
            security_metrics::auth_requests_total().add(1, &[
                KeyValue::new("method", "api_key"),
                KeyValue::new("result", "failure"),
            ]);
            security_metrics::auth_failures_total().add(1, &[
                KeyValue::new("reason", "invalid"),
            ]);
            ApiError::auth_error("Invalid API key")
        })?
    } else {
        warn!("Worker: Request missing authentication credentials");
        security_metrics::auth_failures_total().add(1, &[
            KeyValue::new("reason", "missing"),
        ]);
        return Err(ApiError::auth_error("Missing authentication credentials"));
    };

    let method_label = match &ctx.auth_method {
        AuthMethod::Jwt => "jwt",
        AuthMethod::ApiKey { .. } => "api_key",
        AuthMethod::Disabled => "disabled",
    };

    security_metrics::auth_requests_total().add(1, &[
        KeyValue::new("method", method_label),
        KeyValue::new("result", "success"),
    ]);

    info!(
        subject = %ctx.subject,
        method = ?ctx.auth_method,
        "Worker request authenticated"
    );

    request.extensions_mut().insert(ctx);
    Ok(next.run(request).await)
}

/// Check that the security context has the required permission.
pub fn require_permission(ctx: &SecurityContext, perm: Permission) -> Result<(), ApiError> {
    if ctx.auth_method == AuthMethod::Disabled {
        return Ok(());
    }
    if ctx.has_permission(&perm) {
        Ok(())
    } else {
        warn!(
            subject = %ctx.subject,
            required = %perm,
            "Worker: Permission denied"
        );
        security_metrics::permission_denials_total().add(1, &[
            KeyValue::new("permission", perm.as_str().to_string()),
        ]);
        Err(ApiError::authorization_error(format!(
            "Missing required permission: {}",
            perm
        )))
    }
}
