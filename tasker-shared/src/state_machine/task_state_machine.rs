use super::{
    actions::{
        ErrorStateCleanupAction, PublishTransitionEventAction, StateAction,
        UpdateTaskCompletionAction,
    },
    errors::{StateMachineError, StateMachineResult},
    events::TaskEvent,
    guards::{AllStepsCompleteGuard, StateGuard, TaskCanBeResetGuard, TaskNotInProgressGuard},
    persistence::{TaskTransitionPersistence, TransitionPersistence},
    states::TaskState,
};
use crate::events::publisher::EventPublisher;
use crate::models::Task;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Thread-safe task state machine for lifecycle management
#[derive(Clone)]
pub struct TaskStateMachine {
    task: Task,
    pool: PgPool,
    event_publisher: Arc<EventPublisher>,
    persistence: TaskTransitionPersistence,
}

impl TaskStateMachine {
    /// Create a new task state machine instance
    pub fn new(task: Task, pool: PgPool, event_publisher: Option<Arc<EventPublisher>>) -> Self {
        let event_publisher = match event_publisher {
            Some(publisher) => publisher,
            None => Arc::new(EventPublisher::new()),
        };
        Self {
            task,
            pool,
            event_publisher,
            persistence: TaskTransitionPersistence,
        }
    }

    /// Get the current state of the task
    pub async fn current_state(&self) -> StateMachineResult<TaskState> {
        match self
            .persistence
            .resolve_current_state(self.task.task_uuid, &self.pool)
            .await?
        {
            Some(state_str) => state_str.parse().map_err(|_| {
                StateMachineError::Internal(format!("Invalid state in database: {state_str}"))
            }),
            None => Ok(TaskState::default()), // No transitions yet, return default state
        }
    }

    /// Attempt to transition the task state
    pub async fn transition(&mut self, event: TaskEvent) -> StateMachineResult<TaskState> {
        let current_state = self.current_state().await?;
        let target_state = self.determine_target_state_internal(current_state, &event)?;

        // Check guards
        self.check_guards(current_state, target_state, &event)
            .await?;

        // Persist the transition
        let event_str = serde_json::to_string(&event)?;
        self.persistence
            .persist_transition(
                &self.task,
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
        current_state: TaskState,
        event: &TaskEvent,
    ) -> StateMachineResult<TaskState> {
        self.determine_target_state_internal(current_state, event)
    }

    /// Determine the target state based on current state and event (internal implementation)
    fn determine_target_state_internal(
        &self,
        current_state: TaskState,
        event: &TaskEvent,
    ) -> StateMachineResult<TaskState> {
        let target = match (current_state, event) {
            // Start transitions
            (TaskState::Pending, TaskEvent::Start) => TaskState::InProgress,
            (TaskState::Error, TaskEvent::Reset) => TaskState::Pending,

            // Complete transitions
            (TaskState::InProgress, TaskEvent::Complete) => TaskState::Complete,

            // Failure transitions
            (TaskState::InProgress, TaskEvent::Fail(_)) => TaskState::Error,
            (TaskState::Pending, TaskEvent::Fail(_)) => TaskState::Error,

            // Cancel transitions
            (TaskState::Pending, TaskEvent::Cancel) => TaskState::Cancelled,
            (TaskState::InProgress, TaskEvent::Cancel) => TaskState::Cancelled,
            (TaskState::Error, TaskEvent::Cancel) => TaskState::Cancelled,

            // Manual resolution
            (_, TaskEvent::ResolveManually) => TaskState::ResolvedManually,

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
        current_state: TaskState,
        target_state: TaskState,
        event: &TaskEvent,
    ) -> StateMachineResult<()> {
        match (current_state, target_state, event) {
            // Check all steps complete before task completion
            (TaskState::InProgress, TaskState::Complete, TaskEvent::Complete) => {
                let guard = AllStepsCompleteGuard;
                guard.check(&self.task, &self.pool).await?;
            }

            // Check task not already in progress when starting
            (TaskState::Pending, TaskState::InProgress, TaskEvent::Start) => {
                let guard = TaskNotInProgressGuard;
                guard.check(&self.task, &self.pool).await?;
            }

            // Check task can be reset (must be in error)
            (TaskState::Error, TaskState::Pending, TaskEvent::Reset) => {
                let guard = TaskCanBeResetGuard;
                guard.check(&self.task, &self.pool).await?;
            }

            // No special guards for other transitions
            _ => {}
        }

        Ok(())
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
    pub async fn is_terminal(&self) -> StateMachineResult<bool> {
        let current_state = self.current_state().await?;
        Ok(current_state.is_terminal())
    }

    /// Check if the task is currently active (being processed)
    pub async fn is_active(&self) -> StateMachineResult<bool> {
        let current_state = self.current_state().await?;
        Ok(current_state.is_active())
    }

    /// Get task information
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// Get task UUID
    pub fn task_uuid(&self) -> Uuid {
        self.task.task_uuid
    }
}
