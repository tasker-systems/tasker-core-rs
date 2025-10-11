//! Message Handler
//!
//! Handles different message types for step result processing.

use opentelemetry::KeyValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};
use uuid::Uuid;

use super::metadata_processor::MetadataProcessor;
use super::state_transition_handler::StateTransitionHandler;
use super::task_coordinator::TaskCoordinator;
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::messaging::StepExecutionStatus;
use tasker_shared::metrics::orchestration::*;
use tasker_shared::system_context::SystemContext;

use tasker_shared::messaging::{StepExecutionResult, StepResultMessage};
use tasker_shared::models::{Task, WorkflowStep};

/// Handles different message types for result processing
#[derive(Clone)]
pub struct MessageHandler {
    context: Arc<SystemContext>,
    metadata_processor: MetadataProcessor,
    state_transition_handler: StateTransitionHandler,
    task_coordinator: TaskCoordinator,
}

impl MessageHandler {
    pub fn new(
        context: Arc<SystemContext>,
        metadata_processor: MetadataProcessor,
        state_transition_handler: StateTransitionHandler,
        task_coordinator: TaskCoordinator,
    ) -> Self {
        Self {
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
        }
    }

    /// TAS-32: Handle step result notification with orchestration metadata (coordination only)
    ///
    /// TAS-32 ARCHITECTURE CHANGE: This method now only processes orchestration metadata
    /// for backoff calculations and task-level coordination. Ruby workers handle all
    /// step state transitions and result saving.
    ///
    /// The Rust orchestration focuses on:
    /// - Processing backoff metadata from workers
    /// - Task-level finalization coordination
    /// - Orchestration-level retry decisions
    pub async fn handle_step_result_message(
        &self,
        step_result: StepResultMessage,
    ) -> OrchestrationResult<()> {
        let step_uuid = &step_result.step_uuid;
        let status = &step_result.status;
        let execution_time_ms = &step_result.execution_time_ms;
        let correlation_id = &step_result.correlation_id;
        let orchestration_metadata = &step_result.orchestration_metadata;

        // TAS-29 Phase 3.3: Record step result processed metric
        let result_type = match status {
            StepExecutionStatus::Success => "success",
            StepExecutionStatus::Failed => "error",
            StepExecutionStatus::Timeout => "timeout",
            StepExecutionStatus::Cancelled => "cancelled",
            StepExecutionStatus::Skipped => "skipped",
        };

        if let Some(counter) = STEP_RESULTS_PROCESSED_TOTAL.get() {
            counter.add(
                1,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("result_type", result_type),
                ],
            );
        }

        // TAS-29 Phase 3.3: Start timing result processing
        let start_time = Instant::now();

