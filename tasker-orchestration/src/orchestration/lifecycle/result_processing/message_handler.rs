//! Message Handler
//!
//! Handles different message types for step result processing.

use opentelemetry::KeyValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::metadata_processor::MetadataProcessor;
use super::state_transition_handler::StateTransitionHandler;
use super::task_coordinator::TaskCoordinator;
use crate::actors::batch_processing_actor::BatchProcessingActor;
use crate::actors::decision_point_actor::DecisionPointActor;
use crate::actors::{Handler, ProcessBatchableStepMessage, ProcessDecisionPointMessage};
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::messaging::{BatchProcessingOutcome, DecisionPointOutcome, StepExecutionStatus};
use tasker_shared::metrics::orchestration::*;
use tasker_shared::models::core::task_template::{StepType, TaskTemplate};
use tasker_shared::system_context::SystemContext;

use tasker_shared::messaging::{StepExecutionResult, StepResultMessage};
use tasker_shared::models::{NamedStep, Task, WorkflowStep};

/// Handles different message types for result processing
///
/// TAS-53 Phase 6: Now includes decision point detection and dynamic step creation
/// TAS-59 Phase 4: Now includes batch processing outcome detection and worker creation
#[derive(Clone, Debug)]
pub struct MessageHandler {
    context: Arc<SystemContext>,
    metadata_processor: MetadataProcessor,
    state_transition_handler: StateTransitionHandler,
    task_coordinator: TaskCoordinator,
    /// TAS-53 Phase 6: Decision point actor for dynamic workflow step creation
    decision_point_actor: Arc<DecisionPointActor>,
    /// TAS-59 Phase 4: Batch processing actor for dynamic batch worker creation
    batch_processing_actor: Arc<BatchProcessingActor>,
}

impl MessageHandler {
    /// Create a new MessageHandler
    ///
    /// TAS-53 Phase 6: Now accepts DecisionPointActor for dynamic workflow step creation
    /// TAS-59 Phase 4: Now accepts BatchProcessingActor for dynamic batch worker creation
    pub fn new(
        context: Arc<SystemContext>,
        metadata_processor: MetadataProcessor,
        state_transition_handler: StateTransitionHandler,
        task_coordinator: TaskCoordinator,
        decision_point_actor: Arc<DecisionPointActor>,
        batch_processing_actor: Arc<BatchProcessingActor>,
    ) -> Self {
        Self {
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
            decision_point_actor,
            batch_processing_actor,
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

        // TAS-53 Phase 6: Check for decision point completion and process outcome
        if let Err(e) = self
            .process_decision_point_if_needed(step_uuid, status, correlation_id)
            .await
        {
            warn!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                error = %e,
                "Failed to process decision point outcome (non-fatal, continuing with task coordination)"
            );
        }

        // TAS-59 Phase 4: Check for batch processing outcome and create workers
        if let Err(e) = self
            .process_batch_outcome_if_needed(step_uuid, status, correlation_id)
            .await
        {
            warn!(
                correlation_id = %correlation_id,
                step_uuid = %step_uuid,
                error = %e,
                "Failed to process batch processing outcome (non-fatal, continuing with task coordination)"
            );
        }

        // Coordinate task finalization
        self.task_coordinator
            .coordinate_task_finalization(step_uuid, status, correlation_id)
            .await
    }

    /// TAS-53 Phase 6: Process decision point outcome if this step is a decision point
    ///
    /// This method:
    /// 1. Loads the workflow step and determines if it's a decision point
    /// 2. If it's a successful decision step, extracts the DecisionPointOutcome
    /// 3. Sends the outcome to DecisionPointActor for dynamic step creation
    ///
    /// Note: Errors are logged but don't fail the overall result processing,
    /// as decision point processing is an enhancement that shouldn't block
    /// the core result processing flow.
    async fn process_decision_point_if_needed(
        &self,
        step_uuid: &Uuid,
        status: &String,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        // Only process successful completions
        if status != "completed" {
            return Ok(());
        }

        // Load the workflow step
        let workflow_step =
            match WorkflowStep::find_by_id(self.context.database_pool(), *step_uuid).await? {
                Some(step) => step,
                None => {
                    debug!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        "Workflow step not found for decision point check"
                    );
                    return Ok(());
                }
            };

