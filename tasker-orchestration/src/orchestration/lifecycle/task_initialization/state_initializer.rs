//! State Machine Initialization
//!
//! Handles initialization of task and workflow step state machines.
//! This includes creating initial database transitions and transitioning
//! the task from Pending to Initializing state.

use crate::orchestration::state_manager::StateManager;
use serde_json::json;
use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::models::core::task_transition::NewTaskTransition;
use tasker_shared::models::core::workflow_step_transition::NewWorkflowStepTransition;
use tasker_shared::models::{TaskTransition, WorkflowStepTransition};
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::state_machine::{TaskEvent, TaskStateMachine};
use tasker_shared::system_context::SystemContext;
use tracing::{error, info, warn};

use super::TaskInitializationError;

/// Initializes state machines for tasks and workflow steps
pub struct StateInitializer {
    context: Arc<SystemContext>,
    state_manager: StateManager,
}

impl StateInitializer {
    pub fn new(context: Arc<SystemContext>) -> Self {
        let state_manager = StateManager::new(context.clone());
        Self {
            context,
            state_manager,
        }
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

        // Initialize step state machines WITHOUT evaluating state transitions
        // We don't want to transition steps to InProgress during initialization
        // as this sets in_process=true, making them ineligible for execution
        for &workflow_step_uuid in step_mapping.values() {
            // Simply verify the state machine exists, don't evaluate/transition
            match self
                .state_manager
                .get_or_create_step_state_machine(workflow_step_uuid)
                .await
            {
                Ok(state_machine) => match state_machine.current_state().await {
                    Ok(_current_state) => {}
                    Err(e) => {
                        warn!(
                            step_uuid = %workflow_step_uuid,
                            error = %e,
                            "Failed to get current state from step state machine"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        step_uuid = %workflow_step_uuid,
                        error = %e,
                        "Failed to initialize step state machine, basic initialization completed"
                    );
                    // Don't fail the entire initialization for StateManager issues
                }
            }
        }

        Ok(())
    }
}
