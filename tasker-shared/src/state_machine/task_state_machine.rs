use super::{
    actions::{
        ErrorStateCleanupAction, PublishTransitionEventAction, StateAction,
        UpdateTaskCompletionAction,
    },
    errors::{StateMachineError, StateMachineResult},
    events::TaskEvent,
    persistence::{TaskTransitionPersistence, TransitionPersistence},
    states::TaskState,
};
use crate::events::publisher::EventPublisher;
use crate::models::Task;
use crate::system_context::SystemContext;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Thread-safe task state machine for lifecycle management
#[derive(Clone)]
pub struct TaskStateMachine {
    task_uuid: Uuid,
    task: Task,
    current_state: TaskState,
    pool: PgPool,
    event_publisher: Arc<EventPublisher>,
    persistence: TaskTransitionPersistence,
    processor_uuid: Uuid,
}

crate::debug_with_pgpool!(TaskStateMachine {
    pool: PgPool,
    task_uuid,
    task,
    current_state,
    event_publisher,
    persistence,
    processor_uuid
});

impl TaskStateMachine {
    /// Create a new task state machine instance
    pub fn new(task: Task, system_context: Arc<SystemContext>) -> Self {
        let event_publisher = system_context.event_publisher.clone();
        let persistence = TaskTransitionPersistence;
        let processor_uuid = system_context.processor_uuid();
        let pool = system_context.database_pool().clone();

        Self {
            task_uuid: task.task_uuid,
            task,
            current_state: TaskState::Pending, // Will be resolved on first use
            pool,
            event_publisher,
            persistence,
            processor_uuid,
        }
    }

    /// Create state machine for existing task (TAS-41 approach)
    pub async fn for_task(
        task_uuid: Uuid,
        pool: PgPool,
        processor_uuid: Uuid,
    ) -> StateMachineResult<Self> {
        let persistence = TaskTransitionPersistence;

        let current_state = persistence
            .resolve_current_state(task_uuid, &pool)
            .await?
            .map(|s| s.parse().unwrap_or(TaskState::Pending))
            .unwrap_or(TaskState::Pending);

        // For now, create a minimal task with just the UUID
        // This can be enhanced later when Task::find_by_uuid is available
        let task = Task {
            task_uuid,
            named_task_uuid: Uuid::nil(), // Placeholder
            complete: false,
            requested_at: chrono::Utc::now().naive_utc(),
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "placeholder".to_string(),
            priority: 0,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            correlation_id: Uuid::now_v7(), // TAS-29: Generate placeholder correlation_id
            parent_correlation_id: None,
        };

        let event_publisher = Arc::new(EventPublisher::new());

        Ok(Self {
            task_uuid,
            task,
            current_state,
            pool,
            event_publisher,
            persistence,
            processor_uuid,
        })
    }

    /// Get the current state from database (for validation)
    pub async fn current_state(&self) -> StateMachineResult<TaskState> {
        match self
            .persistence
            .resolve_current_state(self.task_uuid, &self.pool)
            .await?
        {
            Some(state_str) => state_str.parse().map_err(|_| {
                StateMachineError::Internal(format!("Invalid state in database: {state_str}"))
            }),
            None => Ok(TaskState::default()), // No transitions yet, return default state
        }
    }

    /// Attempt to transition the task state
    pub async fn transition(&mut self, event: TaskEvent) -> StateMachineResult<bool> {
        let current_state = self.current_state;
        let target_state = self.determine_target_state_internal(current_state, &event)?;

        // Use the TransitionGuard for validation
        use super::guards::TransitionGuard;
        TransitionGuard::can_transition(current_state, target_state, &event, &self.task).map_err(
            |e| StateMachineError::GuardFailed {
                reason: e.to_string(),
            },
        )?;

        // TAS-54: Processor ownership enforcement removed
        // Processor UUID is still tracked in transitions for audit trail and debugging,
        // but ownership is no longer enforced. This allows tasks to recover after
        // orchestrator crashes without manual intervention.
        //
        // Idempotency is guaranteed by:
        // - State machine guards (current state checks)
        // - Transaction atomicity (all-or-nothing writes)
        // - Unique constraints (identity_hash for tasks)
        // - Atomic claiming (TAS-37 for finalization)

        // Persist the transition with processor UUID for audit trail
        let metadata = Some(serde_json::json!({
            "event": format!("{:?}", event),
        }));

        let success = self
            .persistence
            .transition_with_ownership(
                self.task_uuid,
                current_state,
                target_state,
                self.processor_uuid,
                metadata,
                &self.pool,
            )
            .await
            .map_err(|e| StateMachineError::PersistenceFailed {
                reason: e.to_string(),
            })?;

        if success {
            // Execute actions only if transition succeeded and update cached state
            let event_str = format!("{:?}", event);
            self.execute_actions(current_state, target_state, &event_str)
                .await?;

            // Update cached state after successful transition
            self.current_state = target_state;
        }

        Ok(success)
    }

