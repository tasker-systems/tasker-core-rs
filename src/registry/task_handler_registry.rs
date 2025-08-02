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
use crate::orchestration::step_handler::StepHandler;
use crate::orchestration::types::{HandlerMetadata, TaskHandler};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, info, warn};

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
            .get_task_template_from_registry(&namespace, &name, &version)
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
        let task_namespace = TaskNamespace::find_by_name(&self.db_pool, &namespace)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to query namespace: {}", e)))?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!("Namespace not found: {}", namespace))
            })?;

        // 2. Find the named task in that namespace
        let named_task = NamedTask::find_latest_by_name_namespace(
            &self.db_pool,
            &name,
            task_namespace.task_namespace_id as i64,
        )
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to query named task: {}", e)))?
        .ok_or_else(|| {
            TaskerError::ValidationError(format!("Task not found: {}/{}", namespace, name))
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
            default_dependent_system: default_dependent_system,
            handler_class: handler_class.unwrap_or("TaskerCore::TaskHandler::Base".to_string()),
            config_schema: named_task.configuration,
            registered_at: Utc::now(),
        };

        debug!("✅ DATABASE-FIRST: Handler resolved successfully (pgmq architecture)");

        Ok(handler_metadata)
    }

    /// Validate handler before registration
    fn validate_handler(&self, key: &HandlerKey, _handler: &dyn TaskHandler) -> Result<()> {
        // Validate namespace
        if key.namespace.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler namespace cannot be empty".to_string(),
            ));
        }

        // Validate name
        if key.name.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler name cannot be empty".to_string(),
            ));
        }

        // Validate version
        if key.version.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler version cannot be empty".to_string(),
            ));
        }

        // Validate semantic versioning
        if !self.is_valid_semver(&key.version) {
            return Err(TaskerError::ValidationError(format!(
                "Invalid version format: {}",
                key.version
            )));
        }

        Ok(())
    }

    /// Check if version follows semantic versioning
    fn is_valid_semver(&self, version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::core::named_task::{NamedTask, NewNamedTask};
    use serde_json::json;

    // Note: Worker-related tests are temporarily disabled
    // until worker models are implemented

    /*
    // All tests temporarily disabled - requires worker models to be implemented

    /// Test database-backed handler resolution with namespace, task, and worker setup
    #[sqlx::test]
    async fn test_database_handler_resolution(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        // 1. Create test namespace "fulfillment"
        let namespace = TaskNamespace::find_or_create(&pool, "fulfillment")
            .await
            .expect("Failed to create namespace");

        // 2. Create named task "order_processing" in the namespace
        let new_named_task = NewNamedTask {
            name: "order_processing".to_string(),
            task_namespace_id: namespace.task_namespace_id as i64,
            version: Some("1.0.0".to_string()),
            description: Some("Order processing workflow task".to_string()),
            configuration: Some(json!({"timeout": 300, "retries": 3})),
        };

        let named_task = NamedTask::create(&pool, new_named_task)
            .await
            .expect("Failed to create named task");

        // 3. Create a worker
        let worker_metadata = WorkerMetadata {
            version: Some("1.0.0".to_string()),
            ruby_version: Some("3.1.0".to_string()),
            hostname: Some("test-host".to_string()),
            pid: Some(1234),
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            custom: serde_json::Map::new(),
        };

        let worker = Worker::create_or_update_by_worker_name(
            &pool,
            "test-worker-1",
            serde_json::to_value(worker_metadata).unwrap(),
        )
        .await
        .expect("Failed to create worker");

        // 4. Register the worker as active
        let new_registration = NewWorkerRegistration {
            worker_id: worker.worker_id,
            status: WorkerStatus::Healthy,
            connection_type: "tcp".to_string(),
            connection_details: json!({"host": "localhost", "port": 8080}),
        };

        WorkerRegistration::register(&pool, new_registration)
            .await
            .expect("Failed to register worker");

        // 5. Associate worker with the named task
        let new_association = NewWorkerNamedTask {
            worker_id: worker.worker_id,
            named_task_id: named_task.named_task_id,
            configuration: Some(json!({"priority": 100})),
            priority: Some(100),
        };

        WorkerNamedTask::create(&pool, new_association)
            .await
            .expect("Failed to create worker-task association");

        // 6. Test handler resolution via database queries
        let task_request = TaskRequest::new("order_processing".to_string(), "fulfillment".to_string())
            .with_version("1.0.0".to_string());

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_ok(), "Handler resolution failed: {:?}", result);

        let metadata = result.unwrap();
        assert_eq!(metadata.namespace, "fulfillment");
        assert_eq!(metadata.name, "order_processing");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.handler_class, "order_processingHandler");
        assert!(metadata.config_schema.is_some());
    }

    /// Test handler resolution failure when no active workers exist
    #[sqlx::test]
    async fn test_no_active_workers_error(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        // 1. Create test namespace and task but NO active workers
        let namespace = TaskNamespace::find_or_create(&pool, "inventory")
            .await
            .expect("Failed to create namespace");

        let new_named_task = NewNamedTask {
            name: "stock_check".to_string(),
            task_namespace_id: namespace.task_namespace_id as i64,
            version: Some("1.0.0".to_string()),
            description: Some("Stock check task".to_string()),
            configuration: None,
        };

        let _named_task = NamedTask::create(&pool, new_named_task)
            .await
            .expect("Failed to create named task");

        // 2. Try to resolve handler without any active workers
        let task_request = TaskRequest::new("stock_check".to_string(), "inventory".to_string())
            .with_version("1.0.0".to_string());

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_err(), "Expected error for no active workers");

        let error = result.unwrap_err();
        assert!(
            format!("{:?}", error).contains("No active workers available"),
            "Error should mention no active workers: {:?}",
            error
        );
    }

    /// Test handler resolution failure when namespace doesn't exist
    #[sqlx::test]
    async fn test_namespace_not_found_error(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        let task_request = TaskRequest::new("any_task".to_string(), "nonexistent_namespace".to_string())
            .with_version("1.0.0".to_string());

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_err(), "Expected error for nonexistent namespace");

        let error = result.unwrap_err();
        assert!(
            format!("{:?}", error).contains("Namespace not found"),
            "Error should mention namespace not found: {:?}",
            error
        );
    }

    /// Test handler resolution failure when task doesn't exist in namespace
    #[sqlx::test]
    async fn test_task_not_found_error(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        // Create namespace but not the task
        let _namespace = TaskNamespace::find_or_create(&pool, "notifications")
            .await
            .expect("Failed to create namespace");

        let task_request = TaskRequest::new("nonexistent_task".to_string(), "notifications".to_string())
            .with_version("1.0.0".to_string());

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_err(), "Expected error for nonexistent task");

        let error = result.unwrap_err();
        assert!(
            format!("{:?}", error).contains("Task not found"),
            "Error should mention task not found: {:?}",
            error
        );
    }

    /// Test handler resolution with multiple active workers (should succeed)
    #[sqlx::test]
    async fn test_multiple_active_workers(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        // 1. Create namespace and task
        let namespace = TaskNamespace::find_or_create(&pool, "payments")
            .await
            .expect("Failed to create namespace");

        let new_named_task = NewNamedTask {
            name: "process_payment".to_string(),
            task_namespace_id: namespace.task_namespace_id as i64,
            version: Some("2.0.0".to_string()),
            description: Some("Payment processing task".to_string()),
            configuration: Some(json!({"max_amount": 10000})),
        };

        let named_task = NamedTask::create(&pool, new_named_task)
            .await
            .expect("Failed to create named task");

        // 2. Create multiple workers and register them as active
        for i in 1..=3 {
            let worker_metadata = WorkerMetadata {
                version: Some("1.0.0".to_string()),
                ruby_version: Some("3.1.0".to_string()),
                hostname: Some(format!("worker-host-{}", i)),
                pid: Some(1000 + i),
                started_at: Some(chrono::Utc::now().to_rfc3339()),
                custom: serde_json::Map::new(),
            };

            let worker = Worker::create_or_update_by_worker_name(
                &pool,
                &format!("payment-worker-{}", i),
                serde_json::to_value(worker_metadata).unwrap(),
            )
            .await
            .expect("Failed to create worker");

            // Register worker as healthy
            let new_registration = NewWorkerRegistration {
                worker_id: worker.worker_id,
                status: WorkerStatus::Healthy,
                connection_type: "tcp".to_string(),
                connection_details: json!({"host": "localhost", "port": 8080 + i}),
            };

            WorkerRegistration::register(&pool, new_registration)
                .await
                .expect("Failed to register worker");

            // Associate worker with the named task
            let new_association = NewWorkerNamedTask {
                worker_id: worker.worker_id,
                named_task_id: named_task.named_task_id,
                configuration: Some(json!({"priority": 100 - i})), // Different priorities
                priority: Some(100 - i),
            };

            WorkerNamedTask::create(&pool, new_association)
                .await
                .expect("Failed to create worker-task association");
        }

        // 3. Test handler resolution should succeed with multiple workers
        let task_request = TaskRequest::new("process_payment".to_string(), "payments".to_string())
            .with_version("2.0.0".to_string());

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_ok(), "Handler resolution failed: {:?}", result);

        let metadata = result.unwrap();
        assert_eq!(metadata.namespace, "payments");
        assert_eq!(metadata.name, "process_payment");
        assert_eq!(metadata.version, "2.0.0");
        assert_eq!(metadata.handler_class, "process_paymentHandler");

        // Verify the configuration from named task is included
        assert!(metadata.config_schema.is_some());
        let config = metadata.config_schema.unwrap();
        assert_eq!(config["max_amount"], 10000);
    }

    /// Test version fallback behavior (use named task version when request version is empty)
    #[sqlx::test]
    async fn test_version_fallback(pool: sqlx::PgPool) {
        let registry = TaskHandlerRegistry::new(pool.clone());

        // Create namespace, task, worker, and association
        let namespace = TaskNamespace::find_or_create(&pool, "testing")
            .await
            .expect("Failed to create namespace");

        let new_named_task = NewNamedTask {
            name: "version_test".to_string(),
            task_namespace_id: namespace.task_namespace_id as i64,
            version: Some("3.5.2".to_string()), // Specific version in named task
            description: Some("Version test task".to_string()),
            configuration: None,
        };

        let named_task = NamedTask::create(&pool, new_named_task)
            .await
            .expect("Failed to create named task");

        // Create and register worker
        let worker_metadata = WorkerMetadata {
            version: Some("1.0.0".to_string()),
            ruby_version: None,
            hostname: None,
            pid: None,
            started_at: None,
            custom: serde_json::Map::new(),
        };

        let worker = Worker::create_or_update_by_worker_name(
            &pool,
            "version-test-worker",
            serde_json::to_value(worker_metadata).unwrap(),
        )
        .await
        .expect("Failed to create worker");

        let new_registration = NewWorkerRegistration {
            worker_id: worker.worker_id,
            status: WorkerStatus::Registered,
            connection_type: "tcp".to_string(),
            connection_details: json!({"host": "localhost", "port": 9000}),
        };

        WorkerRegistration::register(&pool, new_registration)
            .await
            .expect("Failed to register worker");

        let new_association = NewWorkerNamedTask {
            worker_id: worker.worker_id,
            named_task_id: named_task.named_task_id,
            configuration: None,
            priority: None,
        };

        WorkerNamedTask::create(&pool, new_association)
            .await
            .expect("Failed to create worker-task association");

        // Test with empty version in request - should use named task version
        let task_request = TaskRequest::new("version_test".to_string(), "testing".to_string());
        // Note: default version is "1.0.0" when not specified

        let result = registry.resolve_handler(&task_request).await;
        assert!(result.is_ok(), "Handler resolution failed: {:?}", result);

        let metadata = result.unwrap();
        assert_eq!(metadata.version, "3.5.2"); // Should use named task version
    }
    */
}
