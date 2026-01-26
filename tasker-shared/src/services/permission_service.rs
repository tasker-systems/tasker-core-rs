//! # Permission Service
//!
//! TAS-76: Consolidated permission checking logic shared between orchestration and worker.
//! Provides framework-agnostic permission enforcement that works with both REST (Axum)
//! and future gRPC (Tonic) endpoints.
//!
//! ## Design
//!
//! The permission service is intentionally simple - it checks if a `SecurityContext`
//! has the required permission and returns an appropriate error if not. The error type
//! is `ApiError` which can be converted to HTTP responses (Axum) or gRPC status (Tonic).
//!
//! ## Usage
//!
//! ```ignore
//! use tasker_shared::services::require_permission;
//! use tasker_shared::types::permissions::Permission;
//!
//! fn my_handler(ctx: SecurityContext) -> Result<(), ApiError> {
//!     require_permission(&ctx, Permission::TasksCreate)?;
//!     // ... handler logic
//!     Ok(())
//! }
//! ```

use opentelemetry::KeyValue;
use tracing::warn;

use crate::metrics::security as security_metrics;
use crate::types::permissions::Permission;
use crate::types::security::{AuthMethod, SecurityContext};
use crate::types::web::{ApiError, AuthFailureSeverity};

/// Check that the security context has the required permission.
///
/// Returns `Ok(())` if:
/// - Auth is disabled (`AuthMethod::Disabled`)
/// - The context's permissions include the required permission (exact or wildcard)
///
/// Returns `Err(ApiError)` with 403 status if permission is missing.
///
/// # Metrics
///
/// Increments `security.permission_denials_total` counter with `permission` label on failure.
///
/// # Example
///
/// ```ignore
/// require_permission(&security_context, Permission::TasksCreate)?;
/// ```
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
            "Permission denied"
        );
        security_metrics::permission_denials_total()
            .add(1, &[KeyValue::new("permission", perm.as_str().to_string())]);
        Err(ApiError::authorization_error_with_context(
            format!("Missing required permission: {}", perm),
            AuthFailureSeverity::Medium,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_auth_always_passes() {
        let ctx = SecurityContext::disabled_context();
        assert!(require_permission(&ctx, Permission::TasksCreate).is_ok());
        assert!(require_permission(&ctx, Permission::SystemConfigRead).is_ok());
    }

    #[test]
    fn test_exact_permission_passes() {
        let ctx = SecurityContext {
            subject: "user".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec!["tasks:create".to_string()],
            issuer: None,
            expires_at: None,
        };
        assert!(require_permission(&ctx, Permission::TasksCreate).is_ok());
    }

    #[test]
    fn test_missing_permission_fails() {
        let ctx = SecurityContext {
            subject: "user".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec!["tasks:read".to_string()],
            issuer: None,
            expires_at: None,
        };
        assert!(require_permission(&ctx, Permission::TasksCreate).is_err());
    }

    #[test]
    fn test_wildcard_permission_passes() {
        let ctx = SecurityContext {
            subject: "admin".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec!["tasks:*".to_string()],
            issuer: None,
            expires_at: None,
        };
        assert!(require_permission(&ctx, Permission::TasksCreate).is_ok());
        assert!(require_permission(&ctx, Permission::TasksRead).is_ok());
        assert!(require_permission(&ctx, Permission::TasksList).is_ok());
        // Different resource should fail
        assert!(require_permission(&ctx, Permission::DlqRead).is_err());
    }

    #[test]
    fn test_global_wildcard_passes_all() {
        let ctx = SecurityContext {
            subject: "superadmin".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec!["*".to_string()],
            issuer: None,
            expires_at: None,
        };
        assert!(require_permission(&ctx, Permission::TasksCreate).is_ok());
        assert!(require_permission(&ctx, Permission::DlqUpdate).is_ok());
        assert!(require_permission(&ctx, Permission::SystemConfigRead).is_ok());
    }
}
