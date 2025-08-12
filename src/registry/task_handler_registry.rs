//! # Database-First Task Handler Registry
//!
//! Database-backed registry for distributed task handler resolution.
//!
//! ## Architecture
//!
//! **AGGRESSIVE DATABASE-FIRST**: Eliminates all in-memory storage, uses database models directly:
//! ```text
//! TaskRequest -> Database Query -> TaskNamespace + NamedTask + WorkerNamedTask -> HandlerMetadata
//! ```
//!
//! ## Key Features
//!
//! - **Database-First**: All handler resolution via database queries (no HashMap)
//! - **Distributed Ready**: Works identically in embedded and distributed deployments
//! - **Persistent State**: Handler registrations survive restarts and deployments
//! - **Worker Awareness**: Real-time knowledge of available workers per namespace
//! - **Event Integration**: Registration notifications via EventPublisher
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::TaskHandlerRegistry;
//! use tasker_core::models::core::task_request::TaskRequest;
//! use sqlx::PgPool;
//!
//! # async fn example(db_pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let registry = TaskHandlerRegistry::new(db_pool);
//!
//! // Handler resolution via database queries
//! let task_request = TaskRequest::new("order_processing".to_string(), "fulfillment".to_string());
//! let handler_metadata = registry.resolve_handler(&task_request).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, TaskerError};
use crate::events::EventPublisher;
use crate::models::core::{
    named_task::NamedTask, task_namespace::TaskNamespace, task_request::TaskRequest,
    task_template::TaskTemplate,
};
use crate::orchestration::types::HandlerMetadata;
use chrono::Utc;
use sqlx::PgPool;
use tracing::{debug, info};

/// Key for handler lookup in the registry
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerKey {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl HandlerKey {
    /// Create a new handler key with explicit values
    pub fn new(namespace: String, name: String, version: String) -> Self {
        Self {
            namespace,
            name,
            version,
        }
    }

    /// Create a handler key from a TaskRequest
    pub fn from_task_request(request: &TaskRequest) -> Self {
        Self {
            namespace: request.namespace.clone(),
            name: request.name.clone(),
            version: request.version.clone(),
        }
    }

    /// Convert to string key for storage
    pub fn key_string(&self) -> String {
        format!("{}/{}/{}", self.namespace, self.name, self.version)
    }
}

impl std::fmt::Display for HandlerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.namespace, self.name, self.version)
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_handlers: usize,
    pub total_step_handlers: usize,
    pub total_ffi_handlers: usize,
    pub namespaces: Vec<String>,
}

/// Database-first task handler registry for distributed orchestration
pub struct TaskHandlerRegistry {
    /// Database connection pool for persistent storage
    db_pool: PgPool,
    /// Event publisher for notifications
    event_publisher: Option<EventPublisher>,
}

impl TaskHandlerRegistry {
    /// Create a new database-backed task handler registry
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            event_publisher: None,
        }
    }

    /// Create a new registry with database connection and event publisher
    pub fn with_event_publisher(db_pool: PgPool, event_publisher: EventPublisher) -> Self {
        Self {
            db_pool,
            event_publisher: Some(event_publisher),
        }
    }

    pub async fn get_task_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<TaskTemplate> {
        let handler_metadata = self
            .get_task_template_from_registry(namespace, name, version)
            .await?;

        let task_template =
            serde_json::from_value(handler_metadata.config_schema.unwrap_or_default())?;
        Ok(task_template)
    }

    /// Resolve a handler from a TaskRequest using database queries
    pub async fn resolve_handler(&self, request: &TaskRequest) -> Result<HandlerMetadata> {
        debug!(
            namespace = &request.namespace,
            name = &request.name,
            version = &request.version,
            "🎯 DATABASE-FIRST: Resolving handler via database queries"
        );

        let handler_metadata = self
            .get_task_template_from_registry(&request.namespace, &request.name, &request.version)
            .await?;

        Ok(handler_metadata)
    }

    async fn get_task_template_from_registry(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<HandlerMetadata> {
        // 1. Find the task namespace
        let task_namespace = TaskNamespace::find_by_name(&self.db_pool, namespace)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to query namespace: {e}")))?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!("Namespace not found: {namespace}"))
            })?;

        // 2. Find the named task in that namespace
        let named_task = NamedTask::find_latest_by_name_namespace(
            &self.db_pool,
            name,
            task_namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to query named task: {e}")))?
        .ok_or_else(|| {
            TaskerError::ValidationError(format!("Task not found: {namespace}/{name}"))
        })?;

        info!(
            namespace = &namespace,
            name = &name,
            version = &version,
            "✅ DATABASE-FIRST: Handler resolved for task (pgmq architecture - no worker registration needed)"
        );

        let default_dependent_system = named_task.configuration.as_ref().and_then(|config| {
            config.get("default_dependent_system").and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or(Some("default".to_string()))
            })
        });
        let handler_class = named_task.configuration.as_ref().and_then(|config| {
            config.get("handler_class").and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or(Some("TaskerCore::TaskHandler::Base".to_string()))
            })
        });
        let handler_metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            default_dependent_system,
            handler_class: handler_class.unwrap_or("TaskerCore::TaskHandler::Base".to_string()),
            config_schema: named_task.configuration,
            registered_at: Utc::now(),
        };

        debug!("✅ DATABASE-FIRST: Handler resolved successfully (pgmq architecture)");

        Ok(handler_metadata)
    }
}

// Note: Default implementation removed - TaskHandlerRegistry now requires database pool

impl Clone for TaskHandlerRegistry {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            event_publisher: self.event_publisher.clone(),
        }
    }
}
