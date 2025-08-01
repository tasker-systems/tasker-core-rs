//! Batch Execution Sender
//!
//! CORRECTED ARCHITECTURE: This handler SENDS ExecuteBatch commands TO Ruby workers
//! based on capability matching and load balancing. It implements the fire-and-forget
//! pattern where Rust orchestrates and delegates execution to framework workers.

use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, StepExecutionRequest,
};
use crate::execution::command_router::CommandRouter;
use crate::models::core::step_execution_batch::{NewStepExecutionBatch, StepExecutionBatch};
use crate::models::core::step_execution_batch_step::{
    NewStepExecutionBatchStep, StepExecutionBatchStep,
};
use crate::models::core::step_execution_batch_transition::{
    NewStepExecutionBatchTransition, StepExecutionBatchTransition,
};
use crate::services::worker_selection_service::WorkerSelectionService;

/// Batch execution sender that SENDS ExecuteBatch commands TO workers
///
/// This implements the corrected architecture where Rust orchestrates and Ruby executes:
/// - WorkflowCoordinator discovers viable steps
/// - BatchExecutionSender selects workers and SENDS ExecuteBatch commands
/// - Ruby workers execute steps and send results back
/// - ResultAggregationHandler receives results and delegates to orchestration
pub struct BatchExecutionSender {
    /// Database-backed worker selection service for distributed worker management
    worker_selection_service: Arc<WorkerSelectionService>,

    /// Command router for sending commands to workers
    command_router: Arc<CommandRouter>,

    /// Database pool for batch persistence
    pool: sqlx::PgPool,

    /// Configuration for batch sending
    config: BatchSenderConfig,
}

/// Configuration for batch execution sending
#[derive(Debug, Clone)]
pub struct BatchSenderConfig {
    /// Default timeout for worker selection in milliseconds
    pub worker_selection_timeout_ms: u64,

    /// Maximum steps per batch
    pub max_steps_per_batch: usize,

    /// Namespace for command routing  
    pub default_namespace: String,
}

impl Default for BatchSenderConfig {
    fn default() -> Self {
        Self {
            worker_selection_timeout_ms: 5_000, // 5 seconds
            max_steps_per_batch: 100,
            default_namespace: "default".to_string(),
        }
    }
}

impl BatchExecutionSender {
    /// Create a new batch execution sender with database-backed worker selection
    pub fn new(
        worker_selection_service: Arc<WorkerSelectionService>,
        command_router: Arc<CommandRouter>,
        pool: sqlx::PgPool,
        config: BatchSenderConfig,
    ) -> Self {
        Self {
            worker_selection_service,
            command_router,
            pool,
            config,
        }
    }

    /// Send a batch of steps to appropriate workers (fire-and-forget)
    ///
    /// This implements the corrected architecture:
    /// 1. Create database batch records for persistence tracking
    /// 2. Select worker based on capabilities and load
    /// 3. Create ExecuteBatch command
    /// 4. Send command TO worker via transport
    /// 5. Update batch state to 'published'
    /// 6. Return immediately (fire-and-forget)
    pub async fn send_batch_to_workers(
        &self,
        task_id: i64,
        batch_id: String,
        steps: Vec<StepExecutionRequest>,
        namespace: Option<String>,
        task_handler_registry: &crate::registry::TaskHandlerRegistry,
    ) -> Result<BatchSendResult, BatchSendError> {
        let namespace = namespace.unwrap_or_else(|| self.config.default_namespace.clone());

        info!(
            "Sending batch {} with {} steps to workers in namespace '{}'",
            batch_id,
            steps.len(),
            namespace
        );

        // Validate batch size
        if steps.len() > self.config.max_steps_per_batch {
            return Err(BatchSendError::BatchTooLarge {
                requested: steps.len(),
                max_allowed: self.config.max_steps_per_batch,
            });
        }

        // 1. Get the TaskTemplate first to include with both database records and batch command
        // This eliminates the need for Ruby workers to look it up again
        let task = crate::models::core::task::Task::find_by_id(&self.pool, task_id)
            .await
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "find_task_for_template".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| BatchSendError::DatabaseError {
                operation: "find_task_for_template".to_string(),
                message: "Task not found".to_string(),
            })?;

        let task_for_orchestration = task.for_orchestration(&self.pool).await.map_err(|e| {
            BatchSendError::DatabaseError {
                operation: "get_task_orchestration_metadata".to_string(),
                message: e.to_string(),
            }
        })?;

        // DATABASE-FIRST: Get task template from named task configuration instead of deprecated registry
        let task_template = self.get_task_template_from_database(&task_for_orchestration).await?;

