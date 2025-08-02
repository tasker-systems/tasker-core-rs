//! # Orchestration System (pgmq-based)
//!
//! Main orchestration system that replaces the TCP-based approach with pgmq queues.
//! Handles the complete lifecycle of task processing through PostgreSQL message queues.

use crate::error::{Result, TaskerError};
use crate::messaging::{PgmqClient, TaskProcessingMessage};
use crate::models::core::{step_execution_batch::StepExecutionBatch, task::Task};
use crate::orchestration::{
    task_request_processor::TaskRequestProcessor, workflow_coordinator::WorkflowCoordinator,
};
use crate::registry::TaskHandlerRegistry;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for the orchestration system
#[derive(Debug, Clone)]
pub struct OrchestrationSystemConfig {
    /// Queue name for tasks to be processed
    pub tasks_queue_name: String,
    /// Queue name for batch results
    pub results_queue_name: String,
    /// Polling interval for tasks (seconds)
    pub task_polling_interval_seconds: u64,
    /// Polling interval for results (seconds)
    pub result_polling_interval_seconds: u64,
    /// Number of tasks to process per batch
    pub task_batch_size: i32,
    /// Number of results to process per batch
    pub result_batch_size: i32,
    /// Visibility timeout for messages (seconds)
    pub visibility_timeout_seconds: i32,
    /// Maximum processing attempts before giving up
    pub max_processing_attempts: i32,
    /// Namespaces to create queues for
    pub active_namespaces: Vec<String>,
}

impl Default for OrchestrationSystemConfig {
    fn default() -> Self {
        Self {
            tasks_queue_name: "orchestration_tasks_to_be_processed".to_string(),
            results_queue_name: "orchestration_batch_results".to_string(),
            task_polling_interval_seconds: 1,
            result_polling_interval_seconds: 1,
            task_batch_size: 10,
            result_batch_size: 20,
            visibility_timeout_seconds: 300, // 5 minutes
            max_processing_attempts: 3,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
        }
    }
}

/// Main orchestration system using pgmq for coordination
pub struct OrchestrationSystemPgmq {
    /// PostgreSQL message queue client
    pgmq_client: Arc<PgmqClient>,
    /// Task request processor
    task_request_processor: Arc<TaskRequestProcessor>,
    /// Workflow coordinator for batch creation
    workflow_coordinator: Arc<WorkflowCoordinator>,
    /// Task handler registry
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Database connection pool
    pool: PgPool,
    /// Configuration
    config: OrchestrationSystemConfig,
}

impl OrchestrationSystemPgmq {
    /// Create a new orchestration system
    pub fn new(
        pgmq_client: Arc<PgmqClient>,
        task_request_processor: Arc<TaskRequestProcessor>,
        workflow_coordinator: Arc<WorkflowCoordinator>,
        task_handler_registry: Arc<TaskHandlerRegistry>,
        pool: PgPool,
        config: OrchestrationSystemConfig,
    ) -> Self {
        Self {
            pgmq_client,
            task_request_processor,
            workflow_coordinator,
            task_handler_registry,
            pool,
            config,
        }
    }

    /// Start the complete orchestration system
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<()> {
        info!("🚀 Starting pgmq-based orchestration system");

        // Initialize all required queues
        self.initialize_queues().await?;

        // Start all processing loops concurrently
        let task_processor = self.clone_for_task_processing();
        let result_processor = self.clone_for_result_processing();

        let task_handle =
            tokio::spawn(async move { task_processor.start_task_processing_loop().await });

        let result_handle =
            tokio::spawn(async move { result_processor.start_result_processing_loop().await });

        // Wait for both loops (they should run forever)
        let (task_result, result_result) = tokio::join!(task_handle, result_handle);

        // Log any errors that caused the loops to exit
        if let Err(e) = task_result {
            error!(error = %e, "Task processing loop panicked");
        }
        if let Err(e) = result_result {
            error!(error = %e, "Result processing loop panicked");
        }

        Ok(())
    }

