//! # State Manager
//!
//! ## Architecture: SQL Functions + State Machine Coordination
//!
//! The StateManager provides high-level state management operations that coordinate
//! between SQL function intelligence and state machine execution. This follows the
//! delegation pattern where SQL functions provide complex state evaluation logic
//! and the StateManager orchestrates the results with state machine transitions.
//!
//! ## Key Features
//!
//! - **SQL-driven evaluation**: Uses existing SQL functions for complex state logic
//! - **State machine coordination**: Integrates with TaskStateMachine and StepStateMachine
//! - **Bulk operations**: Efficient batch state transitions for workflow coordination
//! - **Event integration**: Publishes state change events through EventPublisher
//! - **Error recovery**: Handles state inconsistencies and recovery scenarios
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::StateManager;
//! use tasker_shared::system_context::SystemContext;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create orchestration system context (TAS-50 Phase 2: context-specific loading)
//! let context = Arc::new(SystemContext::new_for_orchestration().await?);
//! let state_manager = StateManager::new(context);
//!
//! // StateManager provides high-level state management operations
//! // Verify creation succeeded (we can't test database operations without a real database)
//! let _state_manager = state_manager;
//! # Ok(())
//! # }
//!
//! // For complete database integration examples, see tests/models/state_manager_integration.rs
//! ```

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
use tasker_shared::events::EventPublisher;
use tasker_shared::models::WorkflowStep;
use tasker_shared::state_machine::events::{StepEvent, TaskEvent};
use tasker_shared::state_machine::states::{TaskState, WorkflowStepState};
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::task_state_machine::TaskStateMachine;
use tasker_shared::system_context::SystemContext;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

