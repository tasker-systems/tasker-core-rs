//! # Permission Checking
//!
//! TAS-150: Per-handler permission enforcement utilities.
//! Used by handlers to verify the authenticated user has the required permission.
//!
//! TAS-76: This module re-exports the shared permission service from tasker-shared.
//! The implementation is consolidated there to enable sharing between orchestration,
//! worker, and future gRPC endpoints.

// Re-export the shared permission service
pub use tasker_shared::services::require_permission;

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::types::permissions::Permission;
    use tasker_shared::types::security::{AuthMethod, SecurityContext};

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
    fn test_global_wildcard_rejected() {
        // Global wildcard (*) is NOT supported - only resource:* patterns are allowed
        let ctx = SecurityContext {
            subject: "superadmin".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec!["*".to_string()],
            issuer: None,
            expires_at: None,
        };
        // All permissions should be denied with only global wildcard
        assert!(require_permission(&ctx, Permission::TasksCreate).is_err());
        assert!(require_permission(&ctx, Permission::DlqUpdate).is_err());
        assert!(require_permission(&ctx, Permission::SystemConfigRead).is_err());
    }

    #[test]
    fn test_all_resource_wildcards_grants_full_access() {
        // Use explicit resource wildcards for full access
        let ctx = SecurityContext {
            subject: "admin".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: vec![
                "tasks:*".to_string(),
                "steps:*".to_string(),
                "dlq:*".to_string(),
                "templates:*".to_string(),
                "system:*".to_string(),
                "worker:*".to_string(),
            ],
            issuer: None,
            expires_at: None,
        };
        assert!(require_permission(&ctx, Permission::TasksCreate).is_ok());
        assert!(require_permission(&ctx, Permission::DlqUpdate).is_ok());
        assert!(require_permission(&ctx, Permission::SystemConfigRead).is_ok());
    }
}
