use super::{
    actions::{
        ErrorStateCleanupAction, PublishTransitionEventAction, ResetAttemptsAction, StateAction,
        TriggerStepDiscoveryAction, UpdateStepResultsAction,
    },
    context::TransitionContext,
    errors::{StateMachineError, StateMachineResult},
    events::StepEvent,
    guards::{StateGuard, StepCanBeRetriedGuard, StepDependenciesMetGuard, StepNotInProgressGuard},
    persistence::{StepTransitionPersistence, TransitionPersistence},
    states::WorkflowStepState,
};
use crate::events::publisher::EventPublisher;
use crate::models::WorkflowStep;
use crate::system_context::SystemContext;
use sqlx::PgPool;
use std::sync::Arc;

/// Thread-safe workflow step state machine for individual step management
#[derive(Clone)]
pub struct StepStateMachine {
    step: WorkflowStep,
    pool: PgPool,
    event_publisher: Arc<EventPublisher>,
    persistence: StepTransitionPersistence,
}

crate::debug_with_pgpool!(StepStateMachine {
    pool: PgPool,
    step,
    event_publisher,
    persistence
});

impl StepStateMachine {
    /// Create a new step state machine instance
    pub fn new(step: WorkflowStep, system_context: Arc<SystemContext>) -> Self {
        let pool = system_context.database_pool().clone();
        let event_publisher = system_context.event_publisher.clone();
        let persistence = StepTransitionPersistence;
        Self {
            step,
            pool,
            event_publisher,
            persistence,
        }
    }

    /// Get the current state of the step
    pub async fn current_state(&self) -> StateMachineResult<WorkflowStepState> {
        match self
            .persistence
            .resolve_current_state(self.step.workflow_step_uuid, &self.pool)
            .await?
        {
            Some(state_str) => state_str.parse().map_err(|_| {
                StateMachineError::Internal(format!("Invalid state in database: {state_str}"))
            }),
            None => Ok(WorkflowStepState::default()), // No transitions yet, return default state
        }
    }

    /// Attempt to transition the step state
    pub async fn transition(&mut self, event: StepEvent) -> StateMachineResult<WorkflowStepState> {
        self.transition_with_context(event, None).await
    }

