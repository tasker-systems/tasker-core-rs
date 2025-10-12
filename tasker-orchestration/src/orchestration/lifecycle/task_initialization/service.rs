//! Task Initializer Service
//!
//! Main orchestration service that coordinates task creation using focused components.

use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use opentelemetry::KeyValue;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::database::SqlFunctionExecutor;
use tasker_shared::logging;
use tasker_shared::metrics::orchestration::*;
use tasker_shared::models::{task_request::TaskRequest, Task};
use tasker_shared::system_context::SystemContext;
use tracing::{error, info, instrument};

use super::{
    NamespaceResolver, StateInitializer, TaskInitializationError, TemplateLoader,
    WorkflowStepBuilder,
};

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
    namespace_resolver: NamespaceResolver,
    template_loader: TemplateLoader,
    workflow_step_builder: WorkflowStepBuilder,
    state_initializer: StateInitializer,
    step_enqueuer_service: Arc<StepEnqueuerService>,
}

impl TaskInitializer {
    /// Create a new TaskInitializer
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer_service: Arc<StepEnqueuerService>,
    ) -> Self {
        Self {
            namespace_resolver: NamespaceResolver::new(context.clone()),
            template_loader: TemplateLoader::new(context.clone()),
            workflow_step_builder: WorkflowStepBuilder::new(context.clone()),
            state_initializer: StateInitializer::new(context.clone()),
            step_enqueuer_service,
            context,
        }
    }

    /// Create a complete task from TaskRequest with atomic transaction safety
    #[instrument(skip(self), fields(
        task_name = %task_request.name,
        correlation_id = %task_request.correlation_id
    ))]
    pub async fn create_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        // Store values before moving task_request
        let namespace = task_request.namespace.clone();
        let task_name = task_request.name.clone();
        let version = task_request.version.clone();
        let correlation_id = task_request.correlation_id;

        // Clone the task_request for handler configuration lookup
        let task_request_for_handler = task_request.clone();

        // Record task request metric
        if let Some(counter) = TASK_REQUESTS_TOTAL.get() {
            counter.add(
                1,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("task_type", task_name.clone()),
                    KeyValue::new("namespace", namespace.clone()),
                ],
            );
        }

        let start_time = Instant::now();

        logging::log_task_operation(
            "TASK_INITIALIZATION_START",
            None,
            Some(&task_name),
            Some(&namespace),
            "STARTING",
            Some(&format!(
                "version={version}, correlation_id={correlation_id}"
            )),
        );

        info!(
            task_name = %task_request.name,
            correlation_id = %correlation_id,
            "Starting task initialization"
        );

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
        let task_template = match self
            .template_loader
            .load_task_template(&task_request_for_handler)
            .await
        {
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
                  correlation_id = %correlation_id,
                  task_uuid = %task_uuid,
                  task_name = %task_name,
                  error = %e,
                  "Failed to load task template"
                );
                return Err(TaskInitializationError::ConfigurationNotFound(format!(
                    "Failed to load task template for task: {task_name}, namespace: {namespace}, version: {version}, error: {e}"
                )));
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
            .workflow_step_builder
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
        self.state_initializer
            .create_initial_state_transitions_in_tx(&mut tx, task_uuid, &step_mapping)
            .await?;

        // Commit transaction
        tx.commit().await.map_err(|e| {
            TaskInitializationError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        self.state_initializer
            .initialize_state_machines_post_transaction(task_uuid, &step_mapping)
            .await?;

        // Publish initialization event if publisher available
        self.publish_task_initialized(task_uuid, step_count, &task_name)
            .await?;

        let result = TaskInitializationResult {
            task_uuid,
            step_count,
            step_mapping: step_mapping.clone(),
            handler_config_name: Some(task_name.clone()),
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
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            task_name = %task_name,
            step_count = step_count,
            "Task initialization completed successfully"
        );

        // Record task initialization duration
        let duration_ms = start_time.elapsed().as_millis() as f64;
        if let Some(histogram) = TASK_INITIALIZATION_DURATION.get() {
            histogram.record(
                duration_ms,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("task_type", task_name.clone()),
                ],
            );
        }

        Ok(result)
    }

    /// Create a task and immediately enqueue its ready steps
    #[instrument(skip(self), fields(
        correlation_id = %task_request.correlation_id,
        task_name = %task_request.name
    ))]
    pub async fn create_and_enqueue_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        // Extract correlation_id before moving task_request
        let correlation_id = task_request.correlation_id;

        // First, create the task using existing method
        let initialization_result = self.create_task_from_request(task_request).await?;

        let task_uuid = initialization_result.task_uuid;

        info!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
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
        let named_task_uuid = self
            .namespace_resolver
            .resolve_named_task_uuid(&task_request)
            .await?;

        let mut new_task = Task::from_task_request(task_request);
        new_task.named_task_uuid = named_task_uuid;

        let task: Task = Task::create_with_transaction(tx, new_task)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!("Failed to create task: {e}"))
            })?;

        Ok(task)
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