        // 2. Create database batch records before sending to worker
        let database_batch_id = self
            .create_batch_with_database_records_with_template(
                task_id,
                &batch_id,
                &steps,
                &task_template,
            )
            .await
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "create_batch".to_string(),
                message: e.to_string(),
            })?;

        // Use database batch_id as the actual batch identifier for better traceability
        let actual_batch_id = database_batch_id.to_string();

        // 3. Select appropriate worker based on capabilities and load
        let selected_worker = self
            .select_worker_for_batch(&namespace, steps.len())
            .await?;

        // 4. Create ExecuteBatch command with database batch_id and TaskTemplate
        let execute_command = Command::new(
            CommandType::ExecuteBatch,
            CommandPayload::ExecuteBatch {
                batch_id: actual_batch_id.clone(),
                steps: steps.clone(),
                task_template,
            },
            CommandSource::RustOrchestrator {
                id: "batch_execution_sender".to_string(),
            },
        );

        // 5. Send command TO worker via command router (fire-and-forget)
        match self.command_router.route_command(execute_command).await {
            Ok(_) => {
                info!(
                    "Successfully sent batch {} (DB ID: {}) with {} steps to worker {}",
                    actual_batch_id,
                    database_batch_id,
                    steps.len(),
                    selected_worker.worker_id
                );

                // 6. Update batch state to 'published' after successful transport
                if let Err(e) = self
                    .update_batch_state(
                        database_batch_id,
                        "created",
                        "published",
                        "tcp_publish_success",
                    )
                    .await
                {
                    warn!(
                        "Failed to update batch {} state to published: {}",
                        database_batch_id, e
                    );
                }

                // 7. Database persistence handles tracking - no in-memory tracking needed

                // 8. Worker load tracking is now handled by database-backed worker selection service
                debug!(
                    "Batch sent to worker {} - load tracking handled by database",
                    selected_worker.worker_id
                );

                Ok(BatchSendResult {
                    batch_id: actual_batch_id,
                    assigned_worker_id: selected_worker.worker_id,
                    step_count: steps.len(),
                    namespace,
                })
            }
            Err(e) => {
                error!(
                    "Failed to send batch {} to worker {}: {}",
                    actual_batch_id, selected_worker.worker_id, e
                );

                // Update batch state to failed if transport fails
                if let Err(update_err) = self
                    .update_batch_state(
                        database_batch_id,
                        "created",
                        "failed",
                        "tcp_publish_failed",
                    )
                    .await
                {
                    warn!(
                        "Failed to update batch {} state to failed: {}",
                        database_batch_id, update_err
                    );
                }

                Err(BatchSendError::TransportError {
                    worker_id: selected_worker.worker_id,
                    message: e.to_string(),
                })
            }
        }
    }

    /// Select the best worker for a batch based on capabilities and load
    ///
    /// This implementation uses database-backed worker selection to find workers
    /// that have explicit associations with tasks in the namespace, rather than
    /// relying on namespace-based in-memory registration.
    async fn select_worker_for_batch(
        &self,
        namespace: &str,
        step_count: usize,
    ) -> Result<SelectedWorker, BatchSendError> {
        debug!(
            "Selecting worker for namespace '{}' with {} steps using database-backed selection",
            namespace, step_count
        );

        // Enhanced worker selection: find workers that can handle tasks in the given namespace
        // This uses database-backed worker selection to find workers with explicit task associations
        // rather than relying on namespace-based in-memory registration.
        debug!(
            "Looking for workers capable of handling {} steps in namespace '{}'",
            step_count, namespace
        );

        // Find workers that can handle any task in the namespace
        let available_workers = self
            .worker_selection_service
            .find_workers_for_namespace(namespace, None)
            .await
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "find_workers_for_namespace".to_string(),
                message: e.to_string(),
            })?;

        if available_workers.is_empty() {
            return Err(BatchSendError::NoAvailableWorkers {
                namespace: namespace.to_string(),
                required_capacity: step_count,
            });
        }

        // Select the first available worker (they're already ordered by priority and health)
        let selected_worker = &available_workers[0];

        debug!(
            "Selected database worker {} (ID: {}) for batch in namespace '{}'",
            selected_worker.worker_name, selected_worker.worker_id, namespace
        );

        // Convert database SelectedWorker to our BatchExecutionSender SelectedWorker
        Ok(SelectedWorker {
            worker_id: selected_worker.worker_name.clone(), // Use worker_name as the ID for command routing
            current_load: 0,   // Database doesn't track current load - legacy field
            max_capacity: 100, // Default capacity - legacy field
        })
    }

    /// Create database batch records for persistence tracking with TaskTemplate
    ///
    /// This creates the main StepExecutionBatch record and associated StepExecutionBatchStep
    /// records, mirroring the ZeroMQ implementation but for TCP command architecture.
    async fn create_batch_with_database_records_with_template(
        &self,
        task_id: i64,
        batch_uuid: &str,
        steps: &[StepExecutionRequest],
        task_template: &crate::models::core::task_template::TaskTemplate,
    ) -> Result<i64, sqlx::Error> {
        let handler_class = task_template.task_handler_class.clone();

        // Create main batch record
        let new_batch = NewStepExecutionBatch {
            task_id,
            handler_class,
            batch_uuid: batch_uuid.to_string(),
            initiated_by: Some("tcp_batch_execution_sender".to_string()),
            batch_size: steps.len() as i32,
            timeout_seconds: 300, // 5 minutes default
            metadata: Some(serde_json::json!({
                "transport": "tcp_command",
                "sender": "batch_execution_sender",
                "step_count": steps.len(),
                "created_at": chrono::Utc::now()
            })),
        };

        // Create the batch record
        let batch = StepExecutionBatch::create(&self.pool, new_batch).await?;

        // Create individual step records for the batch using proper model
        let batch_steps: Vec<NewStepExecutionBatchStep> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| NewStepExecutionBatchStep {
                batch_id: batch.batch_id,
                workflow_step_id: step.step_id,
                sequence_order: index as i32,
                expected_handler_class: step.handler_class.clone(),
                metadata: Some(serde_json::json!({
                    "step_execution_request": {
                        "task_id": step.task_id,
                        "step_id": step.step_id,
                        "handler_class": step.handler_class
                    },
                    "batch_creation": {
                        "created_at": chrono::Utc::now(),
                        "transport": "tcp_command"
                    }
                })),
            })
            .collect();

        // Use the model's bulk creation method
        StepExecutionBatchStep::create_batch(&self.pool, batch_steps).await?;

        // Create initial transition to 'created' state
        self.update_batch_state(batch.batch_id, "", "created", "batch_created")
            .await?;

        Ok(batch.batch_id)
    }

    /// Update batch state with transition tracking
    ///
    /// Records state transitions in the step_execution_batch_transitions table
    /// for complete audit trail of batch lifecycle.
    async fn update_batch_state(
        &self,
        batch_id: i64,
        from_state: &str,
        to_state: &str,
        event_name: &str,
    ) -> Result<(), sqlx::Error> {
        // Find the current sort_key to increment for new transition
        let current_max_sort_key = sqlx::query!(
            "SELECT COALESCE(MAX(sort_key), 0) as max_sort_key FROM tasker_step_execution_batch_transitions WHERE batch_id = $1",
            batch_id
        )
        .fetch_one(&self.pool)
        .await?;

        let new_sort_key = current_max_sort_key.max_sort_key.unwrap_or(0) + 1;

        // Create new transition record using the model
        let new_transition = NewStepExecutionBatchTransition {
            batch_id,
            from_state: if from_state.is_empty() {
                None
            } else {
                Some(from_state.to_string())
            },
            to_state: to_state.to_string(),
            event_name: Some(event_name.to_string()),
            metadata: Some(serde_json::json!({
                "transport": "tcp_command",
                "timestamp": chrono::Utc::now(),
                "sender": "batch_execution_sender"
            })),
            sort_key: new_sort_key,
        };

        // Use the model's create method which handles the most_recent flag properly
        StepExecutionBatchTransition::create(&self.pool, new_transition).await?;

        info!(
            "Updated batch {} state: {} -> {} (event: {})",
            batch_id, from_state, to_state, event_name
        );

        Ok(())
    }

    /// Handle batch completion result reconciliation
    ///
    /// This method processes BatchCompletion responses from workers to update
    /// batch state and track individual step results. This completes the batch
    /// lifecycle tracking similar to the ZeroMQ implementation.
    pub async fn handle_batch_completion(
        &self,
        batch_id: &str,
        worker_id: &str,
        step_results: Vec<StepResult>,
    ) -> Result<BatchReconciliationResult, BatchSendError> {
        info!(
            "Handling batch completion: batch_id={}, worker_id={}, step_count={}",
            batch_id,
            worker_id,
            step_results.len()
        );

        // Find the database batch record
        let batch = StepExecutionBatch::find_by_uuid(&self.pool, batch_id)
            .await
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "find_batch_by_uuid".to_string(),
                message: e.to_string(),
            })?;

        let database_batch = match batch {
            Some(b) => b,
            None => {
                warn!("Received completion for unknown batch: {}", batch_id);
                return Err(BatchSendError::DatabaseError {
                    operation: "find_batch".to_string(),
                    message: format!("Batch not found: {}", batch_id),
                });
            }
        };

        // Process step results and determine overall batch status
        let mut successful_steps = 0;
        let mut failed_steps = 0;
        let mut total_execution_time_ms = 0;

        for step_result in &step_results {
            match step_result.status.as_str() {
                "completed" | "success" => successful_steps += 1,
                "failed" | "error" => failed_steps += 1,
                _ => {
                    warn!("Unknown step status: {}", step_result.status);
                    failed_steps += 1; // Treat unknown as failed for safety
                }
            }
            total_execution_time_ms += step_result.execution_time_ms.unwrap_or(0);
        }

        // Determine final batch state
        let final_state = if failed_steps == 0 {
            "completed"
        } else {
            "failed"
        };

        // Update batch state with completion details
        let _completion_metadata = serde_json::json!({
            "completion_summary": {
                "total_steps": step_results.len(),
                "successful_steps": successful_steps,
                "failed_steps": failed_steps,
                "total_execution_time_ms": total_execution_time_ms,
                "completed_by_worker": worker_id,
                "completed_at": chrono::Utc::now()
            },
            "step_results": step_results
        });

        // Update to final state
        self.update_batch_state(
            database_batch.batch_id,
            "published", // Assuming it was in published state
            final_state,
            &format!("batch_{}", final_state),
        )
        .await
        .map_err(|e| BatchSendError::DatabaseError {
            operation: "update_final_state".to_string(),
            message: e.to_string(),
        })?;

        // Worker load tracking is now handled by database-backed worker selection service
        debug!(
            "Batch completed by worker {} - load tracking handled by database",
            worker_id
        );

        // Database persistence handles tracking - no in-memory cleanup needed

        info!(
            "Batch reconciliation complete: {} -> {} ({}/{} steps successful)",
            batch_id,
            final_state,
            successful_steps,
            step_results.len()
        );

        Ok(BatchReconciliationResult {
            batch_id: batch_id.to_string(),
            database_batch_id: database_batch.batch_id,
            final_state: final_state.to_string(),
            total_steps: step_results.len(),
            successful_steps,
            failed_steps,
            total_execution_time_ms,
        })
    }

    /// Get task template from database using database-first approach
    /// Replaces the deprecated registry.get_task_template() call
    async fn get_task_template_from_database(
        &self,
        task_for_orchestration: &crate::models::core::task::TaskForOrchestration,
    ) -> Result<crate::models::core::task_template::TaskTemplate, BatchSendError> {
        use crate::models::core::named_task::NamedTask;
        use crate::models::core::task_namespace::TaskNamespace;
        
        // Get the task namespace
        let task_namespace = TaskNamespace::find_by_name(&self.pool, &task_for_orchestration.namespace_name)
            .await
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "find_task_namespace".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| BatchSendError::DatabaseError {
                operation: "find_task_namespace".to_string(),
                message: format!("Namespace not found: {}", task_for_orchestration.namespace_name),
            })?;

        // Get the named task with configuration
        let named_task = NamedTask::find_by_name_version_namespace(
            &self.pool,
            &task_for_orchestration.task_name,
            &task_for_orchestration.task_version,
            task_namespace.task_namespace_id as i64,
        )
        .await
        .map_err(|e| BatchSendError::DatabaseError {
            operation: "find_named_task".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| BatchSendError::DatabaseError {
            operation: "find_named_task".to_string(), 
            message: format!(
                "Task not found: {}/{} v{}",
                task_for_orchestration.namespace_name,
                task_for_orchestration.task_name,
                task_for_orchestration.task_version
            ),
        })?;

        // Extract the handler configuration from the named task
        let configuration = named_task.configuration.ok_or_else(|| BatchSendError::DatabaseError {
            operation: "extract_task_configuration".to_string(),
            message: format!(
                "No configuration found for task {}/{} v{}",
                task_for_orchestration.namespace_name,
                task_for_orchestration.task_name,
                task_for_orchestration.task_version
            ),
        })?;

        // Get the handler_config field which contains the YAML structure
        let handler_config_value = configuration.get("handler_config")
            .ok_or_else(|| BatchSendError::DatabaseError {
                operation: "extract_handler_config".to_string(),
                message: format!(
                    "TaskHandlerInfo missing handler_config field for {}/{}",
                    task_for_orchestration.namespace_name,
                    task_for_orchestration.task_name
                ),
            })?;

        // Convert HandlerConfiguration to TaskTemplate
        let handler_config: crate::orchestration::handler_config::HandlerConfiguration = 
            serde_json::from_value(handler_config_value.clone())
            .map_err(|e| BatchSendError::DatabaseError {
                operation: "deserialize_handler_config".to_string(),
                message: format!(
                    "Failed to deserialize handler configuration for {}/{}: {}",
                    task_for_orchestration.namespace_name,
                    task_for_orchestration.task_name,
                    e
                ),
            })?;

        // Convert HandlerConfiguration to TaskTemplate
        let task_template = crate::models::core::task_template::TaskTemplate {
            name: handler_config.name,
            module_namespace: handler_config.module_namespace,
            task_handler_class: handler_config.task_handler_class,
            namespace_name: handler_config.namespace_name,
            version: handler_config.version,
            default_dependent_system: handler_config.default_dependent_system,
            named_steps: handler_config.named_steps,
            schema: handler_config.schema,
            step_templates: handler_config.step_templates.into_iter().map(|st| {
                crate::models::core::task_template::StepTemplate {
                    name: st.name,
                    description: st.description,
                    dependent_system: st.dependent_system,
                    default_retryable: st.default_retryable,
                    default_retry_limit: st.default_retry_limit,
                    skippable: st.skippable,
                    handler_class: st.handler_class,
                    handler_config: st.handler_config,
                    depends_on_step: st.depends_on_step,
                    depends_on_steps: st.depends_on_steps,
                    custom_events: st.custom_events,
                }
            }).collect(),
            environments: handler_config.environments.map(|envs| {
                envs.into_iter().map(|(k, v)| {
                    (k, crate::models::core::task_template::EnvironmentConfig {
                        step_templates: v.step_templates.map(|templates| {
                            templates.into_iter().map(|t| {
                                crate::models::core::task_template::StepTemplateOverride {
                                    name: t.name,
                                    handler_config: t.handler_config,
                                    description: t.description,
                                    dependent_system: t.dependent_system,
                                    default_retryable: t.default_retryable,
                                    default_retry_limit: t.default_retry_limit,
                                    skippable: t.skippable,
                                }
                            }).collect()
                        }),
                        default_context: v.default_context,
                        default_options: v.default_options,
                    })
                }).collect()
            }),
            default_context: handler_config.default_context,
            default_options: handler_config.default_options,
        };

        Ok(task_template)
    }
}

