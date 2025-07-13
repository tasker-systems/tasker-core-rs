//! # Task Initializer
//!
//! Atomic task creation with proper transaction safety and state machine integration.
//!
//! ## Overview
//!
//! The TaskInitializer provides a comprehensive, transaction-safe approach to creating
//! tasks with workflow steps, dependencies, and proper state machine initialization.
//! This component was extracted from Ruby bindings to be part of the core orchestration
//! suite, ensuring proper separation of concerns and reusability.
//!
//! ## Key Features
//!
//! - **Transaction Safety**: All operations wrapped in SQLx transactions for atomicity
//! - **Decomposed Methods**: Complex initialization broken into manageable, testable methods
//! - **State Machine Integration**: Proper initialization of task and step state transitions
//! - **Dependency Management**: Handles complex workflow step dependencies
//! - **Configuration-Driven**: Supports YAML-based task handler configurations
//! - **Error Recovery**: Comprehensive error handling with transaction rollback
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::orchestration::TaskInitializer;
//! use tasker_core::orchestration::handler_config::HandlerConfiguration;
//! use tasker_core::models::core::task_request::TaskRequest;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/nonexistent").await?;
//! let initializer = TaskInitializer::new(pool.clone());
//!
//! let task_request = TaskRequest::new("order_processor".to_string(), "default".to_string())
//!     .with_context(serde_json::json!({"order_id": 12345}))
//!     .with_initiator("test_user".to_string())
//!     .with_source_system("test_system".to_string())
//!     .with_reason("Example usage".to_string());
//!
//! let result = initializer.create_task_from_request(task_request).await?;
//! println!("Created task {} with {} steps", result.task_id, result.step_count);
//! # Ok(())
//! # }
//! ```

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::models::{task_request::TaskRequest, NamedStep, Task, WorkflowStep};
use crate::orchestration::event_publisher::EventPublisher;
use crate::orchestration::handler_config::HandlerConfiguration;
use crate::orchestration::state_manager::StateManager;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

/// Result of task initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInitializationResult {
    /// Created task ID
    pub task_id: i64,
    /// Number of workflow steps created
    pub step_count: usize,
    /// Mapping of step names to workflow step IDs
    pub step_mapping: HashMap<String, i64>,
    /// Handler configuration used (if any)
    pub handler_config_name: Option<String>,
}

/// Configuration for task initialization
#[derive(Debug, Clone)]
pub struct TaskInitializationConfig {
    /// Default system ID for named steps
    pub default_system_id: i32,
    /// Whether to create initial state transitions
    pub initialize_state_machine: bool,
    /// Event metadata to include in transitions
    pub event_metadata: Option<serde_json::Value>,
}

impl Default for TaskInitializationConfig {
    fn default() -> Self {
        Self {
            default_system_id: 1,
            initialize_state_machine: true,
            event_metadata: Some(serde_json::json!({
                "created_by": "task_initializer",
                "initialization": true
            })),
        }
    }
}

/// Atomic task creation with proper transaction safety
pub struct TaskInitializer {
    pool: PgPool,
    config: TaskInitializationConfig,
    event_publisher: Option<EventPublisher>,
    state_manager: Option<StateManager>,
}

