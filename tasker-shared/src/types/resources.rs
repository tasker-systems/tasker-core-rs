//! # Resource-Based Authorization Types
//!
//! Defines resources and actions for protocol-agnostic authorization.
//! Routes declare which resource+action they protect, and the authorization
//! wrapper checks permissions before invoking handlers.
//!
//! This module provides a declarative approach where routes specify:
//! ```rust,ignore
//! .route("/tasks", post(authorize(Resource::Tasks, Action::Create, handler)))
//! ```
//!
//! The `ResourceAction` resolves to the appropriate `Permission` enum variant.

use super::permissions::Permission;
use std::fmt;

/// Resources in the system.
///
/// Each resource maps to a permission prefix (e.g., `Resource::Tasks` → `"tasks:*"`).
/// Public resources (Health, Metrics, Docs) don't require authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Task management (`tasks:*`)
    Tasks,
    /// Workflow step operations (`steps:*`)
    Steps,
    /// Dead letter queue (`dlq:*`)
    Dlq,
    /// Task templates (`templates:*`)
    Templates,
    /// System administration (`system:*`)
    System,
    /// Worker-specific operations (`worker:*`)
    Worker,
    /// Health endpoints - public, no auth required
    Health,
    /// Metrics endpoints - public, no auth required
    Metrics,
    /// API documentation - public, no auth required
    Docs,
}

impl Resource {
    /// Check if this resource is public (no authentication required).
    #[must_use]
    pub const fn is_public(&self) -> bool {
        matches!(self, Self::Health | Self::Metrics | Self::Docs)
    }

    /// Get the permission prefix for this resource.
    #[must_use]
    pub const fn permission_prefix(&self) -> &'static str {
        match self {
            Self::Tasks => "tasks",
            Self::Steps => "steps",
            Self::Dlq => "dlq",
            Self::Templates => "templates",
            Self::System => "system",
            Self::Worker => "worker",
            Self::Health | Self::Metrics | Self::Docs => "",
        }
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tasks => "tasks",
            Self::Steps => "steps",
            Self::Dlq => "dlq",
            Self::Templates => "templates",
            Self::System => "system",
            Self::Worker => "worker",
            Self::Health => "health",
            Self::Metrics => "metrics",
            Self::Docs => "docs",
        })
    }
}

/// Actions that can be performed on resources.
///
/// Standard CRUD actions plus resource-specific actions like `Cancel` and `Resolve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Create new resource (POST)
    Create,
    /// Read single resource (GET by ID)
    Read,
    /// List resources (GET collection)
    List,
    /// Update resource (PUT/PATCH)
    Update,
    /// Delete resource (DELETE)
    Delete,

    // Task-specific actions
    /// Cancel a running task (`tasks:cancel`)
    Cancel,
    /// Read task execution context (`tasks:context_read`)
    ContextRead,

    // Step-specific actions
    /// Manually resolve a step (`steps:resolve`)
    Resolve,

    // DLQ-specific actions
    /// Get DLQ statistics (`dlq:stats`)
    Stats,

    // Template-specific actions
    /// Validate a template (`templates:validate`)
    Validate,

    // System-specific actions
    /// Read system configuration (`system:config_read`, `worker:config_read`)
    ConfigRead,
    /// Read analytics data (`system:analytics_read`)
    AnalyticsRead,
    /// Read registered handlers (legacy) (`system:handlers_read`)
    HandlersRead,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Create => "create",
            Self::Read => "read",
            Self::List => "list",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Cancel => "cancel",
            Self::ContextRead => "context_read",
            Self::Resolve => "resolve",
            Self::Stats => "stats",
            Self::Validate => "validate",
            Self::ConfigRead => "config_read",
            Self::AnalyticsRead => "analytics_read",
            Self::HandlersRead => "handlers_read",
        })
    }
}

