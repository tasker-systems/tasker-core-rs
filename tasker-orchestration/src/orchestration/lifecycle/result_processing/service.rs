//! Orchestration Result Processor Service
//!
//! TAS-32: Orchestration Coordination Logic (No State Management)
//!
//! Main service for orchestrating result processing with focused components.

use std::sync::Arc;

use crate::orchestration::lifecycle::task_finalization::TaskFinalizer;
use crate::orchestration::{BackoffCalculator, BackoffCalculatorConfig};
use tasker_shared::messaging::{StepExecutionResult, StepResultMessage};
use tasker_shared::system_context::SystemContext;
use tasker_shared::errors::OrchestrationResult;

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
#[derive(Clone)]
pub struct OrchestrationResultProcessor {
    message_handler: MessageHandler,
}

impl OrchestrationResultProcessor {
    /// Create a new orchestration result processor
    pub fn new(task_finalizer: TaskFinalizer, context: Arc<SystemContext>) -> Self {
        // Create backoff calculator
        let backoff_config: BackoffCalculatorConfig = context.tasker_config.clone().into();
        let backoff_calculator =
            BackoffCalculator::new(backoff_config, context.database_pool().clone());

        // Create focused components
        let metadata_processor = MetadataProcessor::new(backoff_calculator);
        let state_transition_handler = StateTransitionHandler::new(context.clone());
        let task_coordinator = TaskCoordinator::new(context.clone(), task_finalizer);

        // Create message handler that orchestrates all components
        let message_handler = MessageHandler::new(
            context.clone(),
            metadata_processor,
            state_transition_handler,
            task_coordinator,
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
