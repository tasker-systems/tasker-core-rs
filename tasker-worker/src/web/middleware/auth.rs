//! # Worker Authentication Middleware
//!
//! TAS-150: Authentication middleware for the worker web API.
//! Same pattern as orchestration but adapted for worker's Arc<WorkerWebState>.
//!
//! TAS-76: Permission checking is now consolidated in tasker-shared and re-exported here.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use tasker_shared::metrics::security as security_metrics;
use tasker_shared::types::security::{AuthMethod, SecurityContext};
use tasker_shared::types::web::{ApiError, AuthFailureReason, AuthFailureSeverity};
use tracing::{debug, warn};

use crate::web::state::WorkerWebState;

// TAS-76: Re-export the shared permission service for handler use
pub use tasker_shared::services::require_permission;

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

    // Try Bearer token (reject non-UTF-8 headers explicitly)
    let bearer_token = match request.headers().get("authorization") {
        Some(h) => match h.to_str() {
            Ok(s) => s.strip_prefix("Bearer ").map(|t| t.to_string()),
            Err(_) => {
                warn!("Worker: Authorization header contains non-UTF-8 bytes");
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
                warn!("Worker: API key header contains non-UTF-8 bytes");
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
            warn!(error = %e, "Worker: Bearer token authentication failed");
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
            warn!(error = %e, "Worker: API key authentication failed");
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
        warn!("Worker: Request missing authentication credentials");
        security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "missing")]);
        return Err(ApiError::auth_error_with_context(
            "Missing authentication credentials",
            AuthFailureReason::Missing,
            AuthFailureSeverity::Low,
        ));
    };

    let method_label = match &ctx.auth_method {
        AuthMethod::Jwt => "jwt",
        AuthMethod::ApiKey { .. } => "api_key",
        AuthMethod::Disabled => "disabled",
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
        "Worker request authenticated"
    );

    request.extensions_mut().insert(ctx);
    Ok(next.run(request).await)
}

// TAS-76: require_permission is now re-exported from tasker_shared::services at the top of this file