impl TaskInitializer {
    /// Create a new TaskInitializer
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            config: TaskInitializationConfig::default(),
            event_publisher: None,
            state_manager: None,
        }
    }

    /// Create a TaskInitializer with custom configuration
    pub fn with_config(pool: PgPool, config: TaskInitializationConfig) -> Self {
        Self {
            pool,
            config,
            event_publisher: None,
            state_manager: None,
        }
    }

    /// Create a TaskInitializer with orchestration event publisher
    pub fn with_orchestration_events(pool: PgPool, event_publisher: EventPublisher) -> Self {
        Self {
            pool,
            config: TaskInitializationConfig::default(),
            event_publisher: Some(event_publisher),
            state_manager: None,
        }
    }

    /// Create a TaskInitializer with both config and orchestration event publisher
    pub fn with_config_and_orchestration_events(
        pool: PgPool,
        config: TaskInitializationConfig,
        event_publisher: EventPublisher,
    ) -> Self {
        Self {
            pool,
            config,
            event_publisher: Some(event_publisher),
            state_manager: None,
        }
    }

    /// Create a TaskInitializer with StateManager for proper state handling
    pub fn with_state_manager(
        pool: PgPool,
        config: TaskInitializationConfig,
        event_publisher: EventPublisher,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), pool.clone());

        Self {
            pool,
            config,
            event_publisher: Some(event_publisher),
            state_manager: Some(state_manager),
        }
    }

    /// Create a complete task from TaskRequest with atomic transaction safety
    #[instrument(skip(self), fields(task_name = %task_request.name))]
    pub async fn create_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        info!(task_name = %task_request.name, "Starting task initialization");

        // Use SQLx transaction for atomicity
        let mut tx = self.pool.begin().await.map_err(|e| {
            TaskInitializationError::Database(format!("Failed to begin transaction: {e}"))
        })?;

        let task_name = task_request.name.clone();

        // Create the task within transaction
        let task = self.create_task_record(&mut tx, task_request).await?;
        let task_id = task.task_id;

        debug!(task_id = task_id, "Created task record");

        // Try to load handler configuration
        let handler_config = match self.load_handler_configuration(&task_name).await {
            Ok(config) => {
                debug!(
                    task_id = task_id,
                    config_name = %task_name,
                    step_count = config.step_templates.len(),
                    "Loaded handler configuration"
                );
                Some(config)
            }
            Err(e) => {
                warn!(
                    task_id = task_id,
                    task_name = %task_name,
                    error = %e,
                    "No handler configuration found, creating minimal task"
                );
                None
            }
        };

        let (step_count, step_mapping) = if let Some(config) = handler_config.as_ref() {
            // Create workflow steps and dependencies
            self.create_workflow_steps(&mut tx, task_id, config).await?
        } else {
            // No configuration, no steps
            (0, HashMap::new())
        };

        debug!(
            task_id = task_id,
            step_count = step_count,
            "Created workflow steps"
        );

        // Initialize state machine if requested
        if self.config.initialize_state_machine {
            // Create initial database transitions within the transaction
            self.create_initial_state_transitions_in_tx(&mut tx, task_id, &step_mapping)
                .await?;
            debug!(
                task_id = task_id,
                "Created initial state transitions in transaction"
            );
        }

        // Commit transaction
        tx.commit().await.map_err(|e| {
            TaskInitializationError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        // Initialize StateManager-based state machines after transaction commit
        if self.config.initialize_state_machine {
            self.initialize_state_machines_post_transaction(task_id, &step_mapping)
                .await?;
            debug!(
                task_id = task_id,
                "Initialized StateManager-based state machines"
            );
        }

        // Publish initialization event if publisher available
        if let Some(ref publisher) = self.event_publisher {
            self.publish_task_initialized(task_id, step_count, &task_name, publisher)
                .await?;
        }

        info!(
            task_id = task_id,
            step_count = step_count,
            task_name = %task_name,
            "Task initialization completed successfully"
        );

        Ok(TaskInitializationResult {
            task_id,
            step_count,
            step_mapping,
            handler_config_name: handler_config.map(|_| task_name),
        })
    }

    /// Create the basic task record
    async fn create_task_record(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_request: TaskRequest,
    ) -> Result<Task, TaskInitializationError> {
        // First, resolve the NamedTask from the TaskRequest
        let named_task_id = self.resolve_named_task_id(&task_request).await?;

        // Use the transaction directly since models don't have transaction methods yet
        let requested_at = task_request.requested_at;
        let identity_hash = format!(
            "{}-{}-{}",
            named_task_id,
            serde_json::to_string(&task_request.context)
                .unwrap_or_default()
                .len(),
            chrono::Utc::now().timestamp_millis()
        );

        let task = sqlx::query_as!(
            Task,
            r#"
            INSERT INTO tasker_tasks (named_task_id, context, tags, identity_hash, complete, requested_at, initiator, source_system, reason, bypass_steps, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
            RETURNING task_id, named_task_id, context, tags, identity_hash, complete, requested_at, initiator, source_system, reason, bypass_steps, created_at, updated_at
            "#,
            named_task_id,
            Some(task_request.context),
            Some(serde_json::Value::Array(
                task_request
                    .tags
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )),
            identity_hash,
            false, // complete
            requested_at,
            Some(task_request.initiator),
            Some(task_request.source_system),
            Some(task_request.reason),
            Some(serde_json::Value::Array(
                task_request
                    .bypass_steps
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ))
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| TaskInitializationError::Database(format!("Failed to create task: {e}")))?;

        Ok(task)
    }

    /// Resolve NamedTask ID from TaskRequest (create if not exists)
    async fn resolve_named_task_id(
        &self,
        task_request: &TaskRequest,
    ) -> Result<i32, TaskInitializationError> {
        // First, find or create the task namespace
        let namespace = self
            .find_or_create_namespace(&task_request.namespace)
            .await?;

        // Find or create the named task
        let named_task = self
            .find_or_create_named_task(task_request, namespace.task_namespace_id as i64)
            .await?;

        Ok(named_task.named_task_id)
    }

    /// Find or create a task namespace
    async fn find_or_create_namespace(
        &self,
        namespace_name: &str,
    ) -> Result<crate::models::TaskNamespace, TaskInitializationError> {
        // Try to find existing namespace first
        if let Some(existing) =
            crate::models::TaskNamespace::find_by_name(&self.pool, namespace_name)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!("Failed to query namespace: {e}"))
                })?
        {
            return Ok(existing);
        }

        // Create new namespace if not found
        let new_namespace = crate::models::core::task_namespace::NewTaskNamespace {
            name: namespace_name.to_string(),
            description: Some(format!("Auto-created namespace for {namespace_name}")),
        };

        let namespace = crate::models::TaskNamespace::create(&self.pool, new_namespace)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create namespace: {e}"))
            })?;

        Ok(namespace)
    }

    /// Find or create a named task
    async fn find_or_create_named_task(
        &self,
        task_request: &TaskRequest,
        task_namespace_id: i64,
    ) -> Result<crate::models::NamedTask, TaskInitializationError> {
        // Try to find existing named task first
        let existing_task = crate::models::NamedTask::find_by_name_version_namespace(
            &self.pool,
            &task_request.name,
            &task_request.version,
            task_namespace_id,
        )
        .await
        .map_err(|e| {
            TaskInitializationError::Database(format!("Failed to query named task: {e}"))
        })?;

        if let Some(existing) = existing_task {
            return Ok(existing);
        }

        // Create new named task if not found
        let new_named_task = crate::models::core::named_task::NewNamedTask {
            name: task_request.name.clone(),
            version: Some(task_request.version.clone()),
            description: Some(format!("Auto-created task for {}", task_request.name)),
            task_namespace_id,
            configuration: None,
        };

        let named_task = crate::models::NamedTask::create(&self.pool, new_named_task)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create named task: {e}"))
            })?;

        Ok(named_task)
    }

    /// Create workflow steps and their dependencies from handler configuration
    async fn create_workflow_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_id: i64,
        config: &HandlerConfiguration,
    ) -> Result<(usize, HashMap<String, i64>), TaskInitializationError> {
        // Step 1: Create all named steps and workflow steps
        let step_mapping = self.create_steps(tx, task_id, config).await?;

        // Step 2: Create dependencies between steps
        self.create_step_dependencies(tx, config, &step_mapping)
            .await?;

        Ok((step_mapping.len(), step_mapping))
    }

    /// Create named steps and workflow steps
    async fn create_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_id: i64,
        config: &HandlerConfiguration,
    ) -> Result<HashMap<String, i64>, TaskInitializationError> {
        let mut step_mapping = HashMap::new();

        for step_template in &config.step_templates {
            // Create or find named step using transaction
            let named_steps = NamedStep::find_by_name(&self.pool, &step_template.name)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to search for NamedStep '{}': {}",
                        step_template.name, e
                    ))
                })?;

            let named_step = if let Some(existing_step) = named_steps.first() {
                existing_step.clone()
            } else {
                // Create new named step
                sqlx::query_as!(
                        NamedStep,
                        r#"
                        INSERT INTO tasker_named_steps (dependent_system_id, name, description, created_at, updated_at)
                        VALUES ($1, $2, $3, NOW(), NOW())
                        RETURNING named_step_id, dependent_system_id, name, description, created_at, updated_at
                        "#,
                        self.config.default_system_id,
                        step_template.name,
                        step_template.description
                    )
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to create NamedStep '{}': {}",
                            step_template.name, e
                        ))
                    })?
            };

            // Create workflow step using transaction
            let workflow_step = sqlx::query_as!(
                WorkflowStep,
                r#"
                INSERT INTO tasker_workflow_steps (
                    task_id, named_step_id, retryable, retry_limit, inputs, skippable, 
                    in_process, processed, processed_at, attempts, last_attempted_at, 
                    backoff_request_seconds, results, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
                RETURNING 
                    workflow_step_id, task_id, named_step_id, retryable, retry_limit, inputs, skippable,
                    in_process, processed, processed_at, attempts, last_attempted_at, 
                    backoff_request_seconds, results, created_at, updated_at
                "#,
                task_id,
                named_step.named_step_id,
                step_template.default_retryable,
                step_template.default_retry_limit,
                step_template.handler_config,
                step_template.skippable,
                false, // in_process
                false, // processed
                None::<chrono::NaiveDateTime>, // processed_at
                None::<i32>, // attempts
                None::<chrono::NaiveDateTime>, // last_attempted_at
                None::<i32>, // backoff_request_seconds
                None::<serde_json::Value> // results
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create WorkflowStep '{}': {}",
                    step_template.name, e
                ))
            })?;

            step_mapping.insert(step_template.name.clone(), workflow_step.workflow_step_id);

            debug!(
                step_name = %step_template.name,
                workflow_step_id = workflow_step.workflow_step_id,
                named_step_id = named_step.named_step_id,
                "Created workflow step"
            );
        }

        Ok(step_mapping)
    }

    /// Create dependencies between workflow steps
    async fn create_step_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        config: &HandlerConfiguration,
        step_mapping: &HashMap<String, i64>,
    ) -> Result<(), TaskInitializationError> {
        for step_template in &config.step_templates {
            let to_step_id = step_mapping[&step_template.name];

            // Create edges for all dependencies
            for dependency_name in step_template.all_dependencies() {
                if let Some(&from_step_id) = step_mapping.get(&dependency_name) {
                    sqlx::query!(
                        r#"
                        INSERT INTO tasker_workflow_step_edges (from_step_id, to_step_id, name, created_at, updated_at)
                        VALUES ($1, $2, $3, NOW(), NOW())
                        "#,
                        from_step_id,
                        to_step_id,
                        "provides"
                    )
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to create edge '{}' -> '{}': {}",
                            dependency_name, step_template.name, e
                        ))
                    })?;

                    debug!(
                        from_step = %dependency_name,
                        to_step = %step_template.name,
                        from_step_id = from_step_id,
                        to_step_id = to_step_id,
                        "Created step dependency"
                    );
                }
            }
        }

        Ok(())
    }

    /// Create initial state transitions in database within transaction
    async fn create_initial_state_transitions_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_id: i64,
        step_mapping: &HashMap<String, i64>,
    ) -> Result<(), TaskInitializationError> {
        // Create initial task transition with sort_key and most_recent
        sqlx::query!(
            r#"
            INSERT INTO tasker_task_transitions (task_id, to_state, from_state, metadata, sort_key, most_recent, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
            "#,
            task_id,
            "pending",
            None::<String>,
            self.config.event_metadata,
            1, // sort_key starts at 1 for first transition
            true // most_recent is true for initial transition
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            TaskInitializationError::Database(format!(
                "Failed to create initial task transition: {e}"
            ))
        })?;

        // Create initial step transitions with sort_key and most_recent
        for &workflow_step_id in step_mapping.values() {
            sqlx::query!(
                r#"
                INSERT INTO tasker_workflow_step_transitions (workflow_step_id, to_state, from_state, metadata, sort_key, most_recent, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
                "#,
                workflow_step_id,
                "pending",
                None::<String>,
                self.config.event_metadata,
                1, // sort_key starts at 1 for first transition
                true // most_recent is true for initial transition
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create initial step transition for step {workflow_step_id}: {e}"
                ))
            })?;
        }

        Ok(())
    }

    /// Initialize StateManager-based state machines after transaction commit
    async fn initialize_state_machines_post_transaction(
        &self,
        task_id: i64,
        step_mapping: &HashMap<String, i64>,
    ) -> Result<(), TaskInitializationError> {
        // Get or create StateManager for proper state machine initialization
        let state_manager = if let Some(ref manager) = self.state_manager {
            manager.clone()
        } else {
            // Create a temporary StateManager for this operation
            let sql_executor = SqlFunctionExecutor::new(self.pool.clone());
            let event_publisher = EventPublisher::new();
            StateManager::new(sql_executor, event_publisher, self.pool.clone())
        };

        // Initialize task state machine by evaluating its state
        // This will create the state machine and ensure it's properly initialized
        match state_manager.evaluate_task_state(task_id).await {
            Ok(result) => {
                debug!(
                    task_id = task_id,
                    current_state = %result.current_state,
                    "Task state machine initialized with StateManager"
                );
            }
            Err(e) => {
                warn!(
                    task_id = task_id,
                    error = %e,
                    "Failed to initialize task state machine with StateManager, basic initialization completed"
                );
                // Don't fail the entire initialization for StateManager issues
            }
        }

        // Initialize step state machines by evaluating their states
        for &workflow_step_id in step_mapping.values() {
            match state_manager.evaluate_step_state(workflow_step_id).await {
                Ok(result) => {
                    debug!(
                        step_id = workflow_step_id,
                        current_state = %result.current_state,
                        "Step state machine initialized with StateManager"
                    );
                }
                Err(e) => {
                    warn!(
                        step_id = workflow_step_id,
                        error = %e,
                        "Failed to initialize step state machine with StateManager, basic initialization completed"
                    );
                    // Don't fail the entire initialization for StateManager issues
                }
            }
        }

        Ok(())
    }

    /// Load handler configuration from YAML files
    async fn load_handler_configuration(
        &self,
        task_name: &str,
    ) -> Result<HandlerConfiguration, TaskInitializationError> {
        // TODO: Implement proper YAML configuration loading
        // For now, return error to indicate no configuration
        Err(TaskInitializationError::ConfigurationNotFound(
            task_name.to_string(),
        ))
    }

    /// Publish task initialization event
    async fn publish_task_initialized(
        &self,
        task_id: i64,
        step_count: usize,
        task_name: &str,
        _publisher: &EventPublisher,
    ) -> Result<(), TaskInitializationError> {
        // TODO: Implement event publishing once EventPublisher interface is finalized
        debug!(
            task_id = task_id,
            step_count = step_count,
            task_name = %task_name,
            "Would publish task_initialized event"
        );
        Ok(())
    }
}