    /// Start the task processing loop
    #[instrument(skip(self))]
    async fn start_task_processing_loop(&self) -> Result<()> {
        info!(
            queue = %self.config.tasks_queue_name,
            polling_interval = %self.config.task_polling_interval_seconds,
            "Starting task processing loop"
        );

        loop {
            match self.process_task_batch().await {
                Ok(processed_count) => {
                    if processed_count == 0 {
                        // No tasks processed, wait before polling again
                        sleep(Duration::from_secs(
                            self.config.task_polling_interval_seconds,
                        ))
                        .await;
                    }
                    // If we processed tasks, continue immediately for better throughput
                }
                Err(e) => {
                    error!(error = %e, "Error in task processing batch");
                    // Wait before retrying on error
                    sleep(Duration::from_secs(
                        self.config.task_polling_interval_seconds,
                    ))
                    .await;
                }
            }
        }
    }

    /// Start the result processing loop
    #[instrument(skip(self))]
    async fn start_result_processing_loop(&self) -> Result<()> {
        info!(
            queue = %self.config.results_queue_name,
            polling_interval = %self.config.result_polling_interval_seconds,
            "Starting result processing loop"
        );

        loop {
            match self.process_result_batch().await {
                Ok(processed_count) => {
                    if processed_count == 0 {
                        // No results processed, wait before polling again
                        sleep(Duration::from_secs(
                            self.config.result_polling_interval_seconds,
                        ))
                        .await;
                    }
                    // If we processed results, continue immediately for better throughput
                }
                Err(e) => {
                    error!(error = %e, "Error in result processing batch");
                    // Wait before retrying on error
                    sleep(Duration::from_secs(
                        self.config.result_polling_interval_seconds,
                    ))
                    .await;
                }
            }
        }
    }

