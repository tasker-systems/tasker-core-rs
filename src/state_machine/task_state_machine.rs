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

/// Thread-safe task state machine for lifecycle management
pub struct TaskStateMachine {
    task: Task,
    pool: PgPool,
    event_publisher: EventPublisher,
    persistence: TaskTransitionPersistence,
}

impl TaskStateMachine {
    /// Create a new task state machine instance
    pub fn new(task: Task, pool: PgPool, event_publisher: EventPublisher) -> Self {
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
            .resolve_current_state(self.task.task_id, &self.pool)
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
        let target_state = self.determine_target_state(current_state, &event)?;

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

    /// Determine the target state based on current state and event
    fn determine_target_state(
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

    /// Get task ID
    pub fn task_id(&self) -> i64 {
        self.task.task_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::publisher::EventPublisher;

    #[test]
    #[ignore = "State machine tests deferred as architectural dependency"]
    fn test_state_transitions() {
        // Test valid transitions
        let sm = create_test_state_machine();

        assert_eq!(
            sm.determine_target_state(TaskState::Pending, &TaskEvent::Start)
                .unwrap(),
            TaskState::InProgress
        );

        assert_eq!(
            sm.determine_target_state(TaskState::InProgress, &TaskEvent::Complete)
                .unwrap(),
            TaskState::Complete
        );

        assert_eq!(
            sm.determine_target_state(TaskState::InProgress, &TaskEvent::Fail("error".to_string()))
                .unwrap(),
            TaskState::Error
        );
    }

    #[test]
    #[ignore = "State machine tests deferred as architectural dependency"]
    fn test_invalid_transitions() {
        let sm = create_test_state_machine();

        // Cannot start from complete state
        assert!(sm
            .determine_target_state(TaskState::Complete, &TaskEvent::Start)
            .is_err());

        // Cannot complete from pending state
        assert!(sm
            .determine_target_state(TaskState::Pending, &TaskEvent::Complete)
            .is_err());
    }

    fn create_test_state_machine() -> TaskStateMachine {
        use crate::models::Task;
        use chrono::Utc;

        let task = Task {
            task_id: 1,
            named_task_id: 1,
            complete: false,
            requested_at: Utc::now().naive_utc(),
            initiator: Some("test".to_string()),
            source_system: Some("test_system".to_string()),
            reason: Some("test reason".to_string()),
            bypass_steps: None,
            tags: None,
            context: Some(serde_json::json!({})),
            identity_hash: "test_hash".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        };

        // This would normally require a real pool and event publisher
        // For testing, we'd use mocks or test doubles
        TaskStateMachine {
            task,
            pool: create_test_pool(), // Would be implemented in test helpers
            event_publisher: EventPublisher::default(),
            persistence: TaskTransitionPersistence,
        }
    }

    // Mock helper - would be implemented with actual test database
    fn create_test_pool() -> PgPool {
        todo!("Implement test database pool")
    }
}