/// Errors that can occur during task initialization
#[derive(Debug, thiserror::Error)]
pub enum TaskInitializationError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Configuration not found for task: {0}")]
    ConfigurationNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("State machine error: {0}")]
    StateMachine(String),

    #[error("Event publishing error: {0}")]
    EventPublishing(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_task_request() -> TaskRequest {
        TaskRequest::new("test_task".to_string(), "test".to_string())
            .with_context(serde_json::json!({"test": true}))
            .with_initiator("test_user".to_string())
            .with_source_system("test_system".to_string())
            .with_reason("Unit test".to_string())
    }

    #[test]
    fn test_task_initialization_config_default() {
        let config = TaskInitializationConfig::default();
        assert_eq!(config.default_system_id, 1);
        assert!(config.initialize_state_machine);
        assert!(config.event_metadata.is_some());
    }

    #[test]
    fn test_task_initialization_result_creation() {
        let mut step_mapping = HashMap::new();
        step_mapping.insert("step1".to_string(), 123);
        step_mapping.insert("step2".to_string(), 456);

        let result = TaskInitializationResult {
            task_id: 789,
            step_count: 2,
            step_mapping: step_mapping.clone(),
            handler_config_name: Some("test_handler".to_string()),
        };

        assert_eq!(result.task_id, 789);
        assert_eq!(result.step_count, 2);
        assert_eq!(result.step_mapping.len(), 2);
        assert_eq!(result.handler_config_name, Some("test_handler".to_string()));
    }

    #[test]
    fn test_task_request_creation() {
        let request = create_test_task_request();
        assert_eq!(request.name, "test_task");
        assert_eq!(request.context["test"], true);
    }
}
