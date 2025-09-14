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
//! ```rust,no_run
//! use tasker_orchestration::orchestration::lifecycle::task_initializer::TaskInitializer;
//! use tasker_shared::system_context::SystemContext;
//! use tasker_orchestration::orchestration::lifecycle::step_enqueuer_service::StepEnqueuerService;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create system context and dependencies
//! # use tasker_shared::config::ConfigManager;
//! # let config_manager = ConfigManager::load()?;
//! let context = Arc::new(SystemContext::from_config(config_manager).await?);
//! let step_enqueuer = Arc::new(StepEnqueuerService::new(context.clone()).await?);
//! let initializer = TaskInitializer::new(context, step_enqueuer);
//!
//! // TaskInitializer is ready to create tasks from requests
//! // See integration tests for complete usage examples
//! let _initializer = initializer;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::lifecycle::step_enqueuer_service::StepEnqueuerService;
use crate::orchestration::state_manager::StateManager;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::database::SqlFunctionExecutor;
use tasker_shared::logging;
use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::models::{task_request::TaskRequest, NamedStep, Task, WorkflowStep};
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerError;
use tracing::{error, info, instrument, warn};

/// Result of task initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInitializationResult {
    /// Created task ID
    pub task_uuid: Uuid,
    /// Number of workflow steps created
    pub step_count: usize,
    /// Mapping of step names to workflow step IDs
    pub step_mapping: HashMap<String, Uuid>,
    /// Handler configuration used (if any)
    pub handler_config_name: Option<String>,
}

/// Atomic task creation with proper transaction safety
pub struct TaskInitializer {
    context: Arc<SystemContext>,
    state_manager: StateManager,
    step_enqueuer_service: Arc<StepEnqueuerService>,
}