        info!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            has_orchestration_metadata = orchestration_metadata.is_some(),
            "Starting step result notification processing"
        );

        // Process orchestration metadata for backoff decisions (coordinating retry timing)
        if let Some(metadata) = &orchestration_metadata {
            debug!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                headers_count = metadata.headers.len(),
                has_error_context = metadata.error_context.is_some(),
                has_backoff_hint = metadata.backoff_hint.is_some(),
                custom_fields_count = metadata.custom.len(),
                "Processing orchestration metadata"
            );

            match step_result.status {
                StepExecutionStatus::Failed => {
                    self.metadata_processor
                        .process_metadata(step_uuid, metadata, *correlation_id)
                        .await?;
                }
                StepExecutionStatus::Timeout => {
                    self.metadata_processor
                        .process_metadata(step_uuid, metadata, *correlation_id)
                        .await?;
                }
                _ => {}
            }
        } else {
            debug!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                "No orchestration metadata to process"
            );
        }

        // Delegate to coordination-only result handling (no state updates)
        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            "Delegating to coordination-only processing"
        );

        let result = match self
            .handle_step_result(
                &step_result.step_uuid,
                &step_result.status.clone().into(),
                &(step_result.execution_time_ms as i64),
                *correlation_id,
            )
            .await
        {
            Ok(()) => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    status = %status,
                    "Step result notification processing completed successfully"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    status = %status,
                    error = %e,
                    "Step result notification processing failed"
                );
                Err(e)
            }
        };

        // TAS-29 Phase 3.3: Record step result processing duration
        let duration_ms = start_time.elapsed().as_millis() as f64;
        if let Some(histogram) = STEP_RESULT_PROCESSING_DURATION.get() {
            histogram.record(
                duration_ms,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("result_type", result_type),
                ],
            );
        }

        result
    }

    /// Handle StepExecutionResult message type
    pub async fn handle_step_execution_result(
        &self,
        step_result: &StepExecutionResult,
    ) -> OrchestrationResult<()> {
        let step_uuid = &step_result.step_uuid;
        let status = &step_result.status;
        let execution_time_ms = &step_result.metadata.execution_time_ms;
        let orchestration_metadata = &step_result.orchestration_metadata;

        // Fetch correlation_id from task for distributed tracing
        let correlation_id = self.get_correlation_id_for_step(*step_uuid).await;

        info!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            processor_uuid = %self.context.processor_uuid(),
            "Step execution result notification processing completed successfully"
        );

        // Process orchestration metadata for backoff decisions (coordinating retry timing)
        if let Some(metadata) = &orchestration_metadata {
            debug!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                headers_count = metadata.headers.len(),
                has_error_context = metadata.error_context.is_some(),
                has_backoff_hint = metadata.backoff_hint.is_some(),
                custom_fields_count = metadata.custom.len(),
                "Processing orchestration metadata"
            );

            if let Err(e) = self
                .metadata_processor
                .process_metadata(step_uuid, metadata, correlation_id)
                .await
            {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to process orchestration metadata"
                );
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Successfully processed orchestration metadata"
                );
            }
        } else {
            debug!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                "No orchestration metadata to process"
            );
        }

        // Delegate to coordination-only result handling (no state updates)
        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            "Delegating to coordination-only processing"
        );

        match self
            .handle_step_result(
                &step_result.step_uuid,
                &step_result.status,
                &step_result.metadata.execution_time_ms,
                correlation_id,
            )
            .await
        {
            Ok(()) => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    status = %status,
                    "Step result notification processing completed successfully"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    status = %status,
                    error = %e,
                    "Step result notification processing failed"
                );
                Err(e)
            }
        }
    }

    /// Handle a partial result message (TAS-32: coordination only, no state updates)
    ///
    /// TAS-32 ARCHITECTURE CHANGE: Ruby workers now handle all step execution, result saving,
    /// and state transitions. Rust orchestration focuses only on task-level coordination.
    ///
    /// This method now only handles task finalization checks when steps complete,
    /// since Ruby workers manage step state transitions directly.
    async fn handle_step_result(
        &self,
        step_uuid: &Uuid,
        status: &String,
        execution_time_ms: &i64,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        info!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            status = %status,
            execution_time_ms = execution_time_ms,
            processor_uuid = %self.context.processor_uuid(),
            "Processing step result for orchestration and task coordination"
        );

        // Process orchestration state transition
        if let Err(e) = self
            .state_transition_handler
            .process_state_transition(step_uuid, status, correlation_id)
            .await
        {
            error!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                error = %e,
                "Failed to process orchestration state transition"
            );
            return Err(e);
        }

        // Coordinate task finalization
        self.task_coordinator
            .coordinate_task_finalization(step_uuid, status, correlation_id)
            .await
    }

    /// Get correlation_id for a step by looking up its task
    async fn get_correlation_id_for_step(&self, step_uuid: Uuid) -> Uuid {
        // First get the step to find its task_uuid
        match WorkflowStep::find_by_id(self.context.database_pool(), step_uuid).await {
            Ok(Some(workflow_step)) => {
                // Now get the task to extract correlation_id
                match Task::find_by_id(self.context.database_pool(), workflow_step.task_uuid).await
                {
                    Ok(Some(task)) => task.correlation_id,
                    Ok(None) | Err(_) => Uuid::nil(),
                }
            }
            Ok(None) | Err(_) => Uuid::nil(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::lifecycle::task_finalization::TaskFinalizer;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_message_handler_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);

        let backoff_config: crate::orchestration::BackoffCalculatorConfig =
            context.tasker_config.clone().into();
        let backoff_calculator = crate::orchestration::BackoffCalculator::new(
            backoff_config,
            context.database_pool().clone(),
        );
        let metadata_processor = MetadataProcessor::new(backoff_calculator);
        let state_transition_handler = StateTransitionHandler::new(context.clone());
        let task_coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        let handler = MessageHandler::new(
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
        );

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&handler.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_message_handler_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);

        let backoff_config: crate::orchestration::BackoffCalculatorConfig =
            context.tasker_config.clone().into();
        let backoff_calculator = crate::orchestration::BackoffCalculator::new(
            backoff_config,
            context.database_pool().clone(),
        );
        let metadata_processor = MetadataProcessor::new(backoff_calculator);
        let state_transition_handler = StateTransitionHandler::new(context.clone());
        let task_coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        let handler = MessageHandler::new(
            context.clone(),
            metadata_processor,
            state_transition_handler,
            task_coordinator,
        );

        let cloned = handler.clone();

        // Verify both share the same Arc
        assert_eq!(Arc::as_ptr(&handler.context), Arc::as_ptr(&cloned.context));
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_correlation_id_for_nonexistent_step(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(
            crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService::new(
                context.clone(),
            )
            .await?,
        );
        let task_finalizer = TaskFinalizer::new(context.clone(), step_enqueuer);

        let backoff_config: crate::orchestration::BackoffCalculatorConfig =
            context.tasker_config.clone().into();
        let backoff_calculator = crate::orchestration::BackoffCalculator::new(
            backoff_config,
            context.database_pool().clone(),
        );
        let metadata_processor = MetadataProcessor::new(backoff_calculator);
        let state_transition_handler = StateTransitionHandler::new(context.clone());
        let task_coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        let handler = MessageHandler::new(
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
        );

        let nonexistent_step = Uuid::new_v4();
        let correlation_id = handler.get_correlation_id_for_step(nonexistent_step).await;

        // Should return nil UUID for non-existent step
        assert_eq!(correlation_id, Uuid::nil());
        Ok(())
    }
}
