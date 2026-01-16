//! # FFI Completion Service
//!
//! TAS-69: Step completion processing extracted from command_processor.rs.
//!
//! This service handles:
//! - Processing step execution results from FFI handlers
//! - State machine transitions for step completion
//! - Sending completion notifications to orchestration
//! - TAS-62 audit context enrichment

use std::sync::Arc;

use tracing::{debug, info};
use uuid::Uuid;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::models::{Task, WorkflowStep};
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::TransitionContext;
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::worker::orchestration_result_sender::OrchestrationResultSender;

/// FFI Completion Service
///
/// TAS-69: Extracted from command_processor.rs to provide focused step
/// completion handling.
///
/// This service encapsulates:
/// - Step result processing with state machine integration
/// - TAS-62 audit context enrichment (worker_uuid, correlation_id)
/// - Orchestration notification via OrchestrationResultSender
pub struct FFICompletionService {
    /// Worker identification
    worker_id: String,

    /// System context for dependencies and database access
    context: Arc<SystemContext>,

    /// Orchestration result sender for step completion notifications
    orchestration_result_sender: OrchestrationResultSender,
}

impl std::fmt::Debug for FFICompletionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FFICompletionService")
            .field("worker_id", &self.worker_id)
            .finish()
    }
}

impl FFICompletionService {
    /// Create a new FFICompletionService
    pub fn new(worker_id: String, context: Arc<SystemContext>) -> Self {
        let queues_config = &context.tasker_config.common.queues;
        let orchestration_result_sender =
            OrchestrationResultSender::new(context.message_client(), queues_config);

        Self {
            worker_id,
            context,
            orchestration_result_sender,
        }
    }

    /// Send step result to orchestration
    ///
    /// TAS-62: This method enriches state transitions with attribution context
    /// (worker_uuid, correlation_id) for SOC2-compliant audit trails.
    ///
    /// # Flow
    /// 1. Load WorkflowStep from database
    /// 2. Fetch task for correlation_id (TAS-62 attribution)
    /// 3. Transition step state via StepStateMachine
    /// 4. Send StepMessage to orchestration queue
    pub async fn send_step_result(&self, step_result: StepExecutionResult) -> TaskerResult<()> {
        debug!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            success = step_result.success,
            "Processing step result with state machine integration"
        );

        let db_pool = self.context.database_pool();

        // 1. Load WorkflowStep from database
        let workflow_step = WorkflowStep::find_by_id(db_pool, step_result.step_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to lookup step: {e}")))?
            .ok_or_else(|| {
                TaskerError::WorkerError(format!("Step not found: {}", step_result.step_uuid))
            })?;

        // 2. Fetch task for correlation_id (TAS-62)
        let task_uuid = workflow_step.task_uuid;
        let task = Task::find_by_id(db_pool, task_uuid)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch task: {e}")))?
            .ok_or_else(|| TaskerError::WorkerError(format!("Task not found: {}", task_uuid)))?;
        let correlation_id = task.correlation_id;

        // 3. Transition step state via StepStateMachine
        let mut state_machine = StepStateMachine::new(workflow_step, self.context.clone());

        let step_event = if step_result.success {
            StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
        } else {
            StepEvent::EnqueueAsErrorForOrchestration(Some(serde_json::to_value(&step_result)?))
        };

        // TAS-62: Create attribution context for audit enrichment
        let worker_uuid = self.parse_worker_uuid()?;
        let transition_context = TransitionContext::with_worker(worker_uuid, Some(correlation_id));

        // Execute atomic state transition with attribution context
        let final_state = state_machine
            .transition_with_context(step_event, Some(transition_context))
            .await
            .map_err(|e| {
                TaskerError::StateTransitionError(format!("Step transition failed: {e}"))
            })?;

        // 4. Send StepMessage to orchestration queue
        self.orchestration_result_sender
            .send_completion(task_uuid, step_result.step_uuid, correlation_id)
            .await?;

        info!(
            worker_id = %self.worker_id,
            step_uuid = %step_result.step_uuid,
            correlation_id = %correlation_id,
            final_state = %final_state,
            "Step result processed and sent to orchestration successfully"
        );

        Ok(())
    }

    /// Parse worker UUID from worker_id string
    fn parse_worker_uuid(&self) -> TaskerResult<Uuid> {
        let uuid_str = self
            .worker_id
            .strip_prefix("worker_")
            .unwrap_or(&self.worker_id);

        Uuid::parse_str(uuid_str).map_err(|e| {
            TaskerError::WorkerError(format!("Invalid worker_id UUID '{}': {e}", self.worker_id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_ffi_completion_service_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let worker_id = format!("worker_{}", Uuid::new_v4());
        let service = FFICompletionService::new(worker_id.clone(), context);

        assert_eq!(service.worker_id, worker_id);
    }

    #[test]
    fn test_parse_worker_uuid() {
        // This test verifies UUID parsing without database
        let uuid = Uuid::new_v4();
        let worker_id = format!("worker_{}", uuid);

        let uuid_str = worker_id.strip_prefix("worker_").unwrap();
        let parsed = Uuid::parse_str(uuid_str).unwrap();

        assert_eq!(parsed, uuid);
    }
}