impl TaskInitializer {
    /// Create a new TaskInitializer without step enqueuer (backward compatible)
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> Self {
        let state_manager = StateManager::new(context.clone());

        Self {
            context,
            state_manager,
            step_enqueuer_service,
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

        logging::log_task_operation(
            "TASK_INITIALIZATION_START",
            None,
            Some(&task_name),
            Some(&namespace),
            "STARTING",
            Some(&format!("version={version}")),
        );

        info!(task_name = %task_request.name, "Starting task initialization");

        // Use SQLx transaction for atomicity
        let mut tx = self.context.database_pool().begin().await.map_err(|e| {
            logging::log_error(
                "TaskInitializer",
                "create_task_from_request",
                &format!("Failed to begin transaction: {e}"),
                Some(&task_name),
            );
            TaskInitializationError::Database(format!("Failed to begin transaction: {e}"))
        })?;

        logging::log_database_operation(
            "TRANSACTION_BEGIN",
            Some("tasker_tasks"),
            None,
            "SUCCESS",
            None,
            Some("Atomic task creation transaction started"),
        );

        // Create the task within transaction
        let task = self.create_task_record(&mut tx, task_request).await?;
        let task_uuid = task.task_uuid;

        logging::log_task_operation(
            "TASK_RECORD_CREATED",
            Some(task_uuid),
            Some(&task_name),
            None,
            "SUCCESS",
            Some("Task record created in database"),
        );

        // Try to load task template
        let task_template = match self.load_task_template(&task_request_for_handler).await {
            Ok(template) => {
                logging::log_registry_operation(
                    "TASK_TEMPLATE_LOADED",
                    Some(&namespace),
                    Some(&task_name),
                    Some(&version),
                    "SUCCESS",
                    Some(&format!("Found {} step definitions", template.steps.len())),
                );

                template
            }
            Err(e) => {
                logging::log_registry_operation(
                    "TASK_TEMPLATE_FAILED",
                    Some(&namespace),
                    Some(&task_name),
                    Some(&version),
                    "FAILED",
                    Some(&format!("Registry lookup failed: {e}")),
                );
                error!(
                  task_uuid = task_uuid.to_string(),
                  task_name = %task_name,
                  error = %e,
                  "Failed to load task template"
                );
                return Err(TaskInitializationError::ConfigurationNotFound(format!("Failed to load task template for task: {task_name}, namespace: {namespace}, version: {version}, error: {e}")));
            }
        };

        logging::log_task_operation(
            "WORKFLOW_STEPS_CREATION_START",
            Some(task_uuid),
            Some(&task_name),
            None,
            "STARTING",
            Some(&format!(
                "Creating {} workflow steps",
                task_template.steps.len()
            )),
        );

        // Create workflow steps and dependencies
        let (step_count, step_mapping) = self
            .create_workflow_steps(&mut tx, task_uuid, &task_template)
            .await?;

        logging::log_task_operation(
            "WORKFLOW_STEPS_CREATED",
            Some(task_uuid),
            Some(&task_name),
            None,
            "SUCCESS",
            Some(&format!(
                "Created {step_count} workflow steps with dependencies"
            )),
        );

        // Create initial database transitions within the transaction
        self.create_initial_state_transitions_in_tx(&mut tx, task_uuid, &step_mapping)
            .await?;

        // Commit transaction
        tx.commit().await.map_err(|e| {
            TaskInitializationError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        self.initialize_state_machines_post_transaction(task_uuid, &step_mapping)
            .await?;

        // Publish initialization event if publisher available
        self.publish_task_initialized(task_uuid, step_count, &task_name)
            .await?;

        let result = TaskInitializationResult {
            task_uuid,
            step_count,
            step_mapping: step_mapping.clone(),
            handler_config_name: Some(task_name.clone()), // Always present now that we require configuration
        };

        logging::log_task_operation(
            "TASK_INITIALIZATION_COMPLETE",
            Some(task_uuid),
            Some(&task_name),
            None,
            "SUCCESS",
            Some(&format!(
                "Task completed: {} steps, handler_config: {:?}",
                step_count, result.handler_config_name
            )),
        );

        info!(
            task_uuid = task_uuid.to_string(),
            step_count = step_count,
            task_name = %task_name,
            "Task initialization completed successfully"
        );

        Ok(result)
    }

    /// Create a task and immediately enqueue its ready steps
    ///
    /// This method combines task creation with immediate step enqueuing (TAS-41)
    /// to reduce latency between task creation and step execution.
    #[instrument(skip(self), fields(task_name = %task_request.name))]
    pub async fn create_and_enqueue_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        // First, create the task using existing method
        let initialization_result = self.create_task_from_request(task_request).await?;

        let task_uuid = initialization_result.task_uuid.clone();

        info!(
            task_uuid = task_uuid.to_string(),
            step_count = initialization_result.step_count,
            "Task created, attempting immediate step enqueuing"
        );

        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());

        if let Some(task_info) = sql_executor.get_task_ready_info(task_uuid).await? {
            // Process the single task to enqueue its steps
            match self
                .step_enqueuer_service
                .process_single_task_from_ready_info(&task_info)
                .await
                .map_err(|err| TaskInitializationError::StepEnqueuing(format!("{err}")))
            {
                Ok(_) => Ok(initialization_result),
                Err(err) => Err(TaskInitializationError::StepEnqueuing(format!("{err}"))),
            }
        } else {
            Err(TaskInitializationError::Database(format!(
                "Unable to find task info for {task_uuid}"
            )))
        }
    }

    /// Create the basic task record
    async fn create_task_record(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_request: TaskRequest,
    ) -> Result<Task, TaskInitializationError> {
        // First, resolve the NamedTask from the TaskRequest
        let named_task_uuid = self.resolve_named_task_uuid(&task_request).await?;

        let mut new_task = Task::from_task_request(task_request);
        new_task.named_task_uuid = named_task_uuid;

        let task: Task = Task::create_with_transaction(tx, new_task)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create task: {e}"))
            })?;

