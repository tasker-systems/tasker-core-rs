//! TaskRequest System
//!
//! Provides the interface between user requests and Task creation.
//! TaskRequest represents the input specification for creating a Task instance
//! from a NamedTask template with user-provided context and options.

use bon::Builder;
use chrono::NaiveDateTime;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::named_task::NamedTask;
use super::task::{NewTask, Task};
use crate::errors::{TaskerError, TaskerResult};

/// TaskRequest represents an incoming request to create and execute a task
/// This is the primary routing input that identifies which handler should process the task
/// and contains all the information needed to create a Task instance from a NamedTask template
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Validate, Display, Builder)]
#[display(
    "TaskRequest(name: {}, namespace: {}, version: {}, initiator: {})",
    name,
    namespace,
    version,
    initiator
)]
pub struct TaskRequest {
    /// The name of the task to be performed (Rails: name)
    #[validate(length(min = 1, max = 255))]
    pub name: String,

    /// The namespace of the task to be performed (Rails: namespace, default: "default")
    #[validate(length(min = 1, max = 255))]
    #[builder(default = "default".to_string())]
    pub namespace: String,

    /// The version of the task to be performed (Rails: version, default: "0.1.0")
    #[validate(length(min = 1, max = 50))]
    #[builder(default = "0.1.0".to_string())]
    pub version: String,

    /// Context data required for task execution (Rails: context)
    #[builder(default = serde_json::json!({}))]
    pub context: serde_json::Value,

    /// The entity or system that initiated this task request (Rails: initiator)
    #[validate(length(min = 1, max = 255))]
    #[builder(default = "system".to_string())]
    pub initiator: String,

    /// The system from which this task originated (Rails: source_system)
    #[validate(length(min = 1, max = 255))]
    #[builder(default = "tasker-core".to_string())]
    pub source_system: String,

    /// The reason why this task was requested (Rails: reason)
    #[validate(length(min = 1, max = 500))]
    #[builder(default = "Task requested".to_string())]
    pub reason: String,

    /// Tags associated with this task for categorization or filtering (Rails: tags)
    #[builder(default)]
    pub tags: Vec<String>,

    // conditional workflows (decision points, deferred steps) not step-level bypass flags.
    /// Timestamp when the task was initially requested (Rails: requested_at)
    #[builder(default = chrono::Utc::now().naive_utc())]
    pub requested_at: NaiveDateTime,

    /// Custom options that override task template defaults (Rust extension)
    pub options: Option<HashMap<String, serde_json::Value>>,

    /// Priority for task execution (higher values = higher priority). Default: 0
    #[validate(range(min = -100, max = 100))]
    pub priority: Option<i32>,

    /// Correlation ID for distributed tracing (auto-generated if not provided)
    /// TAS-29: Enables end-to-end request tracking across orchestration and workers
    #[builder(default = Uuid::new_v4())]
    pub correlation_id: Uuid,

    /// Optional parent correlation ID for nested/chained workflow relationships
    /// TAS-29: Enables tracking of workflow hierarchies
    pub parent_correlation_id: Option<Uuid>,

    /// TAS-154: Optional caller-provided idempotency key.
    ///
    /// When provided, this key is used to compute the task's identity_hash,
    /// overriding the named task's default identity strategy.
    ///
    /// Use cases:
    /// - Override STRICT strategy with a custom key
    /// - Provide required key for CALLER_PROVIDED strategy
    /// - Time-bucketed keys for windowed deduplication (e.g., `"job-2026-01-20-09"`)
    ///
    /// The key is combined with the named_task_uuid to prevent collisions across
    /// different named tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Represents the resolved NamedTask and extracted options from a TaskRequest
#[derive(Debug, Clone)]
pub struct ResolvedTaskRequest {
    pub named_task: NamedTask,
    pub task_request: TaskRequest,
    pub resolved_context: serde_json::Value,
    pub resolved_options: HashMap<String, serde_json::Value>,
}

// Note: No Default implementation - use TaskRequest::builder() instead
// TaskRequest requires a name, so Default doesn't make semantic sense

impl TaskRequest {
    /// Create a new TaskRequest with minimal required fields
    pub fn new(name: String, namespace: String) -> Self {
        TaskRequest::builder()
            .name(name)
            .namespace(namespace)
            .build()
    }

    /// Add context data to the request
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    /// Add tags to the request
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add a single tag to the request
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set version for the request
    pub fn with_version(mut self, version: String) -> Self {
        self.version = version;
        self
    }