/// State transition request for bulk operations
#[derive(Debug, Clone)]
pub struct StateTransitionRequest {
    pub entity_uuid: Uuid,
    pub entity_type: StateEntityType,
    pub target_state: String,
    pub event: StateTransitionEvent,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Type of entity for state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum StateEntityType {
    Task,
    Step,
}

/// Events that trigger state transitions
#[derive(Debug, Clone)]
pub enum StateTransitionEvent {
    TaskEvent(TaskEvent),
    StepEvent(StepEvent),
}

/// Result of state evaluation operations
#[derive(Debug, Clone)]
pub struct StateEvaluationResult {
    pub entity_uuid: Uuid,
    pub entity_type: StateEntityType,
    pub current_state: String,
    pub recommended_state: Option<String>,
    pub transition_required: bool,
    pub reason: Option<String>,
}

/// State manager for coordinating SQL functions with state machines
#[derive(Clone, Debug)]
pub struct StateManager {
    system_context: Arc<SystemContext>,
    sql_executor: Arc<SqlFunctionExecutor>,
    event_publisher: Arc<EventPublisher>,
    /// Cache of active state machines to avoid recreation
    task_state_machines: Arc<Mutex<HashMap<Uuid, TaskStateMachine>>>,
    step_state_machines: Arc<Mutex<HashMap<Uuid, StepStateMachine>>>,
}

impl StateManager {
    /// Create new state manager instance
    pub fn new(system_context: Arc<SystemContext>) -> Self {
        let sql_executor = Arc::new(SqlFunctionExecutor::new(
            system_context.database_pool().clone(),
        ));
        let event_publisher = system_context.event_publisher.clone();

        Self {
            system_context,
            sql_executor,
            event_publisher,
            task_state_machines: Arc::new(Mutex::new(HashMap::new())),
            step_state_machines: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get reference to the database pool
    pub fn database_pool(&self) -> &sqlx::PgPool {
        self.system_context.database_pool()
    }

    async fn get_task_uuid_for_step_uuid(&self, step_uuid: Uuid) -> OrchestrationResult<Uuid> {
        let step = WorkflowStep::find_by_id(self.database_pool(), step_uuid)
            .await
            .map_err(|e| OrchestrationError::SqlFunctionError {
                function_name: "find_by_id".to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| OrchestrationError::SqlFunctionError {
                function_name: "find_by_id".to_string(),
                reason: format!("WorkflowStep with UUID {step_uuid} not found"),
            })?;
        Ok(step.task_uuid)
    }

    /// Evaluate task state based on SQL function analysis and trigger transitions if needed
    #[instrument(skip(self), fields(task_uuid = task_uuid.to_string()))]
    pub async fn evaluate_task_state(
        &self,
        task_uuid: Uuid,
    ) -> OrchestrationResult<StateEvaluationResult> {
        debug!(task_uuid = task_uuid.to_string(), "Evaluating task state");

        // 1. Get task execution context from SQL function
        let execution_context = self
            .sql_executor
            .get_task_execution_context(task_uuid)
            .await
            .map_err(|e| OrchestrationError::SqlFunctionError {
                function_name: "get_task_execution_context".to_string(),
                reason: e.to_string(),
            })?;

        let context = execution_context.ok_or_else(|| OrchestrationError::InvalidTaskState {
            task_uuid,
            current_state: "unknown".to_string(),
            expected_states: vec!["pending".to_string(), "in_progress".to_string()],
        })?;

        // 2. Get current state from state machine
        let task_state_machine = self.get_or_create_task_state_machine(task_uuid).await?;
        let current_state = task_state_machine.current_state().await?;

        // 3. Determine recommended state based on step completion
        let (recommended_state, reason) = self
            .calculate_recommended_task_state(&context, &current_state)
            .await?;

        // 4. Check if transition is needed
        let transition_required = matches!(
            (&current_state, &recommended_state),
            (TaskState::Pending, Some(TaskState::Initializing))
                | (TaskState::EvaluatingResults, Some(TaskState::Complete))
                | (TaskState::EvaluatingResults, Some(TaskState::Error))
        );

        let result = StateEvaluationResult {
            entity_uuid: task_uuid,
            entity_type: StateEntityType::Task,
            current_state: current_state.to_string(),
            recommended_state: recommended_state.map(|s| s.to_string()),
            transition_required,
            reason,
        };

        // 5. Perform transition if needed
        if transition_required {
            if let Some(target_state) = &recommended_state {
                self.transition_task_state(task_uuid, *target_state).await?;
                info!(
                    task_uuid = task_uuid.to_string(),
                    from_state = %current_state,
                    to_state = %target_state,
                    "Task state transition completed"
                );
            }
        }

        Ok(result)
    }

    /// Evaluate step state and trigger transitions if needed
    #[instrument(skip(self), fields(step_uuid = step_uuid.to_string()))]
    pub async fn evaluate_step_state(
        &self,
        step_uuid: Uuid,
    ) -> OrchestrationResult<StateEvaluationResult> {
        debug!(step_uuid = step_uuid.to_string(), "Evaluating step state");

        // 1. First get the task_uuid for this step
        let task_uuid_result = sqlx::query!(
            "SELECT task_uuid FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .fetch_optional(self.database_pool())
        .await
        .map_err(|e| OrchestrationError::DatabaseError {
            operation: "get_task_uuid_for_step".to_string(),
            reason: e.to_string(),
        })?;

        let task_uuid = task_uuid_result
            .ok_or_else(|| OrchestrationError::InvalidStepState {
                step_uuid,
                current_state: "unknown".to_string(),
                expected_states: vec!["pending".to_string(), "in_progress".to_string()],
            })?
            .task_uuid;

        // 2. Get step readiness status from SQL function with correct task_uuid
        let readiness_statuses = self
            .sql_executor
            .get_step_readiness_status(task_uuid, Some(vec![step_uuid]))
            .await
            .map_err(|e| OrchestrationError::SqlFunctionError {
                function_name: "get_step_readiness_status".to_string(),
                reason: e.to_string(),
            })?;

        let status = readiness_statuses
            .into_iter()
            .find(|s| s.workflow_step_uuid == step_uuid)
            .ok_or_else(|| OrchestrationError::InvalidStepState {
                step_uuid,
                current_state: "unknown".to_string(),
                expected_states: vec!["pending".to_string(), "in_progress".to_string()],
            })?;

        // 2. Get current state from state machine
        let step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;
        let current_state = step_state_machine.current_state().await?;

        // 3. Determine recommended state based on SQL analysis
        let (recommended_state, reason) =
            self.calculate_recommended_step_state(&status, &current_state);

        // 4. Check if transition is needed
        let transition_required = matches!(
            (&current_state, &recommended_state),
            (
                WorkflowStepState::Pending,
                Some(WorkflowStepState::InProgress)
            ) | (
                WorkflowStepState::InProgress,
                Some(WorkflowStepState::Complete)
            ) | (
                WorkflowStepState::InProgress,
                Some(WorkflowStepState::Error)
            )
        );

        let result = StateEvaluationResult {
            entity_uuid: step_uuid,
            entity_type: StateEntityType::Step,
            current_state: current_state.to_string(),
            recommended_state: recommended_state.map(|s| s.to_string()),
            transition_required,
            reason,
        };

        // 5. Perform transition if needed
        if transition_required {
            if let Some(target_state) = &recommended_state {
                self.transition_step_state(step_uuid, *target_state).await?;
                info!(
                    step_uuid = step_uuid.to_string(),
                    from_state = %current_state,
                    to_state = %target_state,
                    "Step state transition completed"
                );
            }
        }

        Ok(result)
    }

    /// Transition multiple steps to completed state efficiently
    #[instrument(skip(self), fields(step_count = step_uuids.len()))]
    pub async fn transition_steps_to_completed(
        &self,
        step_uuids: Vec<Uuid>,
    ) -> OrchestrationResult<Vec<StateEvaluationResult>> {
        debug!(
            step_count = step_uuids.len(),
            "Transitioning steps to completed"
        );

        let mut results = Vec::new();

        for step_uuid in step_uuids {
            // Get step state machine
            let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;
            let current_state = step_state_machine.current_state().await?;
            let task_uuid = self.get_task_uuid_for_step_uuid(step_uuid).await?;
            // Only transition if not already complete
            if current_state != WorkflowStepState::Complete {
                match step_state_machine
                    .transition(StepEvent::Complete(None))
                    .await
                {
                    Ok(_) => {
                        let result = StateEvaluationResult {
                            entity_uuid: step_uuid,
                            entity_type: StateEntityType::Step,
                            current_state: current_state.to_string(),
                            recommended_state: Some("complete".to_string()),
                            transition_required: true,
                            reason: Some("Bulk completion operation".to_string()),
                        };
                        results.push(result);

                        // Publish event
                        self.event_publisher
                            .publish_step_execution_completed(
                                step_uuid,
                                task_uuid,
                                tasker_shared::events::StepResult::Success,
                            )
                            .await?;
                    }
                    Err(e) => {
                        warn!(
                            step_uuid = step_uuid.to_string(),
                            error = %e,
                            "Failed to transition step to completed"
                        );
                        return Err(OrchestrationError::StateTransitionFailed {
                            entity_type: "WorkflowStep".to_string(),
                            entity_uuid: step_uuid,
                            reason: e.to_string(),
                        });
                    }
                }
            } else {
                let result = StateEvaluationResult {
                    entity_uuid: step_uuid,
                    entity_type: StateEntityType::Step,
                    current_state: current_state.to_string(),
                    recommended_state: None,
                    transition_required: false,
                    reason: Some("Already completed".to_string()),
                };
                results.push(result);
            }
        }

        info!(
            completed_steps = results.len(),
            "Bulk step completion operation completed"
        );

        Ok(results)
    }

    /// Process bulk state transition requests
    #[instrument(skip(self), fields(request_count = requests.len()))]
    pub async fn process_bulk_transitions(
        &self,
        requests: Vec<StateTransitionRequest>,
    ) -> OrchestrationResult<Vec<StateEvaluationResult>> {
        debug!(
            request_count = requests.len(),
            "Processing bulk state transitions"
        );

        let mut results = Vec::new();

        for request in requests {
            let result = match request.entity_type {
                StateEntityType::Task => self.process_task_transition_request(request).await?,
                StateEntityType::Step => self.process_step_transition_request(request).await?,
            };
            results.push(result);
        }

        info!(
            processed_transitions = results.len(),
            "Bulk state transition processing completed"
        );

        Ok(results)
    }

    /// Get system-wide state health using SQL functions
    pub async fn get_state_health_summary(&self) -> OrchestrationResult<StateHealthSummary> {
        let health_counts = self
            .sql_executor
            .get_system_health_counts()
            .await
            .map_err(|e| OrchestrationError::SqlFunctionError {
                function_name: "get_system_health_counts".to_string(),
                reason: e.to_string(),
            })?;

        Ok(StateHealthSummary {
            total_tasks: health_counts.total_tasks,
            pending_tasks: health_counts.pending_tasks,
            in_progress_tasks: health_counts.in_progress_tasks,
            completed_tasks: health_counts.complete_tasks,
            failed_tasks: health_counts.error_tasks,
            total_steps: health_counts.total_steps,
            pending_steps: health_counts.pending_steps,
            in_progress_steps: health_counts.in_progress_steps,
            completed_steps: health_counts.complete_steps,
            failed_steps: health_counts.error_steps,
            overall_health_score: health_counts.health_score(),
            last_updated: Utc::now(),
        })
    }

    /// Calculate recommended task state based on step completion
    async fn calculate_recommended_task_state(
        &self,
        context: &tasker_shared::database::sql_functions::TaskExecutionContext,
        current_state: &TaskState,
    ) -> OrchestrationResult<(Option<TaskState>, Option<String>)> {
        match current_state {
            TaskState::Pending => {
                // Transition to enqueuing_steps if any steps are ready to be enqueued
                if context.total_steps - context.completed_steps - context.pending_steps > 0 {
                    Ok((
                        Some(TaskState::EnqueuingSteps),
                        Some("Steps are being enqueued for processing".to_string()),
                    ))
                } else {
                    Ok((None, None))
                }
            }
            TaskState::StepsInProcess => {
                // Check for completion or failure
                if context.failed_steps > 0
                    && context.completed_steps + context.failed_steps == context.total_steps
                {
                    // All steps done but some failed
                    Ok((
                        Some(TaskState::Error),
                        Some("Task failed due to step failures".to_string()),
                    ))
                } else if context.completed_steps == context.total_steps && context.total_steps > 0
                {
                    // All steps completed successfully
                    Ok((
                        Some(TaskState::Complete),
                        Some("All steps completed successfully".to_string()),
                    ))
                } else {
                    // Still in progress
                    Ok((None, None))
                }
            }
            _ => {
                // Terminal states don't need transitions
                Ok((None, None))
            }
        }
    }

    /// Calculate recommended step state based on SQL analysis
    fn calculate_recommended_step_state(
        &self,
        status: &tasker_shared::database::sql_functions::StepReadinessStatus,
        current_state: &WorkflowStepState,
    ) -> (Option<WorkflowStepState>, Option<String>) {
        match current_state {
            WorkflowStepState::Pending => {
                if status.ready_for_execution && status.dependencies_satisfied {
                    (
                        Some(WorkflowStepState::InProgress),
                        Some("Dependencies satisfied, ready to execute".to_string()),
                    )
                } else {
                    (None, None)
                }
            }
            WorkflowStepState::InProgress => {
                // SQL function doesn't directly tell us about completion,
                // that typically comes from step execution results
                (None, None)
            }
            _ => {
                // Terminal states don't need transitions
                (None, None)
            }
        }
    }

    /// Get or create task state machine
    async fn get_or_create_task_state_machine(
        &self,
        task_uuid: Uuid,
    ) -> OrchestrationResult<TaskStateMachine> {
        let mut task_machines = self.task_state_machines.lock().await;

        if let Some(machine) = task_machines.get(&task_uuid) {
            // Return cloned cached instance for efficiency
            Ok(machine.clone())
        } else {
            // Need to get task from database to create state machine
            let task = tasker_shared::models::core::task::Task::find_by_id(
                self.database_pool(),
                task_uuid,
            )
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "find_task_by_id".to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| OrchestrationError::InvalidTaskState {
                task_uuid,
                current_state: "not_found".to_string(),
                expected_states: vec!["pending".to_string(), "in_progress".to_string()],
            })?;

            let machine = TaskStateMachine::new(task, self.system_context.clone());

            // Store in cache for future use
            task_machines.insert(task_uuid, machine.clone());
            Ok(machine)
        }
    }

    /// Get or create step state machine
    pub async fn get_or_create_step_state_machine(
        &self,
        step_uuid: Uuid,
    ) -> OrchestrationResult<StepStateMachine> {
        let mut step_machines = self.step_state_machines.lock().await;

        if let Some(machine) = step_machines.get(&step_uuid) {
            // Return cloned cached instance for efficiency
            Ok(machine.clone())
        } else {
            // Need to get step from database to create state machine
            let step = tasker_shared::models::core::workflow_step::WorkflowStep::find_by_id(
                self.database_pool(),
                step_uuid,
            )
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "find_workflow_step_by_id".to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| OrchestrationError::InvalidStepState {
                step_uuid,
                current_state: "not_found".to_string(),
                expected_states: vec!["pending".to_string(), "in_progress".to_string()],
            })?;

            let machine = StepStateMachine::new(step, self.system_context.clone());

            // Store in cache for future use
            step_machines.insert(step_uuid, machine.clone());
            Ok(machine)
        }
    }

    /// Transition task state
    async fn transition_task_state(
        &self,
        task_uuid: Uuid,
        target_state: TaskState,
    ) -> OrchestrationResult<()> {
        let mut task_state_machine = self.get_or_create_task_state_machine(task_uuid).await?;

        let event = match target_state {
            TaskState::Initializing => TaskEvent::Start,
            TaskState::Complete => TaskEvent::Complete,
            TaskState::Error => TaskEvent::Fail("Task failed".to_string()),
            _ => return Ok(()), // No transition needed
        };

        task_state_machine.transition(event).await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "Task".to_string(),
                entity_uuid: task_uuid,
                reason: e.to_string(),
            }
        })?;

