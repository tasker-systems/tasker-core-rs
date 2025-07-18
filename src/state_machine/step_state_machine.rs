use super::{
    actions::{
        ErrorStateCleanupAction, PublishTransitionEventAction, StateAction,
        TriggerStepDiscoveryAction, UpdateStepResultsAction,
    },
    errors::{StateMachineError, StateMachineResult},
    events::StepEvent,
    guards::{StateGuard, StepCanBeRetriedGuard, StepDependenciesMetGuard, StepNotInProgressGuard},
    persistence::{StepTransitionPersistence, TransitionPersistence},
    states::WorkflowStepState,
};
use crate::events::publisher::EventPublisher;
use crate::models::WorkflowStep;
use sqlx::PgPool;

/// Thread-safe workflow step state machine for individual step management
#[derive(Clone)]
pub struct StepStateMachine {
    step: WorkflowStep,
    pool: PgPool,
    event_publisher: EventPublisher,
    persistence: StepTransitionPersistence,
}

impl StepStateMachine {
    /// Create a new step state machine instance
    pub fn new(step: WorkflowStep, pool: PgPool, event_publisher: EventPublisher) -> Self {
        Self {
            step,
            pool,
            event_publisher,
            persistence: StepTransitionPersistence,
        }
    }

    /// Get the current state of the step
    pub async fn current_state(&self) -> StateMachineResult<WorkflowStepState> {
        match self
            .persistence
            .resolve_current_state(self.step.workflow_step_id, &self.pool)
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
        let current_state = self.current_state().await?;
        let target_state = self.determine_target_state_internal(current_state, &event)?;

        // Check guards
        self.check_guards(current_state, target_state, &event)
            .await?;

        // Persist the transition
        let event_str = serde_json::to_string(&event)?;
        self.persistence
            .persist_transition(
                &self.step,
                Some(current_state.to_string()),
                target_state.to_string(),
                &event_str,
                None,
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
            // Start transitions
            (WorkflowStepState::Pending, StepEvent::Start) => WorkflowStepState::InProgress,

            // Complete transitions
            (WorkflowStepState::InProgress, StepEvent::Complete(_)) => WorkflowStepState::Complete,

            // Failure transitions
            (WorkflowStepState::InProgress, StepEvent::Fail(_)) => WorkflowStepState::Error,
            (WorkflowStepState::Pending, StepEvent::Fail(_)) => WorkflowStepState::Error,

            // Cancel transitions
            (WorkflowStepState::Pending, StepEvent::Cancel) => WorkflowStepState::Cancelled,
            (WorkflowStepState::InProgress, StepEvent::Cancel) => WorkflowStepState::Cancelled,
            (WorkflowStepState::Error, StepEvent::Cancel) => WorkflowStepState::Cancelled,

            // Retry transitions (from error state back to pending)
            (WorkflowStepState::Error, StepEvent::Retry) => WorkflowStepState::Pending,

            // Manual resolution
            (_, StepEvent::ResolveManually) => WorkflowStepState::ResolvedManually,

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
            // Check dependencies satisfied before step start
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
    pub fn step_id(&self) -> i64 {
        self.step.workflow_step_id
    }

    /// Get associated task ID
    pub fn task_id(&self) -> i64 {
        self.step.task_id
    }

    /// Update retry count after failure
    pub async fn increment_retry_count(&mut self) -> StateMachineResult<()> {
        // This would typically update the step's attempts in the database
        // For now, we'll just log it since we simplified the database operations
        tracing::info!(
            step_id = self.step.workflow_step_id,
            current_attempts = self.step.attempts,
            "Incrementing retry count for step"
        );
        Ok(())
    }

    /// Check if step has exceeded retry limit
    pub fn has_exceeded_retry_limit(&self) -> bool {
        if let (Some(attempts), Some(limit)) = (self.step.attempts, self.step.retry_limit) {
            attempts >= limit
        } else {
            false
        }
    }
}