        // Check if this is a decision point by loading the task template
        let is_decision = self
            .is_decision_step(&workflow_step, correlation_id)
            .await?;
        if !is_decision {
            return Ok(());
        }

        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            "Detected decision point step completion, extracting outcome"
        );

        // Extract the decision outcome from the step results
        let outcome = match workflow_step.get_step_execution_result() {
            Some(result) => match DecisionPointOutcome::from_step_result(&result) {
                Some(outcome) => outcome,
                None => {
                    warn!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        "Decision point step has no valid DecisionPointOutcome in results"
                    );
                    return Ok(());
                }
            },
            None => {
                warn!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Decision point step has no results"
                );
                return Ok(());
            }
        };

        info!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            requires_creation = outcome.requires_step_creation(),
            step_count = outcome.step_names().len(),
            "Processing decision point outcome"
        );

        // Send to DecisionPointActor for processing
        let msg = ProcessDecisionPointMessage {
            workflow_step_uuid: *step_uuid,
            task_uuid: workflow_step.task_uuid,
            outcome,
        };

        match self.decision_point_actor.handle(msg).await {
            Ok(_result) => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Decision point processing completed successfully"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Decision point processing failed"
                );
                Err(tasker_shared::OrchestrationError::from(
                    e.to_string().as_str(),
                ))
            }
        }
    }

    /// Check if a workflow step is a decision point by loading the task template
    ///
    /// This method:
    /// 1. Loads the task to get namespace/task name/version
    /// 2. Loads the task template from the handler registry
    /// 3. Finds the step definition by matching the named step name
    /// 4. Checks if the step definition is a decision step
    async fn is_decision_step(
        &self,
        workflow_step: &WorkflowStep,
        correlation_id: Uuid,
    ) -> OrchestrationResult<bool> {
        // Load the task to get metadata
        let task =
            match Task::find_by_id(self.context.database_pool(), workflow_step.task_uuid).await? {
                Some(task) => task,
                None => {
                    debug!(
                        correlation_id = %correlation_id,
                        task_uuid = %workflow_step.task_uuid,
                        "Task not found for decision point check"
                    );
                    return Ok(false);
                }
            };

        // Get task orchestration metadata
        let task_metadata = task
            .for_orchestration(self.context.database_pool())
            .await
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to load task orchestration metadata: {}", e).as_str(),
                )
            })?;

        // Load the named step to get the step name
        let named_step = match NamedStep::find_by_uuid(
            self.context.database_pool(),
            workflow_step.named_step_uuid,
        )
        .await?
        {
            Some(step) => step,
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    named_step_uuid = %workflow_step.named_step_uuid,
                    "Named step not found for decision point check"
                );
                return Ok(false);
            }
        };

        // Load the task template from handler registry
        let handler_metadata = self
            .context
            .task_handler_registry
            .get_task_template_from_registry(
                &task_metadata.namespace_name,
                &task_metadata.task_name,
                &task_metadata.task_version,
            )
            .await
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to load task template from registry: {}", e).as_str(),
                )
            })?;

        // Extract TaskTemplate from handler metadata
        let task_template: TaskTemplate =
            serde_json::from_value(handler_metadata.config_schema.ok_or_else(|| {
                tasker_shared::OrchestrationError::from(
                    "No config schema found in handler metadata",
                )
            })?)
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to deserialize task template: {}", e).as_str(),
                )
            })?;

        // Find the step definition in the template
        let step_def = task_template
            .steps
            .iter()
            .find(|s| s.name == named_step.name);

        match step_def {
            Some(def) => Ok(def.is_decision()),
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    step_name = %named_step.name,
                    "Step definition not found in template"
                );
                Ok(false)
            }
        }
    }

    /// TAS-59 Phase 4: Process batch processing outcome if this step is batchable
    ///
    /// This method:
    /// 1. Loads the workflow step and determines if it's a batchable step
    /// 2. If it's a successful batchable step, extracts the BatchProcessingOutcome
    /// 3. Sends the outcome to BatchProcessingActor for dynamic worker creation
    ///
    /// Note: Errors are logged but don't fail the overall result processing,
    /// as batch processing is an enhancement that shouldn't block
    /// the core result processing flow.
    async fn process_batch_outcome_if_needed(
        &self,
        step_uuid: &Uuid,
        status: &String,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        // Only process successful completions
        if status != "completed" {
            return Ok(());
        }

        // Load the workflow step
        let workflow_step =
            match WorkflowStep::find_by_id(self.context.database_pool(), *step_uuid).await? {
                Some(step) => step,
                None => {
                    debug!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        "Workflow step not found for batch processing check"
                    );
                    return Ok(());
                }
            };

        // Check if this is a batchable step by loading the task template
        let is_batchable = self
            .is_batchable_step(&workflow_step, correlation_id)
            .await?;
        if !is_batchable {
            return Ok(());
        }

        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            "Detected batchable step completion, extracting outcome"
        );

        // Extract the step execution result
        let step_result = match workflow_step.get_step_execution_result() {
            Some(result) => result,
            None => {
                warn!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Batchable step has no results"
                );
                return Ok(());
            }
        };

        // Check if batch processing outcome exists
        let outcome = match BatchProcessingOutcome::from_step_result(&step_result) {
            Some(outcome) => outcome,
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Batchable step has no valid BatchProcessingOutcome in results"
                );
                return Ok(());
            }
        };

        // Log batch processing outcome
        match &outcome {
            BatchProcessingOutcome::NoBatches => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Batchable step determined no batches needed - will create placeholder worker"
                );
            }
            BatchProcessingOutcome::CreateBatches {
                worker_count,
                total_items,
                ..
            } => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    worker_count = %worker_count,
                    total_items = %total_items,
                    "Processing batch worker creation"
                );
            }
        }

        // Send to BatchProcessingActor for processing
        let msg = ProcessBatchableStepMessage {
            task_uuid: workflow_step.task_uuid,
            batchable_step: workflow_step,
            step_result,
        };

        match self.batch_processing_actor.handle(msg).await {
            Ok(_result) => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    "Batch processing completed successfully"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Batch processing failed"
                );
                Err(tasker_shared::OrchestrationError::from(
                    e.to_string().as_str(),
                ))
            }
        }
    }

    /// Check if a workflow step is batchable by loading the task template
    ///
    /// This method:
    /// 1. Loads the task to get namespace/task name/version
    /// 2. Loads the task template from the handler registry
    /// 3. Finds the step definition by matching the named step name
    /// 4. Checks if the step definition is a batchable step
    async fn is_batchable_step(
        &self,
        workflow_step: &WorkflowStep,
        correlation_id: Uuid,
    ) -> OrchestrationResult<bool> {
        // Load the task to get metadata
        let task =
            match Task::find_by_id(self.context.database_pool(), workflow_step.task_uuid).await? {
                Some(task) => task,
                None => {
                    debug!(
                        correlation_id = %correlation_id,
                        task_uuid = %workflow_step.task_uuid,
                        "Task not found for batchable check"
                    );
                    return Ok(false);
                }
            };

        // Get task orchestration metadata
        let task_metadata = task
            .for_orchestration(self.context.database_pool())
            .await
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to load task orchestration metadata: {}", e).as_str(),
                )
            })?;

        // Load the named step to get the step name
        let named_step = match NamedStep::find_by_uuid(
            self.context.database_pool(),
            workflow_step.named_step_uuid,
        )
        .await?
        {
            Some(step) => step,
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    named_step_uuid = %workflow_step.named_step_uuid,
                    "Named step not found for batchable check"
                );
                return Ok(false);
            }
        };

        // Load the task template from handler registry
        let handler_metadata = self
            .context
            .task_handler_registry
            .get_task_template_from_registry(
                &task_metadata.namespace_name,
                &task_metadata.task_name,
                &task_metadata.task_version,
            )
            .await
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to load task template from registry: {}", e).as_str(),
                )
            })?;

        // Extract TaskTemplate from handler metadata
        let task_template: TaskTemplate =
            serde_json::from_value(handler_metadata.config_schema.ok_or_else(|| {
                tasker_shared::OrchestrationError::from(
                    "No config schema found in handler metadata",
                )
            })?)
            .map_err(|e| {
                tasker_shared::OrchestrationError::from(
                    format!("Failed to deserialize task template: {}", e).as_str(),
                )
            })?;

        // Find the step definition in the template
        let step_def = task_template
            .steps
            .iter()
            .find(|s| s.name == named_step.name);

        match step_def {
            Some(def) => Ok(def.step_type == StepType::Batchable),
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    step_name = %named_step.name,
                    "Step definition not found in template"
                );
                Ok(false)
            }
        }
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

        // Create DecisionPointActor for testing (TAS-53 Phase 6)
        let decision_point_service = Arc::new(
            crate::orchestration::lifecycle::DecisionPointService::new(context.clone()),
        );
        let decision_point_actor = Arc::new(
            crate::actors::decision_point_actor::DecisionPointActor::new(
                context.clone(),
                decision_point_service,
            ),
        );

        // Create BatchProcessingActor for testing (TAS-59 Phase 4)
        let batch_processing_service = Arc::new(
            crate::orchestration::lifecycle::batch_processing::BatchProcessingService::new(
                context.clone(),
            ),
        );
        let batch_processing_actor = Arc::new(
            crate::actors::batch_processing_actor::BatchProcessingActor::new(
                context.clone(),
                batch_processing_service,
            ),
        );

        let handler = MessageHandler::new(
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
            decision_point_actor,
            batch_processing_actor,
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

        // Create DecisionPointActor for testing (TAS-53 Phase 6)
        let decision_point_service = Arc::new(
            crate::orchestration::lifecycle::DecisionPointService::new(context.clone()),
        );
        let decision_point_actor = Arc::new(
            crate::actors::decision_point_actor::DecisionPointActor::new(
                context.clone(),
                decision_point_service,
            ),
        );

        // Create BatchProcessingActor for testing (TAS-59 Phase 4)
        let batch_processing_service = Arc::new(
            crate::orchestration::lifecycle::batch_processing::BatchProcessingService::new(
                context.clone(),
            ),
        );
        let batch_processing_actor = Arc::new(
            crate::actors::batch_processing_actor::BatchProcessingActor::new(
                context.clone(),
                batch_processing_service,
            ),
        );

        let handler = MessageHandler::new(
            context.clone(),
            metadata_processor,
            state_transition_handler,
            task_coordinator,
            decision_point_actor,
            batch_processing_actor,
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

        // Create DecisionPointActor for testing (TAS-53 Phase 6)
        let decision_point_service = Arc::new(
            crate::orchestration::lifecycle::DecisionPointService::new(context.clone()),
        );
        let decision_point_actor = Arc::new(
            crate::actors::decision_point_actor::DecisionPointActor::new(
                context.clone(),
                decision_point_service,
            ),
        );

        // Create BatchProcessingActor for testing (TAS-59 Phase 4)
        let batch_processing_service = Arc::new(
            crate::orchestration::lifecycle::batch_processing::BatchProcessingService::new(
                context.clone(),
            ),
        );
        let batch_processing_actor = Arc::new(
            crate::actors::batch_processing_actor::BatchProcessingActor::new(
                context.clone(),
                batch_processing_service,
            ),
        );

        let handler = MessageHandler::new(
            context,
            metadata_processor,
            state_transition_handler,
            task_coordinator,
            decision_point_actor,
            batch_processing_actor,
        );

        let nonexistent_step = Uuid::new_v4();
        let correlation_id = handler.get_correlation_id_for_step(nonexistent_step).await;

        // Should return nil UUID for non-existent step
        assert_eq!(correlation_id, Uuid::nil());
        Ok(())
    }
}