        Ok(task)
    }

    /// Resolve NamedTask ID from TaskRequest (create if not exists)
    async fn resolve_named_task_uuid(
        &self,
        task_request: &TaskRequest,
    ) -> Result<Uuid, TaskInitializationError> {
        // First, find or create the task namespace
        let namespace = self
            .find_or_create_namespace(&task_request.namespace)
            .await?;

        // Find or create the named task
        let named_task = self
            .find_or_create_named_task(task_request, namespace.task_namespace_uuid as Uuid)
            .await?;

        Ok(named_task.named_task_uuid)
    }

    /// Find or create a task namespace
    async fn find_or_create_namespace(
        &self,
        namespace_name: &str,
    ) -> Result<tasker_shared::models::TaskNamespace, TaskInitializationError> {
        // Try to find existing namespace first
        if let Some(existing) = tasker_shared::models::TaskNamespace::find_by_name(
            &self.context.database_pool(),
            namespace_name,
        )
        .await
        .map_err(|e| TaskInitializationError::Database(format!("Failed to query namespace: {e}")))?
        {
            return Ok(existing);
        }

        // Create new namespace if not found
        let new_namespace = tasker_shared::models::core::task_namespace::NewTaskNamespace {
            name: namespace_name.to_string(),
            description: Some(format!("Auto-created namespace for {namespace_name}")),
        };

        let namespace = tasker_shared::models::TaskNamespace::create(
            &self.context.database_pool(),
            new_namespace,
        )
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
        task_namespace_uuid: Uuid,
    ) -> Result<tasker_shared::models::NamedTask, TaskInitializationError> {
        // Try to find existing named task first
        let existing_task = tasker_shared::models::NamedTask::find_by_name_version_namespace(
            &self.context.database_pool(),
            &task_request.name,
            &task_request.version,
            task_namespace_uuid,
        )
        .await
        .map_err(|e| {
            TaskInitializationError::Database(format!("Failed to query named task: {e}"))
        })?;

        if let Some(existing) = existing_task {
            return Ok(existing);
        }

        // Create new named task if not found
        let new_named_task = tasker_shared::models::core::named_task::NewNamedTask {
            name: task_request.name.clone(),
            version: Some(task_request.version.clone()),
            description: Some(format!("Auto-created task for {}", task_request.name)),
            task_namespace_uuid,
            configuration: None,
        };

        let named_task =
            tasker_shared::models::NamedTask::create(&self.context.database_pool(), new_named_task)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!("Failed to create named task: {e}"))
                })?;

        Ok(named_task)
    }

    /// Create workflow steps and their dependencies from task template
    async fn create_workflow_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &tasker_shared::models::core::task_template::TaskTemplate,
    ) -> Result<(usize, HashMap<String, Uuid>), TaskInitializationError> {
        // Step 1: Create all named steps and workflow steps
        let step_mapping = self.create_steps(tx, task_uuid, task_template).await?;

        // Step 2: Create dependencies between steps
        self.create_step_dependencies(tx, task_template, &step_mapping)
            .await?;

        Ok((step_mapping.len(), step_mapping))
    }

    /// Create named steps and workflow steps using consistent transaction methods
    async fn create_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &tasker_shared::models::core::task_template::TaskTemplate,
    ) -> Result<HashMap<String, Uuid>, TaskInitializationError> {
        let mut step_mapping = HashMap::new();

        for step_definition in &task_template.steps {
            // Create or find named step using transaction
            let named_steps =
                NamedStep::find_by_name(&self.context.database_pool(), &step_definition.name)
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to search for NamedStep '{}': {}",
                            step_definition.name, e
                        ))
                    })?;

            let named_step = if let Some(existing_step) = named_steps.first() {
                existing_step.clone()
            } else {
                // Create new named step using transaction-aware method
                let system_name = "tasker_core_rust"; // Use a consistent system name for Rust core
                NamedStep::find_or_create_by_name_with_transaction(
                    tx,
                    &self.context.database_pool(),
                    &step_definition.name,
                    system_name,
                )
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create NamedStep '{}': {}",
                        step_definition.name, e
                    ))
                })?
            };

            // Create workflow step using consistent transaction method
            // Convert handler initialization to JSON Value for inputs
            let inputs = if step_definition.handler.initialization.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_value(&step_definition.handler.initialization).map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to serialize handler initialization for step '{}': {}",
                            step_definition.name, e
                        ))
                    })?,
                )
            };

            let new_workflow_step = tasker_shared::models::core::workflow_step::NewWorkflowStep {
                task_uuid,
                named_step_uuid: named_step.named_step_uuid,
                retryable: Some(step_definition.retry.retryable),
                retry_limit: Some(step_definition.retry.limit as i32),
                inputs,
                skippable: None, // Not available in new TaskTemplate - could be added if needed
            };

            let workflow_step = WorkflowStep::create_with_transaction(tx, new_workflow_step)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create WorkflowStep '{}': {}",
                        step_definition.name, e
                    ))
                })?;

            step_mapping.insert(
                step_definition.name.clone(),
                workflow_step.workflow_step_uuid,
            );
        }

        Ok(step_mapping)
    }

    /// Create dependencies between workflow steps using consistent transaction methods
    async fn create_step_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_template: &tasker_shared::models::core::task_template::TaskTemplate,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        for step_definition in &task_template.steps {
            let to_step_uuid = step_mapping[&step_definition.name];

            // Create edges for all dependencies using transaction method
            for dependency_name in &step_definition.dependencies {
                if let Some(&from_step_uuid) = step_mapping.get(dependency_name) {
                    let new_edge =
                        tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge {
                            from_step_uuid,
                            to_step_uuid,
                            name: "provides".to_string(),
                        };

                    tasker_shared::models::WorkflowStepEdge::create_with_transaction(tx, new_edge)
                        .await
                        .map_err(|e| {
                            TaskInitializationError::Database(format!(
                                "Failed to create edge '{}' -> '{}': {}",
                                dependency_name, step_definition.name, e
                            ))
                        })?;
                }
            }
        }

        Ok(())
    }

    /// Create initial state transitions in database using consistent transaction methods
    async fn create_initial_state_transitions_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        // Create initial task transition using transaction method
        let new_task_transition = tasker_shared::models::core::task_transition::NewTaskTransition {
            task_uuid,
            to_state: "pending".to_string(),
            from_state: None,
            processor_uuid: Some(self.context.processor_uuid()),
            metadata: Some(json!({"initial_state": "pending", "from_service": "task_initializer"})),
        };

        tasker_shared::models::TaskTransition::create_with_transaction(tx, new_task_transition)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create initial task transition: {e}"
                ))
            })?;

        // Create initial step transitions using transaction method
        for &workflow_step_uuid in step_mapping.values() {
            let new_step_transition =
                tasker_shared::models::core::workflow_step_transition::NewWorkflowStepTransition {
                    workflow_step_uuid,
                    to_state: "pending".to_string(),
                    from_state: None,
                    metadata: Some(
                        json!({"initial_state": "pending", "from_service": "task_initializer"}),
                    ),
                };

            tasker_shared::models::WorkflowStepTransition::create_with_transaction(
                tx,
                new_step_transition,
            )
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create initial step transition for step {workflow_step_uuid}: {e}"
                ))
            })?;
        }

        Ok(())
    }

    /// Initialize StateManager-based state machines after transaction commit
    async fn initialize_state_machines_post_transaction(
        &self,
        task_uuid: Uuid,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        // Ensure the task transitions from Pending to Initializing state
        // This is critical for the task lifecycle to work properly
        use tasker_shared::state_machine::{TaskEvent, TaskStateMachine};

        let mut task_state_machine = TaskStateMachine::for_task(
            task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await
        .map_err(|e| {
            TaskInitializationError::StateMachine(format!(
                "Failed to create task state machine: {e}"
            ))
        })?;

        let current_state = task_state_machine.current_state().await.map_err(|e| {
            TaskInitializationError::StateMachine(format!("Failed to get current state: {e}"))
        })?;

        info!(
            task_uuid = task_uuid.to_string(),
            current_state = %current_state,
            "Task state machine created, transitioning from Pending to Initializing"
        );

        // Transition from Pending to Initializing if needed
        if current_state == TaskState::Pending {
            match task_state_machine.transition(TaskEvent::Start).await {
                Ok(success) => {
                    if success {
                        info!(
                            task_uuid = task_uuid.to_string(),
                            "Successfully transitioned task from Pending to Initializing"
                        );
                    } else {
                        warn!(
                            task_uuid = task_uuid.to_string(),
                            "Task state transition returned false - task may already be in correct state"
                        );
                    }
                }
                Err(e) => {
                    error!(
                        task_uuid = task_uuid.to_string(),
                        error = %e,
                        "Failed to transition task from Pending to Initializing"
                    );
                    return Err(TaskInitializationError::StateMachine(format!(
                        "Failed to transition task from Pending to Initializing: {e}"
                    )));
                }
            }
        } else {
            info!(
                task_uuid = task_uuid.to_string(),
                current_state = %current_state,
                "Task already in non-Pending state, no transition needed"
            );
        }

        // Initialize step state machines WITHOUT evaluating state transitions
        // We don't want to transition steps to InProgress during initialization
        // as this sets in_process=true, making them ineligible for execution
        for &workflow_step_uuid in step_mapping.values() {
            // Simply verify the state machine exists, don't evaluate/transition
            match self
                .state_manager
                .get_or_create_step_state_machine(workflow_step_uuid)
                .await
            {
                Ok(state_machine) => match state_machine.current_state().await {
                    Ok(_current_state) => {}
                    Err(e) => {
                        warn!(
                            step_uuid = workflow_step_uuid.to_string(),
                            error = %e,
                            "Failed to get current state from step state machine"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        step_uuid = workflow_step_uuid.to_string(),
                        error = %e,
                        "Failed to initialize step state machine, basic initialization completed"
                    );
                    // Don't fail the entire initialization for StateManager issues
                }
            }
        }

        Ok(())
    }

    /// Load task template from TaskHandlerRegistry or filesystem
    /// In FFI integration, handlers register their configuration in the registry
    /// For tests, falls back to filesystem-based YAML discovery
    async fn load_task_template(
        &self,
        task_request: &TaskRequest,
    ) -> Result<TaskTemplate, TaskInitializationError> {
        // Try registry first if available
        match self
            .load_from_registry(task_request, self.context.task_handler_registry.clone())
            .await
        {
            Ok(config) => return Ok(config),
            Err(e) => {
                Err(TaskInitializationError::ConfigurationNotFound(
                    format!("Unable to load task template: {e}, request parameters: name {}, namespace {}, version {}", task_request.name, task_request.namespace, task_request.version)
                ))
            }
        }
    }

    /// Load configuration from registry
    async fn load_from_registry(
        &self,
        task_request: &TaskRequest,
        registry: Arc<TaskHandlerRegistry>,
    ) -> Result<TaskTemplate, TaskInitializationError> {
        // Use the namespace and name directly from the TaskRequest
        let namespace = &task_request.namespace;
        let name = &task_request.name;
        let _version = &task_request.version;

        // Look up the handler metadata using the ACTUAL task request version

        let metadata = registry.resolve_handler(task_request).await.map_err(|e| {
            TaskInitializationError::ConfigurationNotFound(format!(
                "Handler not found in registry {namespace}/{name}: {e}"
            ))
        })?;

        // Extract the config_schema from metadata and convert from full TaskHandlerInfo format

        if let Some(config_json) = metadata.config_schema {
            // The database now stores the full TaskTemplate structure directly
            // Deserialize as TaskTemplate (new format) and return directly
            match serde_json::from_value::<TaskTemplate>(config_json.clone()) {
                Ok(task_template) => {
                    // Return TaskTemplate directly - no more legacy conversion!
                    if task_template.steps.is_empty() {
                        return Err(TaskInitializationError::ConfigurationNotFound(format!(
                            "Empty steps array in task template configuration for {}/{}. Cannot create workflow steps without step definitions.",
                            task_template.namespace_name, task_template.name
                        )));
                    }

                    Ok(task_template)
                }
                Err(task_template_error) => {
                    // Aggressive forward-only change: No backward compatibility for legacy formats
                    Err(TaskInitializationError::ConfigurationNotFound(format!(
                        "Failed to deserialize configuration as TaskTemplate for {namespace}/{name}. Error: {task_template_error}. Legacy formats are no longer supported - please migrate to new self-describing TaskTemplate format."
                    )))
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

    /// Publish task initialization event
    async fn publish_task_initialized(
        &self,
        _task_uuid: Uuid,
        _step_count: usize,
        _task_name: &str,
    ) -> Result<(), TaskInitializationError> {
        // TODO: Implement event publishing once EventPublisher interface is finalized
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

    #[error("Step enqueuing error: {0}")]
    StepEnqueuing(String),
}

impl From<TaskInitializationError> for TaskerError {
    fn from(error: TaskInitializationError) -> Self {
        TaskerError::OrchestrationError(format!("Task initialization failed: {error}"))
    }
}

impl From<sqlx::Error> for TaskInitializationError {
    fn from(error: sqlx::Error) -> Self {
        TaskInitializationError::Database(error.to_string())
    }
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
    fn test_task_initialization_result_creation() {
        let task_uuid = Uuid::now_v7();

        let mut step_mapping = HashMap::new();
        step_mapping.insert("step1".to_string(), Uuid::new_v4());
        step_mapping.insert("step2".to_string(), Uuid::new_v4());

        let result = TaskInitializationResult {
            task_uuid,
            step_count: 2,
            step_mapping: step_mapping.clone(),
            handler_config_name: Some("test_handler".to_string()),
        };

        assert_eq!(result.task_uuid, task_uuid);
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