    /// Determine the target state based on current state and event (for testing)
    /// This method is exposed for integration testing purposes
    pub fn determine_target_state(
        &self,
        current_state: TaskState,
        event: &TaskEvent,
    ) -> StateMachineResult<TaskState> {
        self.determine_target_state_internal(current_state, event)
    }

    /// Determine the target state based on current state and event (internal implementation)
    /// Enhanced with comprehensive TAS-41 12-state transitions (lines 1042-1087)
    fn determine_target_state_internal(
        &self,
        current_state: TaskState,
        event: &TaskEvent,
    ) -> StateMachineResult<TaskState> {
        use TaskEvent::*;
        use TaskState::*;

        let target = match (current_state, event) {
            // From Pending (TAS-41 comprehensive transitions)
            (Pending, Start) => Initializing,

            // From Initializing
            (Initializing, ReadyStepsFound(_)) => EnqueuingSteps,
            (Initializing, NoStepsFound) => TaskState::Complete,
            (Initializing, NoDependenciesReady) => WaitingForDependencies,

            // From EnqueuingSteps
            (EnqueuingSteps, StepsEnqueued(_)) => StepsInProcess,
            (EnqueuingSteps, EnqueueFailed(_)) => Error,

            // From StepsInProcess
            (StepsInProcess, AllStepsCompleted) => EvaluatingResults,
            (StepsInProcess, StepCompleted(_)) => EvaluatingResults,
            (StepsInProcess, StepFailed(_)) => WaitingForRetry,

            // From EvaluatingResults
            (EvaluatingResults, AllStepsSuccessful) => TaskState::Complete,
            (EvaluatingResults, ReadyStepsFound(_)) => EnqueuingSteps,
            (EvaluatingResults, NoDependenciesReady) => WaitingForDependencies,
            (EvaluatingResults, PermanentFailure(_)) => BlockedByFailures,

            // From waiting states
            (WaitingForDependencies, DependenciesReady) => EvaluatingResults,
            (WaitingForRetry, RetryReady) => EnqueuingSteps,
            (BlockedByFailures, GiveUp) => Error,
            (BlockedByFailures, ManualResolution) => ResolvedManually,

            // Cancellation from any non-terminal state
            (state, Cancel) if !state.is_terminal() => Cancelled,

            // Legacy transitions for backward compatibility
            (StepsInProcess, TaskEvent::Complete) => TaskState::Complete,
            (StepsInProcess, Fail(_)) => Error,
            (Error, Reset) => Pending,
            (_, ResolveManually) => ResolvedManually,

            // Invalid combinations
            (from_state, _) => {
                return Err(StateMachineError::InvalidTransition {
                    from: Some(from_state.to_string()),
                    to: format!("{event:?}"),
                })
            }
        };

        Ok(target)
    }

    /// Execute actions after successful transition
    async fn execute_actions(
        &self,
        from_state: TaskState,
        to_state: TaskState,
        event: &str,
    ) -> StateMachineResult<()> {
        let actions: Vec<Box<dyn StateAction<Task> + Send + Sync>> = vec![
            Box::new(PublishTransitionEventAction::new(
                self.event_publisher.clone(),
            )),
            Box::new(UpdateTaskCompletionAction),
            Box::new(ErrorStateCleanupAction),
        ];

        for action in actions {
            action
                .execute(
                    &self.task,
                    Some(from_state.to_string()),
                    to_state.to_string(),
                    event,
                    &self.pool,
                )
                .await?;
        }

        Ok(())
    }

    /// Check if the task is in a terminal state
    pub async fn is_terminal(&self) -> Result<bool, StateMachineError> {
        let state = self.current_state().await?;
        Ok(state.is_terminal())
    }

    /// Check if the task is currently active (being processed)
    pub async fn is_active(&self) -> Result<bool, StateMachineError> {
        let state = self.current_state().await?;
        Ok(state.is_active())
    }

    /// Get task information
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// Get task UUID
    pub fn task_uuid(&self) -> Uuid {
        self.task.task_uuid
    }

    // TAS-54: get_current_processor() removed - no longer needed without ownership enforcement
    // Processor UUID is still tracked in transitions via transition_with_ownership() for audit trail
}
