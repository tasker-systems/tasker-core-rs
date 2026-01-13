use super::task_template_manager::TaskTemplateManager;
use opentelemetry::KeyValue;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::metrics::worker::*;
use tasker_shared::models::{
    orchestration::StepTransitiveDependenciesQuery, task::Task, workflow_step::WorkflowStepWithName,
};
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::TaskSequenceStep;
use tasker_shared::{TaskerError, TaskerResult};
use uuid::Uuid;

use tracing::{debug, error};

#[derive(Clone)]
#[expect(dead_code, reason = "StepClaim struct used internally by worker step processing")]
pub(crate) struct StepClaim {
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub context: Arc<SystemContext>,
    pub task_template_manager: Arc<TaskTemplateManager>,
}

impl StepClaim {
    pub fn new(
        task_uuid: Uuid,
        step_uuid: Uuid,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> Self {
        StepClaim {
            task_uuid,
            step_uuid,
            context,
            task_template_manager,
        }
    }
    /// TAS-67: Get TaskSequenceStep by step UUID for dispatch mode
    ///
    /// This method hydrates the step context directly from the database
    /// without needing a full SimpleStepMessage.
    pub async fn get_task_sequence_step_by_uuid(
        &self,
        step_uuid: Uuid,
    ) -> TaskerResult<Option<TaskSequenceStep>> {
        debug!(
            task_uuid = %self.task_uuid,
            step_uuid = %step_uuid,
            "Worker: Hydrating step context for dispatch"
        );

        let db_pool = self.context.database_pool();

        // 1. Fetch task data from database using task_uuid
        let task = match Task::find_by_id(db_pool, self.task_uuid).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                return Err(TaskerError::WorkerError(format!(
                    "Task not found: {}",
                    self.task_uuid
                )));
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch task: {e}"
                )));
            }
        };

        let task_for_orchestration = task.for_orchestration(db_pool).await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to fetch task for orchestration: {e}"))
        })?;

        // Use task_handler_registry to get task template
        let task_template = self
            .context
            .task_handler_registry
            .get_task_template(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to get task template: {e}")))?;

        // Verify task template manager knows about this task
        let _metadata = self
            .task_template_manager
            .get_handler_metadata(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .await?;

        // 2. Fetch workflow step from database using step_uuid
        let workflow_step = match WorkflowStepWithName::find_by_id(db_pool, step_uuid).await {
            Ok(Some(step)) => step,
            Ok(None) => {
                return Err(TaskerError::WorkerError(format!(
                    "Workflow step not found: {}",
                    step_uuid
                )));
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch workflow step: {e}"
                )));
            }
        };

        // Find the step definition in the task template
        let step_definition = task_template
            .steps
            .iter()
            .find(|step| step.name == workflow_step.template_step_name)
            .ok_or_else(|| {
                TaskerError::WorkerError(format!(
                    "Step definition not found in task template for step '{}' (template: '{}')",
                    workflow_step.name, workflow_step.template_step_name
                ))
            })?;

        // Get transitive dependencies and build execution context
        let deps_query = StepTransitiveDependenciesQuery::new(db_pool.clone());
        let dependency_results = deps_query.get_results_map(step_uuid).await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to get dependency results: {e}"))
        })?;

        Ok(Some(TaskSequenceStep {
            task: task_for_orchestration,
            workflow_step,
            dependency_results,
            step_definition: step_definition.clone(),
        }))
    }

    pub async fn get_task_sequence_step_from_step_message(
        &self,
        message: &SimpleStepMessage,
    ) -> TaskerResult<Option<TaskSequenceStep>> {
        debug!(
            task_uuid = %message.task_uuid,
            step_uuid = %message.step_uuid,
            correlation_id = %message.correlation_id,
            "Worker: Processing step message"
        );

        let db_pool = self.context.database_pool();
        // 1. Fetch task data from database using task_uuid
        let task = match Task::find_by_id(db_pool, message.task_uuid).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                return Err(TaskerError::WorkerError(format!(
                    "Task not found: {}",
                    message.task_uuid
                )));
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch task: {e}"
                )));
            }
        };

        let task_for_orchestration = task.for_orchestration(db_pool).await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to fetch task for orchestration: {e}"))
        })?;

        // Use task_handler_registry to get task template and handler information
        let task_template = self
            .context
            .task_handler_registry
            .get_task_template(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to get task template: {e}")))?;

        // we are checking this because it let's us throw an error
        // if our worker level task template manager does not know about this task template
        // even if the db level registry does know about it

        let _metadata = self
            .task_template_manager
            .get_handler_metadata(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .await?;

        // 2. Fetch workflow step from database using step_uuid
        let workflow_step = match WorkflowStepWithName::find_by_id(db_pool, message.step_uuid).await
        {
            Ok(Some(step)) => step,
            Ok(None) => {
                return Err(TaskerError::WorkerError(format!(
                    "Workflow step not found: {}",
                    message.step_uuid
                )));
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch workflow step: {e}"
                )));
            }
        };

        // Find the step definition in the task template using template_step_name
        // For regular steps, template_step_name equals name
        // For dynamically created steps (batch workers, decision point steps),
        // template_step_name contains the template name from inputs->>'__template_step_name'
        let step_definition = task_template
            .steps
            .iter()
            .find(|step| step.name == workflow_step.template_step_name)
            .ok_or_else(|| {
                TaskerError::WorkerError(format!(
                    "Step definition not found in task template for step '{}' (template: '{}')",
                    workflow_step.name, workflow_step.template_step_name
                ))
            })?;

        // Get transitive dependencies and build execution context
        let deps_query = StepTransitiveDependenciesQuery::new(db_pool.clone());
        let dependency_results = deps_query
            .get_results_map(message.step_uuid)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to get dependency results: {e}"))
            })?;

        Ok(Some(TaskSequenceStep {
            task: task_for_orchestration,
            workflow_step,
            dependency_results,
            step_definition: step_definition.clone(),
        }))
    }

    /// Static method to try claiming a step using state machine transitions
    pub async fn try_claim_step(
        &self,
        task_sequence_step: &TaskSequenceStep,
        correlation_id: Uuid,
    ) -> TaskerResult<bool> {
        let step_uuid = task_sequence_step.workflow_step.workflow_step_uuid;

        // TAS-29 Phase 3.3: Start timing step claim
        let start_time = Instant::now();

        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            "Attempting to claim step using state machine transition"
        );

        // Create a WorkflowStep to pass to the state machine

        // Create a state machine for this step
        let mut state_machine = StepStateMachine::new(
            task_sequence_step.workflow_step.clone().into(),
            self.context.clone(),
        );

        // Try to transition the step to in_progress (claiming it)
        // We'll first check the current state and then attempt the appropriate transition
        match state_machine.current_state().await {
            Ok(current_state) => {
                let transition_event = match current_state {
                    WorkflowStepState::Pending => StepEvent::Start, // Legacy transition
                    WorkflowStepState::Enqueued => StepEvent::Start, // TAS-32 compliant transition
                    _ => {
                        debug!(
                            correlation_id = %correlation_id,
                            step_uuid = %step_uuid,
                            current_state = %current_state,
                            "Step is not in a claimable state"
                        );
                        return Ok(false);
                    }
                };

                match state_machine.transition(transition_event).await {
                    Ok(new_state) => {
                        if matches!(new_state, WorkflowStepState::InProgress) {
                            // Increment attempts count and set last_attempted_at
                            // This marks the start of an execution attempt, including the claim itself.
                            // From a distributed systems perspective, claiming = attempt begins.
                            // This ensures worker crashes/timeouts count against retry limits.
                            let db_pool = self.context.database_pool();
                            let now = chrono::Utc::now().naive_utc();

                            sqlx::query!(
                                r#"
                                UPDATE tasker.workflow_steps
                                SET attempts = COALESCE(attempts, 0) + 1,
                                    last_attempted_at = $2,
                                    updated_at = NOW()
                                WHERE workflow_step_uuid = $1
                                "#,
                                step_uuid,
                                now
                            )
                            .execute(db_pool)
                            .await
                            .map_err(|e| {
                                TaskerError::DatabaseError(format!(
                                    "Failed to increment attempts for step {}: {}",
                                    step_uuid, e
                                ))
                            })?;

                            debug!(
                                correlation_id = %correlation_id,
                                step_uuid = %step_uuid,
                                new_state = %new_state,
                                "Successfully claimed step by transitioning to InProgress and incremented attempts"
                            );

                            // TAS-29 Phase 3.3: Record successful step claim
                            if let Some(counter) = STEPS_CLAIMED_TOTAL.get() {
                                counter.add(
                                    1,
                                    &[
                                        KeyValue::new(
                                            "namespace",
                                            task_sequence_step.task.namespace_name.clone(),
                                        ),
                                        KeyValue::new("claim_method", "event"), // or "poll" depending on context
                                    ],
                                );
                            }

                            // TAS-29 Phase 3.3: Record claim duration
                            let duration_ms = start_time.elapsed().as_millis() as f64;
                            if let Some(histogram) = STEP_CLAIM_DURATION.get() {
                                histogram.record(
                                    duration_ms,
                                    &[
                                        KeyValue::new(
                                            "namespace",
                                            task_sequence_step.task.namespace_name.clone(),
                                        ),
                                        KeyValue::new("claim_method", "event"),
                                    ],
                                );
                            }

                            Ok(true)
                        } else {
                            debug!(
                                correlation_id = %correlation_id,
                                step_uuid = %step_uuid,
                                new_state = %new_state,
                                "Unexpected state after transition"
                            );
                            Ok(false)
                        }
                    }
                    Err(e) => {
                        debug!(
                            correlation_id = %correlation_id,
                            step_uuid = %step_uuid,
                            error = %e,
                            "Failed to claim step - likely already claimed by another worker"
                        );
                        // This is expected when another worker has already claimed the step
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to get current state for step claiming"
                );
                Err(TaskerError::StateTransitionError(format!(
                    "Failed to get current state for step claiming, {e}"
                )))
            }
        }
    }
}