    /// Process a batch of task processing messages
    #[instrument(skip(self))]
    async fn process_task_batch(&self) -> Result<usize> {
        // Read messages from the task processing queue
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.tasks_queue_name,
                Some(self.config.visibility_timeout_seconds),
                Some(self.config.task_batch_size),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read task messages: {}", e))
            })?;

        if messages.is_empty() {
            return Ok(0);
        }

        let message_count = messages.len();
        debug!(
            message_count = message_count,
            queue = %self.config.tasks_queue_name,
            "Processing batch of task messages"
        );

        let mut processed_count = 0;

        for message in messages {
            match self
                .process_single_task(&message.message, message.msg_id)
                .await
            {
                Ok(()) => {
                    // Delete the successfully processed message
                    if let Err(e) = self
                        .pgmq_client
                        .delete_message(&self.config.tasks_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %e,
                            "Failed to delete processed task message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = message.msg_id,
                        error = %e,
                        "Failed to process task message"
                    );

                    // Archive failed messages
                    if let Err(archive_err) = self
                        .pgmq_client
                        .archive_message(&self.config.tasks_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %archive_err,
                            "Failed to archive failed task message"
                        );
                    }
                }
            }
        }

        if processed_count > 0 {
            info!(
                processed_count = processed_count,
                total_messages = message_count,
                "Completed task processing batch"
            );
        }

        Ok(processed_count)
    }

    /// Process a single task processing message
    #[instrument(skip(self, payload))]
    async fn process_single_task(&self, payload: &serde_json::Value, msg_id: i64) -> Result<()> {
        // Parse the task processing message
        let task_message: TaskProcessingMessage =
            serde_json::from_value(payload.clone()).map_err(|e| {
                TaskerError::ValidationError(format!("Invalid task processing message: {}", e))
            })?;

        info!(
            task_id = task_message.task_id,
            namespace = %task_message.namespace,
            task_name = %task_message.task_name,
            msg_id = msg_id,
            "Processing task for batch creation"
        );

        // Load the task from the database
        let _task = Task::find_by_id(&self.pool, task_message.task_id)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to load task: {}", e)))?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!("Task not found: {}", task_message.task_id))
            })?;

        // Use the workflow coordinator to create step execution batches
        match self
            .workflow_coordinator
            .execute_task_workflow(task_message.task_id)
            .await
        {
            Ok(orchestration_result) => match orchestration_result {
                crate::orchestration::types::TaskOrchestrationResult::Published {
                    task_id,
                    viable_steps_discovered,
                    steps_published,
                    batch_id,
                    publication_time_ms,
                    next_poll_delay_ms: _,
                } => {
                    info!(
                        task_id = task_id,
                        viable_steps_discovered = viable_steps_discovered,
                        steps_published = steps_published,
                        batch_id = ?batch_id,
                        publication_time_ms = publication_time_ms,
                        "Task workflow steps published successfully"
                    );
                    Ok(())
                }
                crate::orchestration::types::TaskOrchestrationResult::Blocked {
                    task_id,
                    blocking_reason,
                    viable_steps_checked,
                } => {
                    info!(
                        task_id = task_id,
                        blocking_reason = %blocking_reason,
                        viable_steps_checked = viable_steps_checked,
                        "Task workflow is blocked, no steps ready"
                    );
                    Ok(())
                }
                crate::orchestration::types::TaskOrchestrationResult::Complete {
                    task_id,
                    steps_completed,
                    total_execution_time_ms,
                } => {
                    info!(
                        task_id = task_id,
                        steps_completed = steps_completed,
                        total_execution_time_ms = total_execution_time_ms,
                        "Task workflow completed"
                    );
                    Ok(())
                }
                crate::orchestration::types::TaskOrchestrationResult::Failed {
                    task_id,
                    error,
                    failed_steps,
                } => {
                    error!(
                        task_id = task_id,
                        error = %error,
                        failed_steps = ?failed_steps,
                        "Task workflow failed"
                    );
                    Err(TaskerError::OrchestrationError(format!(
                        "Workflow execution failed for task {}: {}",
                        task_id, error
                    )))
                }
            },
            Err(e) => {
                error!(
                    task_id = task_message.task_id,
                    error = %e,
                    "Failed to execute task workflow"
                );
                Err(TaskerError::OrchestrationError(format!(
                    "Workflow execution failed for task {}: {}",
                    task_message.task_id, e
                )))
            }
        }
    }

    /// Process a batch of result messages
    #[instrument(skip(self))]
    async fn process_result_batch(&self) -> Result<usize> {
        // Read messages from the results queue
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.results_queue_name,
                Some(self.config.visibility_timeout_seconds),
                Some(self.config.result_batch_size),
            )
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to read result messages: {}", e))
            })?;

        if messages.is_empty() {
            return Ok(0);
        }

        let message_count = messages.len();
        debug!(
            message_count = message_count,
            queue = %self.config.results_queue_name,
            "Processing batch of result messages"
        );

        let mut processed_count = 0;

        for message in messages {
            match self
                .process_single_result(&message.message, message.msg_id)
                .await
            {
                Ok(()) => {
                    // Delete the successfully processed message
                    if let Err(e) = self
                        .pgmq_client
                        .delete_message(&self.config.results_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %e,
                            "Failed to delete processed result message"
                        );
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        msg_id = message.msg_id,
                        error = %e,
                        "Failed to process result message"
                    );

                    // Archive failed messages
                    if let Err(archive_err) = self
                        .pgmq_client
                        .archive_message(&self.config.results_queue_name, message.msg_id)
                        .await
                    {
                        warn!(
                            msg_id = message.msg_id,
                            error = %archive_err,
                            "Failed to archive failed result message"
                        );
                    }
                }
            }
        }

        if processed_count > 0 {
            info!(
                processed_count = processed_count,
                total_messages = message_count,
                "Completed result processing batch"
            );
        }

        Ok(processed_count)
    }

    /// Process a single batch result message
    #[instrument(skip(self, payload))]
    async fn process_single_result(&self, payload: &serde_json::Value, msg_id: i64) -> Result<()> {
        // Parse the batch result message flexibly to handle Ruby-generated messages
        let result_message = self.parse_batch_result_message(payload).map_err(|e| {
            TaskerError::ValidationError(format!("Invalid batch result message: {}", e))
        })?;

        info!(
            batch_id = result_message.batch_id,
            task_id = result_message.task_id,
            namespace = %result_message.namespace,
            batch_status = %result_message.batch_status,
            successful_steps = result_message.successful_steps,
            failed_steps = result_message.failed_steps,
            msg_id = msg_id,
            "Processing batch result"
        );

        // Find the corresponding step execution batch
        let _batch = StepExecutionBatch::find_by_id(&self.pool, result_message.batch_id)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to load batch: {}", e)))?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!(
                    "Batch not found: {}",
                    result_message.batch_id
                ))
            })?;

        // Start a database transaction for atomic updates
        let mut tx = self.pool.begin().await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to start transaction: {}", e))
        })?;

        // Step 1: Update individual step results in the database
        for step_result in &result_message.step_results {
            if let Err(e) = self.update_step_result(&mut tx, step_result).await {
                error!(
                    step_id = step_result.step_id,
                    batch_id = result_message.batch_id,
                    error = %e,
                    "Failed to update step result"
                );
                // Continue processing other steps, but log the error
            }
        }

        // Step 2: Check if all steps for the task are complete
        let task_completion_stats =
            crate::models::core::workflow_step::WorkflowStep::task_completion_stats(
                &self.pool,
                result_message.task_id,
            )
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to get task completion stats: {}", e))
            })?;

        info!(
            task_id = result_message.task_id,
            total_steps = task_completion_stats.total_steps,
            completed_steps = task_completion_stats.completed_steps,
            failed_steps = task_completion_stats.failed_steps,
            pending_steps = task_completion_stats.pending_steps,
            all_complete = task_completion_stats.all_complete,
            "Task completion status after batch processing"
        );

        // Commit the step result updates
        tx.commit().await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to commit transaction: {}", e))
        })?;

        // Step 3: Handle task completion or continue orchestration
        if task_completion_stats.all_complete {
            // Task is complete - mark it as finished
            if let Err(e) = self.mark_task_complete(result_message.task_id).await {
                error!(
                    task_id = result_message.task_id,
                    error = %e,
                    "Failed to mark task as complete"
                );
            } else {
                info!(
                    task_id = result_message.task_id,
                    batch_id = result_message.batch_id,
                    "Task completed successfully"
                );
            }
        } else if task_completion_stats.pending_steps > 0 {
            // Task has more work - enqueue next batch of ready steps
            if let Err(e) = self
                .enqueue_next_batch_for_task(result_message.task_id)
                .await
            {
                error!(
                    task_id = result_message.task_id,
                    error = %e,
                    "Failed to enqueue next batch for task"
                );
            } else {
                info!(
                    task_id = result_message.task_id,
                    pending_steps = task_completion_stats.pending_steps,
                    "Enqueued next batch for task continuation"
                );
            }
        } else {
            // All steps are either complete or failed - task is done
            info!(
                task_id = result_message.task_id,
                completed_steps = task_completion_stats.completed_steps,
                failed_steps = task_completion_stats.failed_steps,
                "Task finished - no more steps to process"
            );
        }

        info!(
            batch_id = result_message.batch_id,
            task_id = result_message.task_id,
            "Successfully processed batch result"
        );

        Ok(())
    }

    /// Parse batch result message flexibly to handle Ruby-generated messages
    fn parse_batch_result_message(
        &self,
        payload: &serde_json::Value,
    ) -> Result<FlexibleBatchResultMessage> {
        // Extract basic fields
        let batch_id = payload
            .get("batch_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| TaskerError::ValidationError("Missing batch_id".to_string()))?;

        let task_id = payload
            .get("task_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| TaskerError::ValidationError("Missing task_id".to_string()))?;

        let namespace = payload
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let batch_status = payload
            .get("batch_status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        // Parse step results
        let step_results = payload
            .get("step_results")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|step_result| self.parse_step_result(step_result).ok())
            .collect();

        // Parse metadata for statistics
        let metadata = payload.get("metadata").unwrap_or(&serde_json::Value::Null);
        let successful_steps = metadata
            .get("successful_steps")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let failed_steps = metadata
            .get("failed_steps")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        Ok(FlexibleBatchResultMessage {
            batch_id,
            task_id,
            namespace,
            batch_status,
            step_results,
            successful_steps,
            failed_steps,
        })
    }

    /// Parse a step result from JSON flexibly
    fn parse_step_result(&self, step_result: &serde_json::Value) -> Result<FlexibleStepResult> {
        let step_id = step_result
            .get("step_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                TaskerError::ValidationError("Missing step_id in step result".to_string())
            })?;

        let status = step_result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let output = step_result.get("output").cloned();
        let error = step_result
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let error_code = step_result
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let execution_duration_ms = step_result
            .get("execution_duration_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let executed_at = step_result
            .get("executed_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(FlexibleStepResult {
            step_id,
            status,
            output,
            error,
            error_code,
            execution_duration_ms,
            executed_at,
        })
    }

    /// Update a single step result in the database
    async fn update_step_result(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        step_result: &FlexibleStepResult,
    ) -> Result<()> {
        use crate::models::core::workflow_step::WorkflowStep;

        // Load the workflow step
        let workflow_step = WorkflowStep::find_by_id(&self.pool, step_result.step_id)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!(
                    "Failed to load workflow step {}: {}",
                    step_result.step_id, e
                ))
            })?
            .ok_or_else(|| {
                TaskerError::ValidationError(format!(
                    "Workflow step not found: {}",
                    step_result.step_id
                ))
            })?;

        match step_result.status.as_str() {
            "Success" => {
                // Mark step as processed with results
                let results_json = step_result
                    .output
                    .clone()
                    .unwrap_or(serde_json::Value::Null);

                sqlx::query!(
                    r#"
                    UPDATE tasker_workflow_steps 
                    SET processed = true, 
                        in_process = false,
                        processed_at = NOW(),
                        results = $2,
                        updated_at = NOW()
                    WHERE workflow_step_id = $1
                    "#,
                    workflow_step.workflow_step_id,
                    results_json
                )
                .execute(&mut **tx)
                .await
                .map_err(|e| {
                    TaskerError::DatabaseError(format!(
                        "Failed to mark step {} as processed: {}",
                        workflow_step.workflow_step_id, e
                    ))
                })?;

                debug!(
                    step_id = workflow_step.workflow_step_id,
                    execution_duration_ms = step_result.execution_duration_ms,
                    "Updated step as successfully processed"
                );
            }
            "Failed" => {
                // Mark step as failed and set backoff if retryable
                let error_json = serde_json::json!({
                    "error": step_result.error,
                    "error_code": step_result.error_code,
                    "execution_duration_ms": step_result.execution_duration_ms,
                    "executed_at": step_result.executed_at
                });

                if workflow_step.retryable && !workflow_step.has_exceeded_retry_limit() {
                    // Step is retryable - set backoff for retry
                    let backoff_seconds =
                        calculate_backoff_seconds(workflow_step.attempts.unwrap_or(0));

                    sqlx::query!(
                        r#"
                        UPDATE tasker_workflow_steps 
                        SET in_process = false,
                            backoff_request_seconds = $2,
                            results = $3,
                            updated_at = NOW()
                        WHERE workflow_step_id = $1
                        "#,
                        workflow_step.workflow_step_id,
                        backoff_seconds,
                        error_json
                    )
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| {
                        TaskerError::DatabaseError(format!(
                            "Failed to set backoff for step {}: {}",
                            workflow_step.workflow_step_id, e
                        ))
                    })?;

                    debug!(
                        step_id = workflow_step.workflow_step_id,
                        backoff_seconds = backoff_seconds,
                        "Set step backoff for retry"
                    );
                } else {
                    // Step is not retryable or has exceeded retry limit - mark as permanently failed
                    sqlx::query!(
                        r#"
                        UPDATE tasker_workflow_steps 
                        SET in_process = false,
                            processed = true,
                            processed_at = NOW(),
                            results = $2,
                            updated_at = NOW()
                        WHERE workflow_step_id = $1
                        "#,
                        workflow_step.workflow_step_id,
                        error_json
                    )
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| {
                        TaskerError::DatabaseError(format!(
                            "Failed to mark step {} as permanently failed: {}",
                            workflow_step.workflow_step_id, e
                        ))
                    })?;

                    warn!(
                        step_id = workflow_step.workflow_step_id,
                        error = step_result.error,
                        retry_limit_exceeded = workflow_step.has_exceeded_retry_limit(),
                        "Marked step as permanently failed"
                    );
                }
            }
            _ => {
                warn!(
                    step_id = step_result.step_id,
                    status = step_result.status,
                    "Unknown step result status, skipping update"
                );
            }
        }

        Ok(())
    }

    /// Mark a task as complete
    async fn mark_task_complete(&self, task_id: i64) -> Result<()> {
        // Update task status to completed
        sqlx::query!(
            r#"
            UPDATE tasker_tasks 
            SET complete = true,
                updated_at = NOW()
            WHERE task_id = $1
            "#,
            task_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            TaskerError::DatabaseError(format!(
                "Failed to mark task {} as complete: {}",
                task_id, e
            ))
        })?;

        info!(task_id = task_id, "Task marked as complete");

        Ok(())
    }

    /// Enqueue next batch of ready steps for a task
    async fn enqueue_next_batch_for_task(&self, task_id: i64) -> Result<()> {
        // Create a task processing message to trigger next batch creation
        let task_message = TaskProcessingMessage {
            task_id,
            namespace: "continuation".to_string(), // Will be updated by workflow coordinator
            task_name: "task_continuation".to_string(), // Will be updated by workflow coordinator
            task_version: "1.0.0".to_string(),
            priority: crate::messaging::TaskPriority::Normal,
            metadata: crate::messaging::TaskProcessingMetadata {
                enqueued_at: chrono::Utc::now(),
                request_id: format!(
                    "continuation-{}-{}",
                    task_id,
                    chrono::Utc::now().timestamp()
                ),
                processing_attempts: 0,
                retry_after: None,
            },
        };

        // Send the message to the tasks queue for processing
        self.pgmq_client
            .send_json_message(&self.config.tasks_queue_name, &task_message)
            .await
            .map_err(|e| {
                TaskerError::MessagingError(format!(
                    "Failed to enqueue continuation for task {}: {}",
                    task_id, e
                ))
            })?;

        debug!(
            task_id = task_id,
            queue = %self.config.tasks_queue_name,
            "Enqueued task continuation message"
        );

        Ok(())
    }

    /// Initialize all required queues
    async fn initialize_queues(&self) -> Result<()> {
        info!("Initializing orchestration queues");

        // Create main orchestration queues
        let main_queues = vec![
            &self.config.tasks_queue_name,
            &self.config.results_queue_name,
        ];
        let main_queue_count = main_queues.len();

        for queue_name in main_queues {
            if let Err(e) = self.pgmq_client.create_queue(queue_name).await {
                debug!(
                    queue = %queue_name,
                    error = %e,
                    "Queue may already exist"
                );
            }
        }

        // Create namespace-specific batch queues
        for namespace in &self.config.active_namespaces {
            let batch_queue_name = format!("{}_batch_queue", namespace);
            if let Err(e) = self.pgmq_client.create_queue(&batch_queue_name).await {
                debug!(
                    queue = %batch_queue_name,
                    error = %e,
                    "Batch queue may already exist"
                );
            }
        }

        info!(
            main_queues = main_queue_count,
            namespace_queues = self.config.active_namespaces.len(),
            "All orchestration queues initialized"
        );

        Ok(())
    }

    /// Get orchestration statistics
    pub async fn get_statistics(&self) -> Result<OrchestrationStats> {
        // For now, return basic stats
        // TODO: Implement actual queue size queries
        Ok(OrchestrationStats {
            tasks_queue_size: -1,   // Not available yet
            results_queue_size: -1, // Not available yet
            active_namespaces: self.config.active_namespaces.clone(),
            tasks_queue_name: self.config.tasks_queue_name.clone(),
            results_queue_name: self.config.results_queue_name.clone(),
        })
    }

    /// Clone for task processing (to avoid Arc<Arc<>> issues)
    fn clone_for_task_processing(&self) -> OrchestrationSystemPgmq {
        OrchestrationSystemPgmq {
            pgmq_client: self.pgmq_client.clone(),
            task_request_processor: self.task_request_processor.clone(),
            workflow_coordinator: self.workflow_coordinator.clone(),
            task_handler_registry: self.task_handler_registry.clone(),
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }

    /// Clone for result processing (to avoid Arc<Arc<>> issues)
    fn clone_for_result_processing(&self) -> OrchestrationSystemPgmq {
        OrchestrationSystemPgmq {
            pgmq_client: self.pgmq_client.clone(),
            task_request_processor: self.task_request_processor.clone(),
            workflow_coordinator: self.workflow_coordinator.clone(),
            task_handler_registry: self.task_handler_registry.clone(),
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }
}

/// Calculate exponential backoff seconds for retry attempts
pub fn calculate_backoff_seconds(attempt_count: i32) -> i32 {
    // Exponential backoff: 2^attempt seconds, capped at 300 seconds (5 minutes)
    let base_seconds = 2_i32.pow(attempt_count.min(8) as u32); // Cap at 2^8 = 256 seconds
    base_seconds.min(300) // Cap at 5 minutes
}

/// Flexible batch result message that can handle Ruby and Rust formats
#[derive(Debug, Clone)]
struct FlexibleBatchResultMessage {
    pub batch_id: i64,
    pub task_id: i64,
    pub namespace: String,
    pub batch_status: String,
    pub step_results: Vec<FlexibleStepResult>,
    pub successful_steps: i32,
    pub failed_steps: i32,
}

/// Flexible step result that can handle Ruby and Rust formats
#[derive(Debug, Clone)]
struct FlexibleStepResult {
    pub step_id: i64,
    pub status: String,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub execution_duration_ms: i64,
    pub executed_at: String,
}

/// Statistics for the orchestration system
#[derive(Debug, Clone)]
pub struct OrchestrationStats {
    pub tasks_queue_size: i64,
    pub results_queue_size: i64,
    pub active_namespaces: Vec<String>,
    pub tasks_queue_name: String,
    pub results_queue_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = OrchestrationSystemConfig::default();

        assert_eq!(
            config.tasks_queue_name,
            "orchestration_tasks_to_be_processed"
        );
        assert_eq!(config.results_queue_name, "orchestration_batch_results");
        assert_eq!(config.task_polling_interval_seconds, 1);
        assert_eq!(config.result_polling_interval_seconds, 1);
        assert_eq!(config.task_batch_size, 10);
        assert_eq!(config.result_batch_size, 20);
        assert_eq!(config.visibility_timeout_seconds, 300);
        assert_eq!(config.max_processing_attempts, 3);

        // Verify default namespaces
        let expected_namespaces = vec![
            "fulfillment",
            "inventory",
            "notifications",
            "payments",
            "analytics",
        ];
        assert_eq!(config.active_namespaces.len(), expected_namespaces.len());
        for namespace in expected_namespaces {
            assert!(config.active_namespaces.contains(&namespace.to_string()));
        }
    }

    #[test]
    fn test_config_customization() {
        let config = OrchestrationSystemConfig {
            tasks_queue_name: "custom_tasks".to_string(),
            results_queue_name: "custom_results".to_string(),
            task_polling_interval_seconds: 5,
            result_polling_interval_seconds: 3,
            task_batch_size: 20,
            result_batch_size: 50,
            visibility_timeout_seconds: 600,
            max_processing_attempts: 5,
            active_namespaces: vec!["custom".to_string(), "test".to_string()],
        };

        assert_eq!(config.tasks_queue_name, "custom_tasks");
        assert_eq!(config.results_queue_name, "custom_results");
        assert_eq!(config.task_polling_interval_seconds, 5);
        assert_eq!(config.result_polling_interval_seconds, 3);
        assert_eq!(config.task_batch_size, 20);
        assert_eq!(config.result_batch_size, 50);
        assert_eq!(config.visibility_timeout_seconds, 600);
        assert_eq!(config.max_processing_attempts, 5);
        assert_eq!(config.active_namespaces, vec!["custom", "test"]);
    }
}
