//! # Security Context
//!
//! Runtime security context carrying authenticated identity, auth method, and permissions.
//! Extracted from request by middleware and used by handlers for permission checks.

use super::permissions::{has_permission, Permission};
#[cfg(feature = "web-api")]
use super::web::ApiError;

/// How the request was authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    /// Authenticated via JWT bearer token.
    Jwt,
    /// Authenticated via API key.
    ApiKey {
        /// Human-readable description of the key used.
        description: String,
    },
    /// Authentication is disabled; all requests are allowed.
    Disabled,
}

/// Security context attached to each request by the auth middleware.
///
/// Carries the authenticated identity, permissions, and metadata needed for
/// per-handler permission checks.
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Authenticated subject (user/service identifier from token `sub` or API key description).
    pub subject: String,
    /// How this request was authenticated.
    pub auth_method: AuthMethod,
    /// Claimed permissions (from JWT claims or API key configuration).
    pub permissions: Vec<String>,
    /// Token issuer (JWT `iss` claim), if applicable.
    pub issuer: Option<String>,
    /// Token expiration (Unix timestamp), if applicable.
    pub expires_at: Option<i64>,
}

impl SecurityContext {
    /// Check if this context has the required permission.
    ///
    /// When auth is disabled (`AuthMethod::Disabled`), all permission checks pass.
    pub fn has_permission(&self, required: &Permission) -> bool {
        if self.auth_method == AuthMethod::Disabled {
            return true;
        }
        has_permission(&self.permissions, required)
    }

    /// Create a context representing disabled authentication.
    ///
    /// Used when the security configuration has auth disabled, allowing all operations.
    /// Note: The permissions list is empty because when auth is disabled,
    /// `has_permission()` always returns true without checking permissions.
    pub fn disabled_context() -> Self {
        Self {
            subject: "anonymous".to_string(),
            auth_method: AuthMethod::Disabled,
            permissions: vec![],
            issuer: None,
            expires_at: None,
        }
    }
}

/// Axum extractor implementation for SecurityContext.
///
/// Extracts the context injected by the auth middleware. The middleware
/// always inserts a context (disabled context when auth is off), so this
/// should never fail if the middleware is properly configured.
#[cfg(feature = "web-api")]
impl<S> axum::extract::FromRequestParts<S> for SecurityContext
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<SecurityContext>()
            .cloned()
            .ok_or_else(|| {
                ApiError::auth_error(
                    "Security context not found - auth middleware may not have run",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_context(permissions: Vec<&str>) -> SecurityContext {
        SecurityContext {
            subject: "test-user".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: permissions.into_iter().map(String::from).collect(),
            issuer: Some("tasker".to_string()),
            expires_at: Some(9999999999),
        }
    }

    fn api_key_context(permissions: Vec<&str>) -> SecurityContext {
        SecurityContext {
            subject: "test-key".to_string(),
            auth_method: AuthMethod::ApiKey {
                description: "Test key".to_string(),
            },
            permissions: permissions.into_iter().map(String::from).collect(),
            issuer: None,
            expires_at: None,
        }
    }

    #[test]
    fn test_disabled_context_allows_everything() {
        let ctx = SecurityContext::disabled_context();
        assert!(ctx.has_permission(&Permission::TasksCreate));
        assert!(ctx.has_permission(&Permission::DlqUpdate));
        assert!(ctx.has_permission(&Permission::SystemConfigRead));
    }

    #[test]
    fn test_jwt_context_exact_permission() {
        let ctx = jwt_context(vec!["tasks:create", "tasks:read"]);
        assert!(ctx.has_permission(&Permission::TasksCreate));
        assert!(ctx.has_permission(&Permission::TasksRead));
        assert!(!ctx.has_permission(&Permission::TasksList));
        assert!(!ctx.has_permission(&Permission::DlqRead));
    }

    #[test]
    fn test_jwt_context_wildcard_permission() {
        let ctx = jwt_context(vec!["tasks:*"]);
        assert!(ctx.has_permission(&Permission::TasksCreate));
        assert!(ctx.has_permission(&Permission::TasksRead));
        assert!(ctx.has_permission(&Permission::TasksList));
        assert!(ctx.has_permission(&Permission::TasksCancel));
        assert!(!ctx.has_permission(&Permission::StepsRead));
    }

    #[test]
    fn test_jwt_context_global_wildcard_rejected() {
        // Global wildcard (*) is NOT supported - only resource:* patterns are allowed
        let ctx = jwt_context(vec!["*"]);
        for perm in Permission::all() {
            assert!(
                !ctx.has_permission(perm),
                "Global wildcard should not grant {}",
                perm
            );
        }
    }

    #[test]
    fn test_jwt_context_full_access_with_resource_wildcards() {
        // Use explicit resource wildcards for full access
        let ctx = jwt_context(vec![
            "tasks:*",
            "steps:*",
            "dlq:*",
            "templates:*",
            "system:*",
            "worker:*",
        ]);
        for perm in Permission::all() {
            assert!(
                ctx.has_permission(perm),
                "Full resource wildcards should grant {}",
                perm
            );
        }
    }

    #[test]
    fn test_jwt_context_no_permissions() {
        let ctx = jwt_context(vec![]);
        assert!(!ctx.has_permission(&Permission::TasksCreate));
    }

    #[test]
    fn test_api_key_context_permission_check() {
        let ctx = api_key_context(vec!["tasks:read", "tasks:list"]);
        assert!(ctx.has_permission(&Permission::TasksRead));
        assert!(ctx.has_permission(&Permission::TasksList));
        assert!(!ctx.has_permission(&Permission::TasksCreate));
    }

    #[test]
    fn test_auth_method_equality() {
        assert_eq!(AuthMethod::Jwt, AuthMethod::Jwt);
        assert_eq!(AuthMethod::Disabled, AuthMethod::Disabled);
        assert_eq!(
            AuthMethod::ApiKey {
                description: "a".to_string()
            },
            AuthMethod::ApiKey {
                description: "a".to_string()
            }
        );
        assert_ne!(AuthMethod::Jwt, AuthMethod::Disabled);
        assert_ne!(
            AuthMethod::ApiKey {
                description: "a".to_string()
            },
            AuthMethod::ApiKey {
                description: "b".to_string()
            }
        );
    }
}