    /// Attempt to transition the step state with optional attribution context.
    ///
    /// This method enriches transition metadata with attribution data from
    /// `TransitionContext` (worker_uuid, correlation_id). The SQL trigger
    /// `create_step_result_audit` extracts these values to populate the
    /// audit table for SOC2 compliance.
    ///
    /// # Arguments
    ///
    /// * `event` - The step event triggering the transition
    /// * `context` - Optional attribution context for audit enrichment
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tasker_shared::state_machine::{StepStateMachine, StepEvent, TransitionContext};
    /// use uuid::Uuid;
    ///
    /// // Transition with attribution (for worker-side transitions)
    /// let context = TransitionContext::with_worker(worker_uuid, Some(correlation_id));
    /// state_machine.transition_with_context(event, Some(context)).await?;
    ///
    /// // Transition without attribution (backward compatible)
    /// state_machine.transition_with_context(event, None).await?;
    /// ```
    pub async fn transition_with_context(
        &mut self,
        event: StepEvent,
        context: Option<TransitionContext>,
    ) -> StateMachineResult<WorkflowStepState> {
        let current_state = self.current_state().await?;
        let target_state = self.determine_target_state_internal(current_state, &event)?;

        // Check guards
        self.check_guards(current_state, target_state, &event)
            .await?;

        // Persist the transition with enriched metadata
        let event_str = serde_json::to_string(&event)?;

        // Build base metadata
        let base_metadata = serde_json::json!({
            "event": event_str,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        // Enrich metadata with attribution context if provided
        let metadata = match context {
            Some(ctx) if ctx.has_attribution() => ctx.merge_into_metadata(base_metadata),
            _ => base_metadata,
        };

        self.persistence
            .persist_transition(
                &self.step,
                Some(current_state.to_string()),
                target_state.to_string(),
                &event_str,
                Some(metadata),
                &self.pool,
            )
            .await?;

        // Execute actions
        self.execute_actions(current_state, target_state, &event_str)
            .await?;

        Ok(target_state)
    }

    /// Determine the target state based on current state and event (for testing)
    /// This method is exposed for integration testing purposes
    pub fn determine_target_state(
        &self,
        current_state: WorkflowStepState,
        event: &StepEvent,
    ) -> StateMachineResult<WorkflowStepState> {
        self.determine_target_state_internal(current_state, event)
    }

    /// Determine the target state based on current state and event (internal implementation)
    fn determine_target_state_internal(
        &self,
        current_state: WorkflowStepState,
        event: &StepEvent,
    ) -> StateMachineResult<WorkflowStepState> {
        let target = match (current_state, event) {
            (WorkflowStepState::Pending, StepEvent::Enqueue) => WorkflowStepState::Enqueued,

            (WorkflowStepState::Enqueued, StepEvent::Start) => WorkflowStepState::InProgress,
            // Legacy: Support pending → in_progress for backward compatibility during transition
            (WorkflowStepState::Pending, StepEvent::Start) => WorkflowStepState::InProgress,

            // Worker completes step and enqueues for orchestration
            (WorkflowStepState::InProgress, StepEvent::EnqueueForOrchestration(_)) => {
                WorkflowStepState::EnqueuedForOrchestration
            }

            // Worker fails step and enqueues for orchestration error processing
            (WorkflowStepState::InProgress, StepEvent::EnqueueAsErrorForOrchestration(_)) => {
                WorkflowStepState::EnqueuedAsErrorForOrchestration
            }

            // Orchestration processes and completes
            (WorkflowStepState::EnqueuedForOrchestration, StepEvent::Complete(_)) => {
                WorkflowStepState::Complete
            }

            // Orchestration processes and fails
            (WorkflowStepState::EnqueuedForOrchestration, StepEvent::Fail(_)) => {
                WorkflowStepState::Error
            }

            // Orchestration processes error notification
            (WorkflowStepState::EnqueuedAsErrorForOrchestration, StepEvent::Fail(_)) => {
                WorkflowStepState::Error
            }

            // Allow for error recovery if orchestration determines step actually succeeded
            (WorkflowStepState::EnqueuedAsErrorForOrchestration, StepEvent::Complete(_)) => {
                WorkflowStepState::Complete
            }

            // Complete transitions (legacy direct path, should eventually be removed)
            (WorkflowStepState::InProgress, StepEvent::Complete(_)) => WorkflowStepState::Complete,

            // Failure transitions
            (WorkflowStepState::InProgress, StepEvent::Fail(_)) => WorkflowStepState::Error,
            (WorkflowStepState::Pending, StepEvent::Fail(_)) => WorkflowStepState::Error,
            (WorkflowStepState::Enqueued, StepEvent::Fail(_)) => WorkflowStepState::Error,

            // Cancel transitions
            (WorkflowStepState::Pending, StepEvent::Cancel) => WorkflowStepState::Cancelled,
            (WorkflowStepState::Enqueued, StepEvent::Cancel) => WorkflowStepState::Cancelled,
            (WorkflowStepState::InProgress, StepEvent::Cancel) => WorkflowStepState::Cancelled,
            (WorkflowStepState::EnqueuedForOrchestration, StepEvent::Cancel) => {
                WorkflowStepState::Cancelled
            }
            (WorkflowStepState::EnqueuedAsErrorForOrchestration, StepEvent::Cancel) => {
                WorkflowStepState::Cancelled
            }
            (WorkflowStepState::Error, StepEvent::Cancel) => WorkflowStepState::Cancelled,

            // Retry transitions (from error state back to pending)
            (WorkflowStepState::Error, StepEvent::Retry) => WorkflowStepState::Pending,

            // WaitingForRetry transitions
            // Transition to waiting for retry when error is retryable
            (WorkflowStepState::InProgress, StepEvent::WaitForRetry(_)) => {
                WorkflowStepState::WaitingForRetry
            }
            (WorkflowStepState::Enqueued, StepEvent::WaitForRetry(_)) => {
                WorkflowStepState::WaitingForRetry
            }
            (WorkflowStepState::Pending, StepEvent::WaitForRetry(_)) => {
                WorkflowStepState::WaitingForRetry
            }
            (WorkflowStepState::EnqueuedAsErrorForOrchestration, StepEvent::WaitForRetry(_)) => {
                WorkflowStepState::WaitingForRetry
            }

            // Transition from waiting for retry back to pending when ready
            (WorkflowStepState::WaitingForRetry, StepEvent::Retry) => WorkflowStepState::Pending,

            // Cancel from waiting for retry
            (WorkflowStepState::WaitingForRetry, StepEvent::Cancel) => WorkflowStepState::Cancelled,

            // Manual completion with results - transitions to Complete state
            // This allows operators to provide execution results that dependent steps can consume
            (_, StepEvent::CompleteManually(_)) => WorkflowStepState::Complete,

            // Manual resolution - transitions to ResolvedManually state
            (_, StepEvent::ResolveManually) => WorkflowStepState::ResolvedManually,

            // Reset for retry - resets attempt counter and returns to Pending
            // Allows operators to reset failed steps when transient issues are resolved
            (WorkflowStepState::Error, StepEvent::ResetForRetry) => WorkflowStepState::Pending,

            // Invalid transitions
            (from_state, _) => {
                return Err(StateMachineError::InvalidTransition {
                    from: Some(from_state.to_string()),
                    to: format!("{event:?}"),
                })
            }
        };

        Ok(target)
    }

    /// Check guard conditions for the transition
    async fn check_guards(
        &self,
        current_state: WorkflowStepState,
        target_state: WorkflowStepState,
        event: &StepEvent,
    ) -> StateMachineResult<()> {
        match (current_state, target_state, event) {
            (WorkflowStepState::Pending, WorkflowStepState::Enqueued, StepEvent::Enqueue) => {
                let guard = StepDependenciesMetGuard;
                guard.check(&self.step, &self.pool).await?;

                let guard = StepNotInProgressGuard;
                guard.check(&self.step, &self.pool).await?;
            }

            (WorkflowStepState::Enqueued, WorkflowStepState::InProgress, StepEvent::Start) => {
                let guard = StepNotInProgressGuard;
                guard.check(&self.step, &self.pool).await?;
            }

            // Legacy: Check dependencies satisfied before step start (backward compatibility)
            (WorkflowStepState::Pending, WorkflowStepState::InProgress, StepEvent::Start) => {
                let guard = StepDependenciesMetGuard;
                guard.check(&self.step, &self.pool).await?;

                let guard = StepNotInProgressGuard;
                guard.check(&self.step, &self.pool).await?;
            }

            // Check step can be retried (must be in error)
            (WorkflowStepState::Error, WorkflowStepState::Pending, StepEvent::Retry) => {
                let guard = StepCanBeRetriedGuard;
                guard.check(&self.step, &self.pool).await?;
            }

            // No special guards for other transitions
            _ => {}
        }

        Ok(())
    }

    /// Execute actions after successful transition
    async fn execute_actions(
        &self,
        from_state: WorkflowStepState,
        to_state: WorkflowStepState,
        event: &str,
    ) -> StateMachineResult<()> {
        let actions: Vec<Box<dyn StateAction<WorkflowStep> + Send + Sync>> = vec![
            Box::new(PublishTransitionEventAction::new(
                self.event_publisher.clone(),
            )),
            Box::new(UpdateStepResultsAction),
            Box::new(TriggerStepDiscoveryAction::new(
                self.event_publisher.clone(),
            )),
            Box::new(ErrorStateCleanupAction),
            Box::new(ResetAttemptsAction::new()),
        ];

        for action in actions {
            action
                .execute(
                    &self.step,
                    Some(from_state.to_string()),
                    to_state.to_string(),
                    event,
                    &self.pool,
                )
                .await?;
        }

        Ok(())
    }

    /// Check if the step is in a terminal state
    pub async fn is_terminal(&self) -> StateMachineResult<bool> {
        let current_state = self.current_state().await?;
        Ok(current_state.is_terminal())
    }

    /// Check if the step is currently active (being processed)
    pub async fn is_active(&self) -> StateMachineResult<bool> {
        let current_state = self.current_state().await?;
        Ok(current_state.is_active())
    }

    /// Check if the step satisfies dependencies for other steps
    pub async fn satisfies_dependencies(&self) -> StateMachineResult<bool> {
        let current_state = self.current_state().await?;
        Ok(current_state.satisfies_dependencies())
    }

    /// Get step information
    pub fn step(&self) -> &WorkflowStep {
        &self.step
    }

    /// Get step ID
    pub fn step_uuid(&self) -> uuid::Uuid {
        self.step.workflow_step_uuid
    }

    /// Get associated task ID
    pub fn task_uuid(&self) -> uuid::Uuid {
        self.step.task_uuid
    }

    /// Check if step has exceeded retry limit
    pub fn has_exceeded_max_attempts(&self) -> bool {
        if let (Some(attempts), Some(limit)) = (self.step.attempts, self.step.max_attempts) {
            attempts >= limit
        } else {
            false
        }
    }
}