/// Combines Resource + Action for declarative route authorization.
///
/// # Example
///
/// ```rust
/// use tasker_shared::types::resources::{Resource, Action, ResourceAction};
///
/// let ra = ResourceAction::new(Resource::Tasks, Action::Create);
/// assert!(ra.required_permission().is_some());
///
/// // Public resources return None
/// let public = ResourceAction::new(Resource::Health, Action::Read);
/// assert!(public.required_permission().is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAction {
    /// The resource being accessed
    pub resource: Resource,
    /// The action being performed
    pub action: Action,
}

impl ResourceAction {
    /// Create a new resource+action pair.
    #[must_use]
    pub const fn new(resource: Resource, action: Action) -> Self {
        Self { resource, action }
    }

    /// Returns the required permission, or `None` for public resources.
    ///
    /// Public resources (Health, Metrics, Docs) don't require authentication,
    /// so this returns `None` for those resources.
    #[must_use]
    pub fn required_permission(&self) -> Option<Permission> {
        if self.resource.is_public() {
            return None;
        }
        Some(self.to_permission())
    }

    /// Convert to the corresponding Permission enum variant.
    ///
    /// # Panics
    ///
    /// Panics if called on a public resource (Health, Metrics, Docs) or
    /// an invalid resource+action combination. Use `required_permission()`
    /// for safe access.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "Private method matching internal conventions; Copy type but method is private"
    )]
    fn to_permission(&self) -> Permission {
        match (self.resource, self.action) {
            // Tasks
            (Resource::Tasks, Action::Create) => Permission::TasksCreate,
            (Resource::Tasks, Action::Read) => Permission::TasksRead,
            (Resource::Tasks, Action::List) => Permission::TasksList,
            (Resource::Tasks, Action::Cancel) => Permission::TasksCancel,
            (Resource::Tasks, Action::ContextRead) => Permission::TasksContextRead,

            // Steps - Read/List both use StepsRead permission
            (Resource::Steps, Action::Read | Action::List) => Permission::StepsRead,
            (Resource::Steps, Action::Resolve) => Permission::StepsResolve,

            // DLQ - Read/List both use DlqRead permission
            (Resource::Dlq, Action::Read | Action::List) => Permission::DlqRead,
            (Resource::Dlq, Action::Update) => Permission::DlqUpdate,
            (Resource::Dlq, Action::Stats) => Permission::DlqStats,

            // Templates - Read/List both use TemplatesRead permission
            (Resource::Templates, Action::Read | Action::List) => Permission::TemplatesRead,
            (Resource::Templates, Action::Validate) => Permission::TemplatesValidate,

            // System
            (Resource::System, Action::ConfigRead) => Permission::SystemConfigRead,
            (Resource::System, Action::HandlersRead) => Permission::HandlersRead,
            (Resource::System, Action::AnalyticsRead) => Permission::AnalyticsRead,

            // Worker
            (Resource::Worker, Action::ConfigRead) => Permission::WorkerConfigRead,
            (Resource::Worker, Action::Read | Action::List) => Permission::WorkerTemplatesRead,
            (Resource::Worker, Action::Validate) => Permission::TemplatesValidate,

            // Invalid combinations - these indicate a bug in route configuration
            (resource, action) => {
                panic!(
                    "Invalid resource+action combination: {:?}:{:?}. \
                     If this is a public resource, use required_permission() instead.",
                    resource, action
                )
            }
        }
    }
}

