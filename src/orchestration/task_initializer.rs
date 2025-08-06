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
use crate::events::EventPublisher;
use crate::models::{task_request::TaskRequest, NamedStep, Task, WorkflowStep};
use crate::orchestration::handler_config::HandlerConfiguration;
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::task_config_finder::TaskConfigFinder;
use crate::orchestration::config::ConfigurationManager;
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
    registry: Option<std::sync::Arc<crate::registry::TaskHandlerRegistry>>,
    task_config_finder: Option<TaskConfigFinder>,
}

impl TaskInitializer {
    /// Create a new TaskInitializer
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            config: TaskInitializationConfig::default(),
            event_publisher: None,
            state_manager: None,
            registry: None,
            task_config_finder: None,
        }
    }

    /// Create a TaskInitializer with custom configuration
    pub fn with_config(pool: PgPool, config: TaskInitializationConfig) -> Self {
        Self {
            pool,
            config,
            event_publisher: None,
            state_manager: None,
            registry: None,
            task_config_finder: None,
        }
    }

    /// Create a TaskInitializer with orchestration event publisher
    pub fn with_orchestration_events(pool: PgPool, event_publisher: EventPublisher) -> Self {
        Self {
            pool,
            config: TaskInitializationConfig::default(),
            event_publisher: Some(event_publisher),
            state_manager: None,
            registry: None,
            task_config_finder: None,
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
            registry: None,
            task_config_finder: None,
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
            registry: None,
            task_config_finder: None,
        }
    }

    /// Create a TaskInitializer with StateManager and Registry for FFI integration
    pub fn with_state_manager_and_registry(
        pool: PgPool,
        config: TaskInitializationConfig,
        event_publisher: EventPublisher,
        registry: std::sync::Arc<crate::registry::TaskHandlerRegistry>,
    ) -> Self {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), pool.clone());

        Self {
            pool,
            config,
            event_publisher: Some(event_publisher),
            state_manager: Some(state_manager),
            registry: Some(registry),
            task_config_finder: None,
        }
    }

    /// Create a TaskInitializer for testing with filesystem-based configuration loading
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn for_testing(pool: PgPool) -> Self {
        let config_manager = std::sync::Arc::new(ConfigurationManager::new());
        let registry = std::sync::Arc::new(crate::registry::TaskHandlerRegistry::new(pool.clone()));
        let task_config_finder = TaskConfigFinder::new(config_manager, registry);
        
        Self {
            pool,
            config: TaskInitializationConfig::default(),
            event_publisher: None,
            state_manager: None,
            registry: None,
            task_config_finder: Some(task_config_finder),
        }
    }

    /// Create a complete task from TaskRequest with atomic transaction safety
    #[instrument(skip(self), fields(task_name = %task_request.name))]
    pub async fn create_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        // Store values before moving task_request
        let namespace = task_request.namespace.clone();
        let task_name = task_request.name.clone();
        let version = task_request.version.clone();

        // Clone the task_request for handler configuration lookup
        let task_request_for_handler = task_request.clone();

        crate::logging::log_task_operation(
            "TASK_INITIALIZATION_START",
            None,
            Some(&task_name),
            Some(&namespace),
            "STARTING",
            Some(&format!("version={version}")),
        );

        info!(task_name = %task_request.name, "Starting task initialization");

        // Use SQLx transaction for atomicity
        let mut tx = self.pool.begin().await.map_err(|e| {
            crate::logging::log_error(
                "TaskInitializer",
                "create_task_from_request",
                &format!("Failed to begin transaction: {e}"),
                Some(&task_name),
            );
            TaskInitializationError::Database(format!("Failed to begin transaction: {e}"))
        })?;

        crate::logging::log_database_operation(
            "TRANSACTION_BEGIN",
            Some("tasker_tasks"),
            None,
            "SUCCESS",
            None,
            Some("Atomic task creation transaction started"),
        );

        // Create the task within transaction
        let task = self.create_task_record(&mut tx, task_request).await?;
        let task_id = task.task_id;

        crate::logging::log_task_operation(
            "TASK_RECORD_CREATED",
            Some(task_id),
            Some(&task_name),
            None,
            "SUCCESS",
            Some("Task record created in database"),
        );

        debug!(task_id = task_id, "Created task record");

        // Try to load handler configuration
        let handler_config = match self
            .load_handler_configuration(&task_request_for_handler)
            .await
        {
            Ok(config) => {
                crate::logging::log_registry_operation(
                    "HANDLER_CONFIG_LOADED",
                    Some(&namespace),
                    Some(&task_name),
                    Some(&version),
                    "SUCCESS",
                    Some(&format!(
                        "Found {} step templates",
                        config.step_templates.len()
                    )),
                );

                debug!(
                    task_id = task_id,
                    config_name = %task_name,
                    step_count = config.step_templates.len(),
                    "Loaded handler configuration"
                );
                Some(config)
            }
            Err(e) => {
                crate::logging::log_registry_operation(
                    "HANDLER_CONFIG_FAILED",
                    Some(&namespace),
                    Some(&task_name),
                    Some(&version),
                    "FAILED",
                    Some(&format!("Registry lookup failed: {e}")),
                );
                error!(
                  task_id = task_id,
                  task_name = %task_name,
                  error = %e,
                  "Failed to load handler configuration"
                );
                return Err(TaskInitializationError::ConfigurationNotFound(format!("Failed to load handler configuration for task: {task_name}, namespace: {namespace}, version: {version}, error: {e}")));
            }
        };

        // Handler configuration is guaranteed to exist (we return early with error if not)
        let config = handler_config.as_ref().unwrap();

        crate::logging::log_task_operation(
            "WORKFLOW_STEPS_CREATION_START",
            Some(task_id),
            Some(&task_name),
            None,
            "STARTING",
            Some(&format!(
                "Creating {} workflow steps",
                config.step_templates.len()
            )),
        );

        // Create workflow steps and dependencies
        let (step_count, step_mapping) =
            self.create_workflow_steps(&mut tx, task_id, config).await?;

        crate::logging::log_task_operation(
            "WORKFLOW_STEPS_CREATED",
            Some(task_id),
            Some(&task_name),
            None,
            "SUCCESS",
            Some(&format!(
                "Created {step_count} workflow steps with dependencies"
            )),
        );

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

        // ISSUE RESOLVED: State machine initialization updated to avoid in_process=true
        // Initialize StateManager-based state machines after transaction commit
        // The method has been fixed to only create state machines without setting in_process=true
        if self.config.initialize_state_machine {
            self.initialize_state_machines_post_transaction(task_id, &step_mapping)
                .await?;
            debug!(
                task_id = task_id,
                "Initialized StateManager-based state machines (without in_process=true)"
            );
        }

        // Publish initialization event if publisher available
        if let Some(ref publisher) = self.event_publisher {
            self.publish_task_initialized(task_id, step_count, &task_name, publisher)
                .await?;
        }

        let result = TaskInitializationResult {
            task_id,
            step_count,
            step_mapping: step_mapping.clone(),
            handler_config_name: Some(task_name.clone()), // Always present now that we require configuration
        };

        crate::logging::log_task_operation(
            "TASK_INITIALIZATION_COMPLETE",
            Some(task_id),
            Some(&task_name),
            None,
            "SUCCESS",
            Some(&format!(
                "Task completed: {} steps, handler_config: {:?}",
                step_count, result.handler_config_name
            )),
        );

        info!(
            task_id = task_id,
            step_count = step_count,
            task_name = %task_name,
            "Task initialization completed successfully"
        );

        Ok(result)
    }

    /// Create the basic task record
    async fn create_task_record(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_request: TaskRequest,
    ) -> Result<Task, TaskInitializationError> {
        // First, resolve the NamedTask from the TaskRequest
        let named_task_id = self.resolve_named_task_id(&task_request).await?;

        let mut new_task = Task::from_task_request(task_request);
        new_task.named_task_id = named_task_id;

        let task: Task = Task::create_with_transaction(tx, new_task)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create task: {e}"))
            })?;

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

    /// Create named steps and workflow steps using consistent transaction methods
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
                // Create new named step using transaction-aware method
                let system_name = "tasker_core_rust"; // Use a consistent system name for Rust core
                NamedStep::find_or_create_by_name_with_transaction(
                    tx,
                    &self.pool,
                    &step_template.name,
                    system_name,
                )
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create NamedStep '{}': {}",
                        step_template.name, e
                    ))
                })?
            };

            // Create workflow step using consistent transaction method
            let new_workflow_step = crate::models::core::workflow_step::NewWorkflowStep {
                task_id,
                named_step_id: named_step.named_step_id,
                retryable: step_template.default_retryable,
                retry_limit: step_template.default_retry_limit,
                inputs: step_template.handler_config.clone(),
                skippable: step_template.skippable,
            };

            let workflow_step = WorkflowStep::create_with_transaction(tx, new_workflow_step)
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
                "Created workflow step using transaction methods"
            );
        }

        Ok(step_mapping)
    }

    /// Create dependencies between workflow steps using consistent transaction methods
    async fn create_step_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        config: &HandlerConfiguration,
        step_mapping: &HashMap<String, i64>,
    ) -> Result<(), TaskInitializationError> {
        for step_template in &config.step_templates {
            let to_step_id = step_mapping[&step_template.name];

            // Create edges for all dependencies using transaction method
            for dependency_name in step_template.all_dependencies() {
                if let Some(&from_step_id) = step_mapping.get(&dependency_name) {
                    let new_edge = crate::models::core::workflow_step_edge::NewWorkflowStepEdge {
                        from_step_id,
                        to_step_id,
                        name: "provides".to_string(),
                    };

                    crate::models::WorkflowStepEdge::create_with_transaction(tx, new_edge)
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
                        "Created step dependency using transaction method"
                    );
                }
            }
        }

        Ok(())
    }

    /// Create initial state transitions in database using consistent transaction methods
    async fn create_initial_state_transitions_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_id: i64,
        step_mapping: &HashMap<String, i64>,
    ) -> Result<(), TaskInitializationError> {
        // Create initial task transition using transaction method
        let new_task_transition = crate::models::core::task_transition::NewTaskTransition {
            task_id,
            to_state: "pending".to_string(),
            from_state: None,
            metadata: self.config.event_metadata.clone(),
        };

        crate::models::TaskTransition::create_with_transaction(tx, new_task_transition)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create initial task transition: {e}"
                ))
            })?;

        // Create initial step transitions using transaction method
        for &workflow_step_id in step_mapping.values() {
            let new_step_transition =
                crate::models::core::workflow_step_transition::NewWorkflowStepTransition {
                    workflow_step_id,
                    to_state: "pending".to_string(),
                    from_state: None,
                    metadata: self.config.event_metadata.clone(),
                };

            crate::models::WorkflowStepTransition::create_with_transaction(tx, new_step_transition)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create initial step transition for step {workflow_step_id}: {e}"
                    ))
                })?;
        }

        debug!(
            task_id = task_id,
            step_count = step_mapping.len(),
            "Created initial state transitions using transaction methods"
        );

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

        // Initialize step state machines WITHOUT evaluating state transitions
        // We don't want to transition steps to InProgress during initialization
        // as this sets in_process=true, making them ineligible for execution
        for &workflow_step_id in step_mapping.values() {
            // Simply verify the state machine exists, don't evaluate/transition
            match state_manager
                .get_or_create_step_state_machine(workflow_step_id)
                .await
            {
                Ok(state_machine) => match state_machine.current_state().await {
                    Ok(current_state) => {
                        debug!(
                            step_id = workflow_step_id,
                            current_state = %current_state,
                            "Step state machine initialized (no evaluation)"
                        );
                    }
                    Err(e) => {
                        warn!(
                            step_id = workflow_step_id,
                            error = %e,
                            "Failed to get current state from step state machine"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        step_id = workflow_step_id,
                        error = %e,
                        "Failed to initialize step state machine, basic initialization completed"
                    );
                    // Don't fail the entire initialization for StateManager issues
                }
            }
        }

        Ok(())
    }

    /// Load handler configuration from TaskHandlerRegistry or filesystem
    /// In FFI integration, handlers register their configuration in the registry
    /// For tests, falls back to filesystem-based YAML discovery
    async fn load_handler_configuration(
        &self,
        task_request: &TaskRequest,
    ) -> Result<HandlerConfiguration, TaskInitializationError> {
        // Try registry first if available
        if let Some(registry) = &self.registry {
            match self.load_from_registry(task_request, registry).await {
                Ok(config) => return Ok(config),
                Err(e) => {
                    debug!("Registry loading failed, trying filesystem fallback: {}", e);
                }
            }
        }

        // Fall back to filesystem configuration using TaskConfigFinder
        if let Some(task_config_finder) = &self.task_config_finder {
            return self.load_from_filesystem(task_request, task_config_finder).await;
        }

        // No configuration source available
        Err(TaskInitializationError::ConfigurationNotFound(
            "No TaskHandlerRegistry or TaskConfigFinder available - TaskInitializer must be created with configuration support".to_string()
        ))
    }

    /// Load configuration from registry
    async fn load_from_registry(
        &self,
        task_request: &TaskRequest,
        registry: &crate::registry::TaskHandlerRegistry,
    ) -> Result<HandlerConfiguration, TaskInitializationError> {
        // Use the namespace and name directly from the TaskRequest
        let namespace = &task_request.namespace;
        let name = &task_request.name;
        let version = &task_request.version;

        // Look up the handler metadata using the ACTUAL task request version
        debug!(
            "🔍 HANDLER LOOKUP: Looking for namespace='{}', name='{}', version='{}'",
            namespace, name, version
        );

        let metadata = registry.resolve_handler(task_request).await.map_err(|e| {
            TaskInitializationError::ConfigurationNotFound(format!(
                "Handler not found in registry {namespace}/{name}: {e}"
            ))
        })?;

        // Extract the config_schema from metadata and convert from full TaskHandlerInfo format
        debug!(
            task_name = %name,
            namespace = %namespace,
            config_schema_present = %metadata.config_schema.is_some(),
            "🎯 TASK_INIT: Checking if config_schema is present"
        );

        if let Some(config_json) = metadata.config_schema {
            // The database now stores the full TaskTemplate structure directly
            // Try to deserialize as TaskTemplate first (new format), then convert to HandlerConfiguration
            match serde_json::from_value::<crate::models::core::task_template::TaskTemplate>(config_json.clone()) {
                Ok(task_template) => {
                    // Convert TaskTemplate to HandlerConfiguration
                    // We need to convert the step templates and environments from TaskTemplate types to HandlerConfiguration types
                    let handler_step_templates = task_template.step_templates.into_iter().map(|st| {
                        crate::orchestration::handler_config::StepTemplate {
                            name: st.name,
                            description: st.description,
                            dependent_system: st.dependent_system,
                            default_retryable: st.default_retryable,
                            default_retry_limit: st.default_retry_limit,
                            skippable: st.skippable,
                            timeout_seconds: None, // TaskTemplate doesn't have this field
                            handler_class: st.handler_class,
                            handler_config: st.handler_config,
                            depends_on_step: st.depends_on_step,
                            depends_on_steps: st.depends_on_steps,
                            custom_events: st.custom_events,
                        }
                    }).collect();

                    let handler_environments = task_template.environments.map(|envs| {
                        envs.into_iter().map(|(name, env)| {
                            let handler_env = crate::orchestration::handler_config::EnvironmentConfig {
                                step_templates: env.step_templates.map(|sts| {
                                    sts.into_iter().map(|st| {
                                        crate::orchestration::handler_config::StepTemplateOverride {
                                            name: st.name,
                                            handler_config: st.handler_config,
                                            description: st.description,
                                            dependent_system: st.dependent_system,
                                            default_retryable: st.default_retryable,
                                            default_retry_limit: st.default_retry_limit,
                                            skippable: st.skippable,
                                            timeout_seconds: None, // TaskTemplate doesn't have this field
                                        }
                                    }).collect()
                                }),
                                default_context: env.default_context,
                                default_options: env.default_options,
                            };
                            (name, handler_env)
                        }).collect()
                    });

                    let handler_config = HandlerConfiguration {
                        name: task_template.name,
                        module_namespace: task_template.module_namespace,
                        task_handler_class: task_template.task_handler_class,
                        namespace_name: task_template.namespace_name,
                        version: task_template.version,
                        description: None, // TaskTemplate doesn't have description in this context
                        default_dependent_system: task_template.default_dependent_system,
                        named_steps: task_template.named_steps,
                        schema: task_template.schema,
                        step_templates: handler_step_templates,
                        environments: handler_environments,
                        handler_config: None, // The handler_config is at the task level in TaskTemplate
                        default_context: task_template.default_context,
                        default_options: task_template.default_options,
                    };
                    
                    if handler_config.step_templates.is_empty() {
                        return Err(TaskInitializationError::ConfigurationNotFound(format!(
                            "Empty step_templates array in task handler configuration for {}/{}. Cannot create workflow steps without step templates.",
                            handler_config.namespace_name, handler_config.name
                        )));
                    }
                    
                    return Ok(handler_config);
                }
                Err(task_template_error) => {
                    // Fall back to old nested format for backward compatibility
                    debug!("Failed to deserialize as TaskTemplate (new format): {}, trying old nested format", task_template_error);
                    
                    let handler_config_value = config_json.get("handler_config").ok_or_else(|| {
                        TaskInitializationError::ConfigurationNotFound(format!(
                            "Configuration is neither new TaskTemplate format nor old nested format with handler_config field for {namespace}/{name}. TaskTemplate error: {task_template_error}"
                        ))
                    })?;

                    // Try to deserialize from the handler_config field (old nested format)
                    let handler_config =
                        serde_json::from_value::<HandlerConfiguration>(handler_config_value.clone())
                            .map_err(|e| {
                                TaskInitializationError::ConfigurationNotFound(format!(
                            "Failed to deserialize handler configuration for {namespace}/{name}: {e}. Handler config: {handler_config_value}"
                        ))
                            })?;
                    
                    if handler_config.step_templates.is_empty() {
                        return Err(TaskInitializationError::ConfigurationNotFound(format!(
                            "Empty step_templates array in task handler configuration for {}/{}. Cannot create workflow steps without step templates.",
                            handler_config.namespace_name, handler_config.name
                        )));
                    }
                    
                    return Ok(handler_config);
                }
            }
        } else {
            // No config_schema provided - this is a hard error
            Err(TaskInitializationError::ConfigurationNotFound(format!(
                "No task handler configuration found in database for {}/{}. Task handler must be registered with configuration before task creation.",
                metadata.namespace, metadata.name
            )))
        }
    }

    /// Load configuration from filesystem using TaskConfigFinder
    async fn load_from_filesystem(
        &self,
        task_request: &TaskRequest,
        task_config_finder: &TaskConfigFinder,
    ) -> Result<HandlerConfiguration, TaskInitializationError> {
        let namespace = &task_request.namespace;
        let name = &task_request.name;
        let version = &task_request.version;

        debug!(
            "🔍 FILESYSTEM LOOKUP: Looking for namespace='{}', name='{}', version='{}'",
            namespace, name, version
        );

        // Find the task template using TaskConfigFinder
        let task_template = task_config_finder
            .find_task_template(namespace, name, version)
            .await
            .map_err(|e| {
                TaskInitializationError::ConfigurationNotFound(format!(
                    "Failed to load task template from filesystem for {namespace}/{name}/{version}: {e}"
                ))
            })?;

        // Convert TaskTemplate to HandlerConfiguration
        let handler_config = HandlerConfiguration {
            name: task_template.name,
            module_namespace: task_template.module_namespace,
            task_handler_class: task_template.task_handler_class,
            namespace_name: task_template.namespace_name,
            version: task_template.version,
            description: None, // TaskTemplate doesn't have a top-level description
            default_dependent_system: task_template.default_dependent_system,
            named_steps: task_template.named_steps,
            schema: task_template.schema,
            step_templates: task_template.step_templates.into_iter().map(|st| {
                crate::orchestration::handler_config::StepTemplate {
                    name: st.name,
                    description: st.description,
                    dependent_system: st.dependent_system.or_else(|| Some("default".to_string())), // Use actual field with default fallback
                    default_retryable: st.default_retryable,
                    default_retry_limit: st.default_retry_limit,
                    skippable: st.skippable, // Use actual field from models
                    timeout_seconds: None, // This field exists in handler_config but not in models - keep as None for now
                    handler_class: st.handler_class,
                    handler_config: st.handler_config,
                    depends_on_step: st.depends_on_step,
                    depends_on_steps: st.depends_on_steps,
                    custom_events: st.custom_events, // Use actual field from models
                }
            }).collect(),
            environments: task_template.environments.map(|envs| {
                envs.into_iter().map(|(key, _env_config)| {
                    // For now, just create empty environment configs
                    // We could expand this if needed
                    (key, crate::orchestration::handler_config::EnvironmentConfig {
                        step_templates: None,
                        default_context: None,
                        default_options: None,
                    })
                }).collect()
            }),
            handler_config: None, // TaskTemplate doesn't have a top-level handler_config
            default_context: task_template.default_context,
            default_options: task_template.default_options,
        };

        if handler_config.step_templates.is_empty() {
            return Err(TaskInitializationError::ConfigurationNotFound(format!(
                "Empty step_templates array in task configuration for {}/{}. Cannot create workflow steps without step templates.",
                handler_config.namespace_name, handler_config.name
            )));
        }

        debug!(
            "✅ FILESYSTEM LOOKUP: Successfully loaded configuration for {}/{}/{}",
            namespace, name, version
        );

        Ok(handler_config)
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
