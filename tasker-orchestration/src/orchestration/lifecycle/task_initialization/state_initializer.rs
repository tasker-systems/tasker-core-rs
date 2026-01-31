//! State Machine Initialization
//!
//! Handles initialization of task and workflow step state machines.
//! This includes creating initial database transitions and transitioning
//! the task from Pending to Initializing state.

use serde_json::json;
use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::models::core::task_transition::NewTaskTransition;
use tasker_shared::models::core::workflow_step_transition::NewWorkflowStepTransition;
use tasker_shared::models::{TaskTransition, WorkflowStep, WorkflowStepTransition};
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::{TaskEvent, TaskStateMachine};
use tasker_shared::system_context::SystemContext;
use tracing::{error, info, warn};

use super::TaskInitializationError;

/// Initializes state machines for tasks and workflow steps
#[derive(Debug)]
pub struct StateInitializer {
    context: Arc<SystemContext>,
}

impl StateInitializer {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Create initial state transitions in database using consistent transaction methods
    pub async fn create_initial_state_transitions_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        // Create initial task transition using transaction method
        let new_task_transition = NewTaskTransition {
            task_uuid,
            to_state: "pending".to_string(),
            from_state: None,
            processor_uuid: Some(self.context.processor_uuid()),
            metadata: Some(json!({"initial_state": "pending", "from_service": "task_initializer"})),
        };