impl fmt::Display for ResourceAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.resource, self.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Resource Tests
    // =========================================================================

    #[test]
    fn test_public_resources() {
        assert!(Resource::Health.is_public());
        assert!(Resource::Metrics.is_public());
        assert!(Resource::Docs.is_public());

        assert!(!Resource::Tasks.is_public());
        assert!(!Resource::Steps.is_public());
        assert!(!Resource::Dlq.is_public());
        assert!(!Resource::Templates.is_public());
        assert!(!Resource::System.is_public());
        assert!(!Resource::Worker.is_public());
    }

    #[test]
    fn test_resource_permission_prefix() {
        assert_eq!(Resource::Tasks.permission_prefix(), "tasks");
        assert_eq!(Resource::Steps.permission_prefix(), "steps");
        assert_eq!(Resource::Dlq.permission_prefix(), "dlq");
        assert_eq!(Resource::Templates.permission_prefix(), "templates");
        assert_eq!(Resource::System.permission_prefix(), "system");
        assert_eq!(Resource::Worker.permission_prefix(), "worker");
        assert_eq!(Resource::Health.permission_prefix(), "");
    }

    #[test]
    fn test_resource_display() {
        assert_eq!(format!("{}", Resource::Tasks), "tasks");
        assert_eq!(format!("{}", Resource::Health), "health");
    }

    // =========================================================================
    // Action Tests
    // =========================================================================

    #[test]
    fn test_action_display() {
        assert_eq!(format!("{}", Action::Create), "create");
        assert_eq!(format!("{}", Action::Read), "read");
        assert_eq!(format!("{}", Action::Cancel), "cancel");
        assert_eq!(format!("{}", Action::Resolve), "resolve");
        assert_eq!(format!("{}", Action::ConfigRead), "config_read");
    }

    // =========================================================================
    // ResourceAction Tests
    // =========================================================================

    #[test]
    fn test_public_resource_returns_none() {
        let health = ResourceAction::new(Resource::Health, Action::Read);
        assert!(health.required_permission().is_none());

        let metrics = ResourceAction::new(Resource::Metrics, Action::Read);
        assert!(metrics.required_permission().is_none());

        let docs = ResourceAction::new(Resource::Docs, Action::Read);
        assert!(docs.required_permission().is_none());
    }

    #[test]
    fn test_protected_resource_returns_permission() {
        let create_task = ResourceAction::new(Resource::Tasks, Action::Create);
        assert_eq!(
            create_task.required_permission(),
            Some(Permission::TasksCreate)
        );

        let list_tasks = ResourceAction::new(Resource::Tasks, Action::List);
        assert_eq!(
            list_tasks.required_permission(),
            Some(Permission::TasksList)
        );

        let cancel_task = ResourceAction::new(Resource::Tasks, Action::Cancel);
        assert_eq!(
            cancel_task.required_permission(),
            Some(Permission::TasksCancel)
        );
    }

    #[test]
    fn test_steps_permissions() {
        let read_step = ResourceAction::new(Resource::Steps, Action::Read);
        assert_eq!(read_step.required_permission(), Some(Permission::StepsRead));

        let list_steps = ResourceAction::new(Resource::Steps, Action::List);
        assert_eq!(
            list_steps.required_permission(),
            Some(Permission::StepsRead)
        );

        let resolve_step = ResourceAction::new(Resource::Steps, Action::Resolve);
        assert_eq!(
            resolve_step.required_permission(),
            Some(Permission::StepsResolve)
        );
    }

    #[test]
    fn test_dlq_permissions() {
        let read_dlq = ResourceAction::new(Resource::Dlq, Action::Read);
        assert_eq!(read_dlq.required_permission(), Some(Permission::DlqRead));

        let list_dlq = ResourceAction::new(Resource::Dlq, Action::List);
        assert_eq!(list_dlq.required_permission(), Some(Permission::DlqRead));

        let update_dlq = ResourceAction::new(Resource::Dlq, Action::Update);
        assert_eq!(
            update_dlq.required_permission(),
            Some(Permission::DlqUpdate)
        );

        let stats_dlq = ResourceAction::new(Resource::Dlq, Action::Stats);
        assert_eq!(stats_dlq.required_permission(), Some(Permission::DlqStats));
    }

    #[test]
    fn test_templates_permissions() {
        let read_template = ResourceAction::new(Resource::Templates, Action::Read);
        assert_eq!(
            read_template.required_permission(),
            Some(Permission::TemplatesRead)
        );

        let list_templates = ResourceAction::new(Resource::Templates, Action::List);
        assert_eq!(
            list_templates.required_permission(),
            Some(Permission::TemplatesRead)
        );

        let validate_template = ResourceAction::new(Resource::Templates, Action::Validate);
        assert_eq!(
            validate_template.required_permission(),
            Some(Permission::TemplatesValidate)
        );
    }

    #[test]
    fn test_system_permissions() {
        let config = ResourceAction::new(Resource::System, Action::ConfigRead);
        assert_eq!(
            config.required_permission(),
            Some(Permission::SystemConfigRead)
        );

        let handlers = ResourceAction::new(Resource::System, Action::HandlersRead);
        assert_eq!(
            handlers.required_permission(),
            Some(Permission::HandlersRead)
        );

        let analytics = ResourceAction::new(Resource::System, Action::AnalyticsRead);
        assert_eq!(
            analytics.required_permission(),
            Some(Permission::AnalyticsRead)
        );
    }

    #[test]
    fn test_worker_permissions() {
        let config = ResourceAction::new(Resource::Worker, Action::ConfigRead);
        assert_eq!(
            config.required_permission(),
            Some(Permission::WorkerConfigRead)
        );

        let read = ResourceAction::new(Resource::Worker, Action::Read);
        assert_eq!(
            read.required_permission(),
            Some(Permission::WorkerTemplatesRead)
        );

        let list = ResourceAction::new(Resource::Worker, Action::List);
        assert_eq!(
            list.required_permission(),
            Some(Permission::WorkerTemplatesRead)
        );
    }

    #[test]
    fn test_resource_action_display() {
        let ra = ResourceAction::new(Resource::Tasks, Action::Create);
        assert_eq!(format!("{}", ra), "tasks:create");

        let ra2 = ResourceAction::new(Resource::Dlq, Action::Stats);
        assert_eq!(format!("{}", ra2), "dlq:stats");
    }

    // =========================================================================
    // Comprehensive Permission Mapping Tests
    // =========================================================================

    /// Test that all expected resource+action combinations map to the correct permissions.
    /// This serves as a regression test for the permission mapping.
    #[test]
    fn test_all_permission_mappings() {
        let mappings = [
            // Tasks
            (Resource::Tasks, Action::Create, Permission::TasksCreate),
            (Resource::Tasks, Action::Read, Permission::TasksRead),
            (Resource::Tasks, Action::List, Permission::TasksList),
            (Resource::Tasks, Action::Cancel, Permission::TasksCancel),
            (
                Resource::Tasks,
                Action::ContextRead,
                Permission::TasksContextRead,
            ),
            // Steps
            (Resource::Steps, Action::Read, Permission::StepsRead),
            (Resource::Steps, Action::List, Permission::StepsRead),
            (Resource::Steps, Action::Resolve, Permission::StepsResolve),
            // DLQ
            (Resource::Dlq, Action::Read, Permission::DlqRead),
            (Resource::Dlq, Action::List, Permission::DlqRead),
            (Resource::Dlq, Action::Update, Permission::DlqUpdate),
            (Resource::Dlq, Action::Stats, Permission::DlqStats),
            // Templates
            (Resource::Templates, Action::Read, Permission::TemplatesRead),
            (Resource::Templates, Action::List, Permission::TemplatesRead),
            (
                Resource::Templates,
                Action::Validate,
                Permission::TemplatesValidate,
            ),
            // System
            (
                Resource::System,
                Action::ConfigRead,
                Permission::SystemConfigRead,
            ),
            (
                Resource::System,
                Action::HandlersRead,
                Permission::HandlersRead,
            ),
            (
                Resource::System,
                Action::AnalyticsRead,
                Permission::AnalyticsRead,
            ),
            // Worker
            (
                Resource::Worker,
                Action::ConfigRead,
                Permission::WorkerConfigRead,
            ),
            (
                Resource::Worker,
                Action::Read,
                Permission::WorkerTemplatesRead,
            ),
            (
                Resource::Worker,
                Action::List,
                Permission::WorkerTemplatesRead,
            ),
            (
                Resource::Worker,
                Action::Validate,
                Permission::TemplatesValidate,
            ),
        ];

        for (resource, action, expected_permission) in mappings {
            let ra = ResourceAction::new(resource, action);
            let actual = ra.required_permission();
            assert_eq!(
                actual,
                Some(expected_permission),
                "Mapping mismatch for {}:{} - expected {:?}, got {:?}",
                resource,
                action,
                expected_permission,
                actual
            );
        }
    }
}