        Ok(())
    }

    /// Transition step state
    async fn transition_step_state(
        &self,
        step_uuid: Uuid,
        target_state: WorkflowStepState,
    ) -> OrchestrationResult<()> {
        let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;

        let event = match target_state {
            WorkflowStepState::InProgress => StepEvent::Start,
            WorkflowStepState::Complete => StepEvent::Complete(None),
            WorkflowStepState::Error => StepEvent::Fail("Step failed".to_string()),
            _ => return Ok(()), // No transition needed
        };

        step_state_machine.transition(event).await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_uuid: step_uuid,
                reason: e.to_string(),
            }
        })?;

        Ok(())
    }

    /// TAS-32 DEPRECATED: Complete step with results - stores results in database
    ///
    /// **DEPRECATED in TAS-32**: Ruby workers now handle step completion with MessageManager.
    /// This method is no longer called by Rust orchestration but kept for backward compatibility.
    #[deprecated(note = "Ruby workers handle step completion. Use Ruby MessageManager instead.")]
    pub async fn complete_step_with_results(
        &self,
        step_uuid: Uuid,
        step_results: Option<serde_json::Value>,
    ) -> OrchestrationResult<()> {
        let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;

        let event = StepEvent::Complete(step_results);

        step_state_machine.transition(event).await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_uuid: step_uuid,
                reason: e.to_string(),
            }
        })?;

        Ok(())
    }

    /// Fail step with error information - marks step as failed in database
    pub async fn fail_step_with_error(
        &self,
        step_uuid: Uuid,
        error_message: String,
    ) -> OrchestrationResult<()> {
        let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;

        let event = StepEvent::Fail(error_message);

        step_state_machine.transition(event).await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_uuid: step_uuid,
                reason: e.to_string(),
            }
        })?;

        Ok(())
    }

    /// TAS-32 DEPRECATED: Handle step failure with retry logic
    ///
    /// **DEPRECATED in TAS-32**: Ruby workers now handle step failures with MessageManager.
    /// This method is no longer called by Rust orchestration but kept for backward compatibility.
    #[deprecated(note = "Ruby workers handle step failures. Use Ruby MessageManager instead.")]
    pub async fn handle_step_failure_with_retry(
        &self,
        step_uuid: Uuid,
        error_message: String,
    ) -> OrchestrationResult<()> {
        let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;

        // First transition to error state
        let fail_event = StepEvent::Fail(error_message.clone());
        step_state_machine
            .transition(fail_event)
            .await
            .map_err(|e| OrchestrationError::StateTransitionFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_uuid: step_uuid,
                reason: e.to_string(),
            })?;

        // Check if step has retries remaining
        if !step_state_machine.has_exceeded_max_attempts() {
            // Transition back to pending for retry
            let retry_event = StepEvent::Retry;
            step_state_machine
                .transition(retry_event)
                .await
                .map_err(|e| OrchestrationError::StateTransitionFailed {
                    entity_type: "WorkflowStep".to_string(),
                    entity_uuid: step_uuid,
                    reason: format!("Failed to transition to pending for retry: {e}"),
                })?;

            tracing::info!(
                "Step {} failed but has retries remaining, transitioned to pending for retry. Error: {}",
                step_uuid,
                error_message
            );
        } else {
            tracing::info!(
                "Step {} failed and has no retries remaining, staying in error state. Error: {}",
                step_uuid,
                error_message
            );
        }

        Ok(())
    }

    /// Mark a task as in progress when steps have been enqueued
    pub async fn mark_task_in_progress(&self, task_uuid: Uuid) -> OrchestrationResult<()> {
        debug!(
            task_uuid = task_uuid.to_string(),
            "Marking task as in progress"
        );

        // Check current state to avoid invalid transitions
        let task_state_machine = self.get_or_create_task_state_machine(task_uuid).await?;
        let current_state = task_state_machine.current_state().await?;

        if current_state.is_active() {
            debug!(
                task_uuid = task_uuid.to_string(),
                current_state = %current_state,
                "Task is already in progress, skipping transition"
            );
            return Ok(());
        }

        debug!(
            task_uuid = task_uuid.to_string(),
            current_state = %current_state,
            "Transitioning task to in_progress"
        );

        self.transition_task_state(task_uuid, TaskState::StepsInProcess)
            .await
    }

    /// Mark step as enqueued - TAS-32: transitions pending → enqueued when step is queued
    pub async fn mark_step_enqueued(&self, step_uuid: Uuid) -> OrchestrationResult<()> {
        // 1. Transition state machine to Enqueued (pending → enqueued)
        let mut step_state_machine = self.get_or_create_step_state_machine(step_uuid).await?;

        let event = StepEvent::Enqueue;

        step_state_machine.transition(event).await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_uuid: step_uuid,
                reason: e.to_string(),
            }
        })?;

        debug!(step_uuid = step_uuid.to_string(), "Marked step as enqueued");
        Ok(())
    }

    /// Process task transition request
    async fn process_task_transition_request(
        &self,
        request: StateTransitionRequest,
    ) -> OrchestrationResult<StateEvaluationResult> {
        let mut task_state_machine = self
            .get_or_create_task_state_machine(request.entity_uuid)
            .await?;
        let current_state = task_state_machine.current_state().await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "task".to_string(),
                entity_uuid: request.entity_uuid,
                reason: format!("Failed to get current state: {e}"),
            }
        })?;

        if let StateTransitionEvent::TaskEvent(event) = request.event {
            match task_state_machine.transition(event).await {
                Ok(_) => Ok(StateEvaluationResult {
                    entity_uuid: request.entity_uuid,
                    entity_type: StateEntityType::Task,
                    current_state: current_state.to_string(),
                    recommended_state: Some(request.target_state),
                    transition_required: true,
                    reason: Some("Explicit transition request".to_string()),
                }),
                Err(e) => Err(OrchestrationError::StateTransitionFailed {
                    entity_type: "Task".to_string(),
                    entity_uuid: request.entity_uuid,
                    reason: e.to_string(),
                }),
            }
        } else {
            Err(OrchestrationError::ValidationError {
                field: "event".to_string(),
                reason: "Task transition request must use TaskEvent".to_string(),
            })
        }
    }

    /// Process step transition request
    async fn process_step_transition_request(
        &self,
        request: StateTransitionRequest,
    ) -> OrchestrationResult<StateEvaluationResult> {
        let mut step_state_machine = self
            .get_or_create_step_state_machine(request.entity_uuid)
            .await?;
        let current_state = step_state_machine.current_state().await?;

        if let StateTransitionEvent::StepEvent(event) = request.event {
            match step_state_machine.transition(event).await {
                Ok(_) => Ok(StateEvaluationResult {
                    entity_uuid: request.entity_uuid,
                    entity_type: StateEntityType::Step,
                    current_state: current_state.to_string(),
                    recommended_state: Some(request.target_state),
                    transition_required: true,
                    reason: Some("Explicit transition request".to_string()),
                }),
                Err(e) => Err(OrchestrationError::StateTransitionFailed {
                    entity_type: "WorkflowStep".to_string(),
                    entity_uuid: request.entity_uuid,
                    reason: e.to_string(),
                }),
            }
        } else {
            Err(OrchestrationError::ValidationError {
                field: "event".to_string(),
                reason: "Step transition request must use StepEvent".to_string(),
            })
        }
    }
}

/// Summary of system-wide state health
#[derive(Debug, Clone)]
pub struct StateHealthSummary {
    pub total_tasks: i64,
    pub pending_tasks: i64,
    pub in_progress_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub overall_health_score: f64,
    pub last_updated: DateTime<Utc>,
}

impl StateHealthSummary {
    /// Check if system is healthy
    pub fn is_healthy(&self) -> bool {
        self.overall_health_score >= 0.8
    }

    /// Get task completion percentage
    pub fn task_completion_percentage(&self) -> f64 {
        if self.total_tasks > 0 {
            self.completed_tasks as f64 / self.total_tasks as f64 * 100.0
        } else {
            0.0
        }
    }

    /// Get step completion percentage
    pub fn step_completion_percentage(&self) -> f64 {
        if self.total_steps > 0 {
            self.completed_steps as f64 / self.total_steps as f64 * 100.0
        } else {
            0.0
        }
    }
}