        TaskTransition::create_with_transaction(tx, new_task_transition)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create initial task transition: {e}"
                ))
            })?;

        // Create initial step transitions using transaction method
        for &workflow_step_uuid in step_mapping.values() {
            let new_step_transition = NewWorkflowStepTransition {
                workflow_step_uuid,
                to_state: "pending".to_string(),
                from_state: None,
                metadata: Some(
                    json!({"initial_state": "pending", "from_service": "task_initializer"}),
                ),
            };

            WorkflowStepTransition::create_with_transaction(tx, new_step_transition)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create initial step transition for step {workflow_step_uuid}: {e}"
                    ))
                })?;
        }

        Ok(())
    }

    /// Initialize StateManager-based state machines after transaction commit
    pub async fn initialize_state_machines_post_transaction(
        &self,
        task_uuid: Uuid,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        // Ensure the task transitions from Pending to Initializing state
        // This is critical for the task lifecycle to work properly
        let mut task_state_machine = TaskStateMachine::for_task(
            task_uuid,
            self.context.database_pool().clone(),
            self.context.processor_uuid(),
        )
        .await
        .map_err(|e| {
            TaskInitializationError::StateMachine(format!(
                "Failed to create task state machine: {e}"
            ))
        })?;

        let current_state = task_state_machine.current_state().await.map_err(|e| {
            TaskInitializationError::StateMachine(format!("Failed to get current state: {e}"))
        })?;

        info!(
            task_uuid = %task_uuid,
            current_state = %current_state,
            "Task state machine created, transitioning from Pending to Initializing"
        );

        // Transition from Pending to Initializing if needed
        if current_state == TaskState::Pending {
            match task_state_machine.transition(TaskEvent::Start).await {
                Ok(success) => {
                    if success {
                        info!(
                            task_uuid = %task_uuid,
                            "Successfully transitioned task from Pending to Initializing"
                        );
                    } else {
                        warn!(
                            task_uuid = %task_uuid,
                            "Task state transition returned false - task may already be in correct state"
                        );
                    }
                }
                Err(e) => {
                    error!(
                        task_uuid = %task_uuid,
                        error = %e,
                        "Failed to transition task from Pending to Initializing"
                    );
                    return Err(TaskInitializationError::StateMachine(format!(
                        "Failed to transition task from Pending to Initializing: {e}"
                    )));
                }
            }
        } else {
            info!(
                task_uuid = %task_uuid,
                current_state = %current_state,
                "Task already in non-Pending state, no transition needed"
            );
        }

        // Verify step state machines can be created WITHOUT evaluating state transitions.
        // We don't want to transition steps to InProgress during initialization
        // as this sets in_process=true, making them ineligible for execution.
        for &workflow_step_uuid in step_mapping.values() {
            match WorkflowStep::find_by_id(self.context.database_pool(), workflow_step_uuid).await {
                Ok(Some(workflow_step)) => {
                    let state_machine = StepStateMachine::new(workflow_step, self.context.clone());
                    if let Err(e) = state_machine.current_state().await {
                        warn!(
                            step_uuid = %workflow_step_uuid,
                            error = %e,
                            "Failed to get current state from step state machine"
                        );
                    }
                }
                Ok(None) => {
                    warn!(
                        step_uuid = %workflow_step_uuid,
                        "Step not found in database during state machine initialization"
                    );
                }
                Err(e) => {
                    warn!(
                        step_uuid = %workflow_step_uuid,
                        error = %e,
                        "Failed to load step for state machine initialization"
                    );
                    // Don't fail the entire initialization for step loading issues
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::models::Task;

    #[test]
    fn test_step_mapping_structure_for_transitions() {
        // Test that step mapping can be used for transition creation
        let mut step_mapping = HashMap::new();
        let step1_uuid = Uuid::new_v4();
        let step2_uuid = Uuid::new_v4();

        step_mapping.insert("step1".to_string(), step1_uuid);
        step_mapping.insert("step2".to_string(), step2_uuid);

        // Verify we can iterate over values (as needed for transition creation)
        let uuids: Vec<Uuid> = step_mapping.values().copied().collect();
        assert_eq!(uuids.len(), 2);
        assert!(uuids.contains(&step1_uuid));
        assert!(uuids.contains(&step2_uuid));
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_initial_state_transitions_creates_task_transition(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let initializer = StateInitializer::new(context.clone());

        // Create namespace and named task first (required for foreign key)
        let namespace = tasker_shared::models::TaskNamespace::create(
            &pool,
            tasker_shared::models::core::task_namespace::NewTaskNamespace {
                name: "test_ns".to_string(),
                description: Some("Test namespace".to_string()),
            },
        )
        .await?;

        let named_task =
            tasker_shared::models::NamedTask::find_or_create_by_name_version_namespace(
                &pool,
                "test_task",
                "1.0.0",
                namespace.task_namespace_uuid,
            )
            .await?;

        // Create a test task
        let task_request = tasker_shared::models::task_request::TaskRequest::new(
            "test_task".to_string(),
            "test_ns".to_string(),
        );
        let mut task = Task::from_task_request(task_request);
        task.named_task_uuid = named_task.named_task_uuid;
        let task = Task::create(&pool, task).await?;
        let task_uuid = task.task_uuid;

        // Create step mapping
        let step_mapping = HashMap::new();

        // Begin transaction
        let mut tx = pool.begin().await?;

        // Create initial transitions
        initializer
            .create_initial_state_transitions_in_tx(&mut tx, task_uuid, &step_mapping)
            .await?;

        // Commit transaction
        tx.commit().await?;

        // Verify task transition was created
        let transitions = sqlx::query!(
            "SELECT to_state, from_state FROM tasker.task_transitions WHERE task_uuid = $1",
            task_uuid
        )
        .fetch_all(&pool)
        .await?;

        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].to_state, "pending");
        assert!(transitions[0].from_state.is_none());

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_initial_state_transitions_creates_step_transitions(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let initializer = StateInitializer::new(context.clone());

        // Create namespace and named task first (required for foreign key)
        let namespace = tasker_shared::models::TaskNamespace::create(
            &pool,
            tasker_shared::models::core::task_namespace::NewTaskNamespace {
                name: "test_ns".to_string(),
                description: Some("Test namespace".to_string()),
            },
        )
        .await?;

        let named_task =
            tasker_shared::models::NamedTask::find_or_create_by_name_version_namespace(
                &pool,
                "test_task",
                "1.0.0",
                namespace.task_namespace_uuid,
            )
            .await?;

        // Create a test task with steps
        let task_request = tasker_shared::models::task_request::TaskRequest::new(
            "test_task".to_string(),
            "test_ns".to_string(),
        );
        let mut task = Task::from_task_request(task_request);
        task.named_task_uuid = named_task.named_task_uuid;
        let task = Task::create(&pool, task).await?;
        let task_uuid = task.task_uuid;

        // Create named steps and workflow steps
        let mut step_mapping = HashMap::new();

        let named_step =
            tasker_shared::models::NamedStep::find_or_create_by_name(&pool, "test_step").await?;

        // Begin transaction
        let mut tx = pool.begin().await?;

        let new_workflow_step = tasker_shared::models::core::workflow_step::NewWorkflowStep {
            task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: Some(true),
            max_attempts: Some(3),
            inputs: None,
        };

        let workflow_step = tasker_shared::models::WorkflowStep::create_with_transaction(
            &mut tx,
            new_workflow_step,
        )
        .await?;

        step_mapping.insert("test_step".to_string(), workflow_step.workflow_step_uuid);

        // Create initial transitions
        initializer
            .create_initial_state_transitions_in_tx(&mut tx, task_uuid, &step_mapping)
            .await?;

        // Commit transaction
        tx.commit().await?;

        // Verify step transition was created
        let transitions = sqlx::query!(
            "SELECT to_state, from_state FROM tasker.workflow_step_transitions WHERE workflow_step_uuid = $1",
            workflow_step.workflow_step_uuid
        )
        .fetch_all(&pool)
        .await?;

        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].to_state, "pending");
        assert!(transitions[0].from_state.is_none());

        Ok(())
    }
}
