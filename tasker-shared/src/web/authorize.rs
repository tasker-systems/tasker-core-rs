//! # Handler Authorization Wrapper
//!
//! TAS-176: Provides declarative resource-based authorization at the route level.
//! Permission checking happens BEFORE body deserialization.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::web::authorize;
//! use tasker_shared::types::resources::{Resource, Action};
//!
//! // In route definitions:
//! Router::new()
//!     .route("/tasks", post(authorize(Resource::Tasks, Action::Create, create_task)))
//!     .route("/tasks", get(authorize(Resource::Tasks, Action::List, list_tasks)))
//!     // Public routes don't need authorize():
//!     .route("/health", get(health_check))
//! ```
//!
//! ## Design
//!
//! The `authorize()` function wraps a handler with permission checking:
//!
//! 1. Extracts `SecurityContext` from request extensions (set by auth middleware)
//! 2. If resource is public (Health/Metrics/Docs) → proceeds to handler
//! 3. If auth disabled (`AuthMethod::Disabled`) → proceeds to handler
//! 4. Checks `has_permission(required)` → if yes, proceeds; if no, returns 403
//!
//! This moves authorization from handler code to route definitions, making it
//! declarative and ensuring checks happen before body deserialization.

use std::future::Future;
use std::pin::Pin;

use axum::extract::Request;
use axum::handler::Handler;
use axum::response::{IntoResponse, Response};
use opentelemetry::KeyValue;
use tracing::warn;

use crate::metrics::security as security_metrics;
use crate::types::resources::{Action, Resource, ResourceAction};
use crate::types::security::{AuthMethod, SecurityContext};
use crate::types::web::{ApiError, AuthFailureSeverity};

/// Wraps a handler with resource+action authorization.
///
/// The authorization check happens before the inner handler is invoked,
/// and before any request body deserialization occurs.
///
/// # Arguments
///
/// * `resource` - The resource being accessed (e.g., `Resource::Tasks`)
/// * `action` - The action being performed (e.g., `Action::Create`)
/// * `handler` - The handler function to wrap
///
/// # Returns
///
/// A new handler that checks authorization before calling the inner handler.
///
/// # Example
///
/// ```rust,ignore
/// use tasker_shared::web::authorize;
/// use tasker_shared::types::resources::{Resource, Action};
///
/// Router::new()
///     .route("/tasks", post(authorize(Resource::Tasks, Action::Create, create_task)))
///     .route("/tasks/{id}", get(authorize(Resource::Tasks, Action::Read, get_task)))
/// ```
pub fn authorize<H, T, S>(resource: Resource, action: Action, handler: H) -> AuthorizedHandler<H>
where
    H: Handler<T, S>,
    T: 'static,
    S: Clone + Send + Sync + 'static,
{
    AuthorizedHandler {
        resource_action: ResourceAction::new(resource, action),
        inner: handler,
    }
}

/// Handler wrapper that performs authorization before delegating to the inner handler.
#[derive(Clone)]
pub struct AuthorizedHandler<H> {
    resource_action: ResourceAction,
    inner: H,
}

impl<H> AuthorizedHandler<H> {
    /// Get the resource being protected.
    pub fn resource(&self) -> Resource {
        self.resource_action.resource
    }

    /// Get the action being performed.
    pub fn action(&self) -> Action {
        self.resource_action.action
    }
}

impl<H, T, S> Handler<T, S> for AuthorizedHandler<H>
where
    H: Handler<T, S> + Clone + Send + 'static,
    T: 'static,
    S: Clone + Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request, state: S) -> Self::Future {
        Box::pin(async move {
            // Check if this is a public resource (no auth needed)
            if self.resource_action.resource.is_public() {
                return self.inner.call(req, state).await.into_response();
            }

            // Extract SecurityContext from request extensions
            let security_context = match req.extensions().get::<SecurityContext>() {
                Some(ctx) => ctx.clone(),
                None => {
                    // If middleware didn't inject a context, auth might be misconfigured
                    // This shouldn't happen if middleware is set up correctly
                    warn!(
                        resource = %self.resource_action.resource,
                        action = %self.resource_action.action,
                        "SecurityContext missing from request extensions"
                    );
                    return ApiError::auth_error(
                        "Security context not found - auth middleware may not have run",
                    )
                    .into_response();
                }
            };

            // If auth is disabled, allow everything
            if security_context.auth_method == AuthMethod::Disabled {
                return self.inner.call(req, state).await.into_response();
            }

            // Get the required permission for this resource+action
            let required = match self.resource_action.required_permission() {
                Some(perm) => perm,
                None => {
                    // Public resource - shouldn't reach here due to is_public() check above
                    return self.inner.call(req, state).await.into_response();
                }
            };

            // Check permission
            if security_context.has_permission(&required) {
                // Permission granted - call the inner handler
                self.inner.call(req, state).await.into_response()
            } else {
                // Permission denied
                warn!(
                    subject = %security_context.subject,
                    required = %required,
                    resource = %self.resource_action.resource,
                    action = %self.resource_action.action,
                    "Permission denied at route level"
                );
                security_metrics::permission_denials_total().add(
                    1,
                    &[KeyValue::new("permission", required.as_str().to_string())],
                );
                ApiError::authorization_error_with_context(
                    format!("Missing required permission: {}", required),
                    AuthFailureSeverity::Medium,
                )
                .into_response()
            }
        })
    }
}

// Debug implementation for better error messages
impl<H> std::fmt::Debug for AuthorizedHandler<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedHandler")
            .field("resource_action", &self.resource_action)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a test handler
    async fn test_handler() -> &'static str {
        "ok"
    }

    #[allow(dead_code)] // Used in integration tests
    fn make_context(permissions: Vec<&str>) -> SecurityContext {
        SecurityContext {
            subject: "test-user".to_string(),
            auth_method: AuthMethod::Jwt,
            permissions: permissions.into_iter().map(String::from).collect(),
            issuer: Some("test".to_string()),
            expires_at: None,
        }
    }

    #[test]
    fn test_authorized_handler_debug() {
        // Use explicit type annotation for state since we're not using it in a Router
        let handler = authorize::<_, _, ()>(Resource::Tasks, Action::Create, test_handler);
        let debug = format!("{:?}", handler);
        assert!(debug.contains("AuthorizedHandler"));
        assert!(debug.contains("resource_action"));
    }

    #[test]
    fn test_authorized_handler_accessors() {
        // Use explicit type annotation for state since we're not using it in a Router
        let handler = authorize::<_, _, ()>(Resource::Tasks, Action::Create, test_handler);
        assert_eq!(handler.resource(), Resource::Tasks);
        assert_eq!(handler.action(), Action::Create);
    }

    #[test]
    fn test_resource_action_for_public_resource() {
        let ra = ResourceAction::new(Resource::Health, Action::Read);
        assert!(ra.required_permission().is_none());
    }

    #[test]
    fn test_resource_action_for_protected_resource() {
        let ra = ResourceAction::new(Resource::Tasks, Action::Create);
        assert!(ra.required_permission().is_some());
    }

    // Full integration tests with request/response flow are in:
    // - tasker-orchestration/src/web/routes.rs tests
    // - tasker-worker/src/web/routes.rs tests
}