    /// Add initiator information
    pub fn with_initiator(mut self, initiator: String) -> Self {
        self.initiator = initiator;
        self
    }

    /// Add source system information
    pub fn with_source_system(mut self, source_system: String) -> Self {
        self.source_system = source_system;
        self
    }

    /// Add reason for task execution
    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = reason;
        self
    }

    /// Add custom options
    pub fn with_options(mut self, options: HashMap<String, serde_json::Value>) -> Self {
        self.options = Some(options);
        self
    }

    /// Set task priority (higher values = higher priority)
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set correlation ID for distributed tracing
    /// TAS-29: Allows external systems to provide their own correlation ID
    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = correlation_id;
        self
    }

    /// Set parent correlation ID for nested workflows
    /// TAS-29: Enables tracking of workflow hierarchies and dependencies
    pub fn with_parent_correlation_id(mut self, parent_correlation_id: Uuid) -> Self {
        self.parent_correlation_id = Some(parent_correlation_id);
        self
    }

    /// TAS-154: Set idempotency key for custom deduplication control
    ///
    /// When provided, this key overrides the named task's identity strategy
    /// and is used to compute the task's identity_hash.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Time-bucketed key for hourly deduplication
    /// let hour = chrono::Utc::now().format("%Y-%m-%d-%H");
    /// let request = TaskRequest::new("process-order".to_string(), "default".to_string())
    ///     .with_idempotency_key(format!("order-{}-{}", order_id, hour));
    /// ```
    pub fn with_idempotency_key(mut self, idempotency_key: String) -> Self {
        self.idempotency_key = Some(idempotency_key);
        self
    }

    /// Get the routing key for this task request (used by handler registry)
    /// Format: "namespace/name:version"
    pub fn routing_key(&self) -> String {
        format!("{}/{}:{}", self.namespace, self.name, self.version)
    }

    /// Get the handler namespace/name/version tuple for handler registry lookup
    pub fn handler_identifier(&self) -> (String, String, String) {
        (
            self.namespace.clone(),
            self.name.clone(),
            self.version.clone(),
        )
    }

    /// Resolve this TaskRequest to a NamedTask and extract configuration
    pub async fn resolve(&self, pool: &PgPool) -> TaskerResult<ResolvedTaskRequest> {
        // Step 1: Find the named task by name, version, and namespace
        let named_task = self.find_named_task(pool).await?;

        // Step 2: Extract and merge options from task template and request
        let resolved_options = self.extract_request_options(&named_task);

        // Step 3: Merge context from template defaults and request
        let resolved_context = self.merge_context(&named_task);

        Ok(ResolvedTaskRequest {
            named_task,
            task_request: self.clone(),
            resolved_context,
            resolved_options,
        })
    }

    /// Find the NamedTask that matches this request
    async fn find_named_task(&self, pool: &PgPool) -> TaskerResult<NamedTask> {
        // First, find the task namespace
        let namespace = sqlx::query!(
            "SELECT task_namespace_uuid FROM tasker.task_namespaces WHERE name = $1",
            self.namespace
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(e.to_string()))?
        .ok_or_else(|| {
            TaskerError::ValidationError(format!("Task namespace '{}' not found", self.namespace))
        })?;

        // Find the named task with exact version match
        let named_task = sqlx::query_as!(
            NamedTask,
            r#"
            SELECT named_task_uuid, name, version, description, task_namespace_uuid,
                   configuration, identity_strategy as "identity_strategy: _", created_at, updated_at
            FROM tasker.named_tasks
            WHERE name = $1 AND version = $2 AND task_namespace_uuid = $3
            "#,
            self.name,
            self.version,
            namespace.task_namespace_uuid
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(e.to_string()))?;

        named_task.ok_or_else(|| {
            TaskerError::ValidationError(format!(
                "Named task '{}/{}:{}' not found",
                self.namespace, self.name, self.version
            ))
        })
    }

    /// Extract and merge options from task template and request
    fn extract_request_options(
        &self,
        named_task: &NamedTask,
    ) -> HashMap<String, serde_json::Value> {
        let mut options = HashMap::new();

        // Start with template defaults
        if let Some(config) = &named_task.configuration {
            if let Some(default_options) = config.get("default_options") {
                if let Some(defaults) = default_options.as_object() {
                    for (key, value) in defaults {
                        options.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        // Override with request-specific options
        if let Some(request_options) = &self.options {
            for (key, value) in request_options {
                options.insert(key.clone(), value.clone());
            }
        }

        options
    }

    /// Merge context from template defaults and request
    fn merge_context(&self, named_task: &NamedTask) -> serde_json::Value {
        let mut context = serde_json::Map::new();

        // Start with template default context
        if let Some(config) = &named_task.configuration {
            if let Some(default_context) = config.get("default_context") {
                if let Some(defaults) = default_context.as_object() {
                    for (key, value) in defaults {
                        context.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        // Merge with request context
        if let Some(request_obj) = self.context.as_object() {
            for (key, value) in request_obj {
                context.insert(key.clone(), value.clone());
            }
        }

        serde_json::Value::Object(context)
    }
}

impl ResolvedTaskRequest {
    /// Convert this resolved request into a NewTask for creation
    ///
    /// # Errors
    ///
    /// Returns `Err` if the named task uses CallerProvided identity strategy
    /// but no idempotency_key was provided in the request.
    pub fn to_new_task(&self) -> Result<NewTask, String> {
        // TAS-154: Compute identity hash using the named task's strategy
        let identity_hash = Task::compute_identity_hash(
            self.named_task.identity_strategy,
            self.named_task.named_task_uuid,
            &Some(self.resolved_context.clone()),
            self.task_request.idempotency_key.as_deref(),
        )?;

        Ok(NewTask {
            named_task_uuid: self.named_task.named_task_uuid,
            requested_at: Some(self.task_request.requested_at),
            initiator: Some(self.task_request.initiator.clone()),
            source_system: Some(self.task_request.source_system.clone()),
            reason: Some(self.task_request.reason.clone()),

            tags: Some(serde_json::Value::Array(
                self.task_request
                    .tags
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )),
            context: Some(self.resolved_context.clone()),
            identity_hash,
            priority: self.task_request.priority,
            correlation_id: self.task_request.correlation_id,
            parent_correlation_id: self.task_request.parent_correlation_id,
        })
    }

    /// Create and save a Task from this resolved request
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The named task uses CallerProvided strategy but no idempotency_key was provided
    /// - Database operation fails
    pub async fn create_task(&self, pool: &PgPool) -> TaskerResult<Task> {
        let new_task = self.to_new_task().map_err(TaskerError::ValidationError)?;

        Task::create(pool, new_task)
            .await
            .map_err(|e| TaskerError::DatabaseError(e.to_string()))
    }

    /// Get the resolved options for this request
    pub fn get_request_options(&self) -> &HashMap<String, serde_json::Value> {
        &self.resolved_options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_request_creation() {
        let request = TaskRequest::new("order_processing".to_string(), "payments".to_string())
            .with_context(serde_json::json!({"order_id": 12345}))
            .with_initiator("user:alice".to_string())
            .with_reason("Process new order".to_string());

        assert_eq!(request.name, "order_processing");
        assert_eq!(request.namespace, "payments");
        assert_eq!(request.initiator, "user:alice");
        assert_eq!(request.reason, "Process new order");
    }

    #[test]
    fn test_routing_key_generation() {
        // Test with default version
        let request = TaskRequest::new("order_processing".to_string(), "payments".to_string());
        assert_eq!(request.routing_key(), "payments/order_processing:0.1.0");

        // Test with specific version
        let request_v2 = request.clone().with_version("2.1.0".to_string());
        assert_eq!(request_v2.routing_key(), "payments/order_processing:2.1.0");

        // Test handler identifier tuple
        let (namespace, name, version) = request.handler_identifier();
        assert_eq!(namespace, "payments");
        assert_eq!(name, "order_processing");
        assert_eq!(version, "0.1.0"); // Default version from builder
    }

    #[test]
    fn test_context_merging() {
        use super::super::identity_strategy::IdentityStrategy;

        let named_task = NamedTask {
            named_task_uuid: Uuid::now_v7(),
            name: "test_task".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            task_namespace_uuid: Uuid::now_v7(),
            configuration: Some(serde_json::json!({
                "default_context": {
                    "timeout": 300,
                    "retry_count": 3
                }
            })),
            identity_strategy: IdentityStrategy::Strict,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        let request = TaskRequest::new("test_task".to_string(), "test".to_string()).with_context(
            serde_json::json!({
                "order_id": 12345,
                "retry_count": 5  // Override default
            }),
        );

        let merged = request.merge_context(&named_task);

        assert_eq!(merged["timeout"], 300);
        assert_eq!(merged["retry_count"], 5); // Request overrides template
        assert_eq!(merged["order_id"], 12345);
    }
}
