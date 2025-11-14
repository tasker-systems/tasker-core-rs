//! Orchestration Result Processor Service
//!
//! TAS-32: Orchestration Coordination Logic (No State Management)
//!
//! Main service for orchestrating result processing with focused components.

use std::sync::Arc;

use crate::actors::batch_processing_actor::BatchProcessingActor;
use crate::actors::decision_point_actor::DecisionPointActor;
use crate::orchestration::lifecycle::task_finalization::TaskFinalizer;
use crate::orchestration::{BackoffCalculator, BackoffCalculatorConfig};
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::messaging::{StepExecutionResult, StepResultMessage};
use tasker_shared::system_context::SystemContext;

use super::message_handler::MessageHandler;
use super::metadata_processor::MetadataProcessor;
use super::state_transition_handler::StateTransitionHandler;
use super::task_coordinator::TaskCoordinator;

/// TAS-32: Orchestration coordination processor (coordination only, no state management)
///
/// **ARCHITECTURE CHANGE**: This component now handles only task-level coordination
/// and orchestration metadata processing. Ruby workers manage all step-level operations.
///
/// **Coordination Responsibilities**:
/// - Task finalization when steps complete
/// - Processing backoff metadata from workers
/// - Orchestration-level retry timing decisions
/// - Cross-cutting orchestration concerns
///
/// **No Longer Handles**:
/// - Step state transitions (Ruby workers + Statesman)
/// - Step result persistence (Ruby MessageManager)
/// - Workflow step transition creation (Ruby state machines)
///
/// **TAS-59 Phase 4**: Now processes batch processing outcomes for dynamic worker creation
#[derive(Clone, Debug)]
pub struct OrchestrationResultProcessor {
    message_handler: MessageHandler,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    ///
    /// TAS-53 Phase 6: Now accepts DecisionPointActor for dynamic workflow step creation
    /// TAS-59 Phase 4: Now accepts BatchProcessingActor for dynamic batch worker creation
    pub fn new(
        task_finalizer: TaskFinalizer,
        context: Arc<SystemContext>,
        decision_point_actor: Arc<DecisionPointActor>,
        batch_processing_actor: Arc<BatchProcessingActor>,
    ) -> Self {
        // Create backoff calculator (V2 config is canonical)
        let backoff_config: BackoffCalculatorConfig = context.tasker_config.clone().into();
        let backoff_calculator =
            BackoffCalculator::new(backoff_config, context.database_pool().clone());

        // Create focused components
        let metadata_processor = MetadataProcessor::new(backoff_calculator);
        let state_transition_handler = StateTransitionHandler::new(context.clone());
        let task_coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        // Create message handler that orchestrates all components
        // TAS-53: includes decision point actor
        // TAS-59: includes batch processing actor
        let message_handler = MessageHandler::new(
            context.clone(),
            metadata_processor,
            state_transition_handler,
            task_coordinator,
            decision_point_actor,
            batch_processing_actor,
        );

        Self { message_handler }
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
    pub async fn handle_step_result_with_metadata(
        &self,
        step_result: StepResultMessage,
    ) -> OrchestrationResult<()> {
        self.message_handler
            .handle_step_result_message(step_result)
            .await
    }

    /// Handle StepExecutionResult message type
    pub async fn handle_step_execution_result(
        &self,
        step_result: &StepExecutionResult,
    ) -> OrchestrationResult<()> {
        self.message_handler
            .handle_step_execution_result(step_result)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_orchestration_result_processor_creation(
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

        // Create DecisionPointActor for testing (TAS-53 Phase 6)
        let decision_point_service = Arc::new(
            crate::orchestration::lifecycle::DecisionPointService::new(context.clone()),
        );
        let decision_point_actor = Arc::new(DecisionPointActor::new(
            context.clone(),
            decision_point_service,
        ));

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

        let processor = OrchestrationResultProcessor::new(
            task_finalizer,
            context,
            decision_point_actor,
            batch_processing_actor,
        );

        // Verify it's created (basic smoke test)
        assert!(std::mem::size_of_val(&processor) > 0);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_orchestration_result_processor_clone(
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

        // Create DecisionPointActor for testing (TAS-53 Phase 6)
        let decision_point_service = Arc::new(
            crate::orchestration::lifecycle::DecisionPointService::new(context.clone()),
        );
        let decision_point_actor = Arc::new(DecisionPointActor::new(
            context.clone(),
            decision_point_service,
        ));

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

        let processor = OrchestrationResultProcessor::new(
            task_finalizer,
            context,
            decision_point_actor,
            batch_processing_actor,
        );

        let cloned = processor.clone();

        // Verify both exist and are independent
        assert!(std::mem::size_of_val(&processor) > 0);
        assert!(std::mem::size_of_val(&cloned) > 0);
        Ok(())
    }
}