/// Result of successfully sending a batch
#[derive(Debug, Clone)]
pub struct BatchSendResult {
    pub batch_id: String,
    pub assigned_worker_id: String,
    pub step_count: usize,
    pub namespace: String,
}

/// Information about a selected worker
#[derive(Debug, Clone)]
pub struct SelectedWorker {
    pub worker_id: String,
    pub current_load: usize,
    pub max_capacity: usize,
}

/// Individual step execution result from worker
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepResult {
    pub step_id: i64,
    pub status: String, // "completed", "failed", "error", etc.
    pub execution_time_ms: Option<u64>,
    pub error_message: Option<String>,
    pub result_data: Option<serde_json::Value>,
}

/// Result of batch reconciliation after completion
#[derive(Debug, Clone)]
pub struct BatchReconciliationResult {
    pub batch_id: String,
    pub database_batch_id: i64,
    pub final_state: String,
    pub total_steps: usize,
    pub successful_steps: usize,
    pub failed_steps: usize,
    pub total_execution_time_ms: u64,
}

/// Errors that can occur when sending batches
#[derive(Debug, thiserror::Error)]
pub enum BatchSendError {
    #[error("Batch too large: requested {requested} steps, max allowed {max_allowed}")]
    BatchTooLarge {
        requested: usize,
        max_allowed: usize,
    },

    #[error("No available workers for namespace '{namespace}' with capacity for {required_capacity} steps")]
    NoAvailableWorkers {
        namespace: String,
        required_capacity: usize,
    },

    #[error("No capable workers for namespace '{namespace}': need capacity {required_capacity}, have {available_workers} workers")]
    NoCapableWorkers {
        namespace: String,
        required_capacity: usize,
        available_workers: usize,
    },

    #[error("Transport error sending to worker {worker_id}: {message}")]
    TransportError { worker_id: String, message: String },

    #[error("Worker selection timeout after {timeout_ms}ms")]
    WorkerSelectionTimeout { timeout_ms: u64 },

    #[error("Database error during {operation}: {message}")]
    DatabaseError { operation: String, message: String },
}
