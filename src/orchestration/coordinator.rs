//! # Orchestration Coordinator
//!
//! ## Architecture: State Machine Integration
//!
//! The OrchestrationCoordinator serves as the primary workflow orchestration engine,
//! integrating state machines for atomic state management with delegation-based
//! step execution. This follows the delegation pattern where Rust handles coordination
//! and frameworks (Rails, Python, etc.) handle step execution logic.
//!
//! ## Key Integration Points:
//! - **TaskStateMachine**: Manages overall task lifecycle (pending → in_progress → complete)
//! - **StepStateMachine**: Manages individual step states with dependency awareness
//! - **ViableStepDiscovery**: Uses SQL functions + state verification for readiness
//! - **Delegation Interface**: Clean separation between coordination and execution

use crate::error::{OrchestrationError, OrchestrationResult};
use crate::events::publisher::EventPublisher;
use crate::models::core::task::Task;
// use crate::models::core::workflow_step::WorkflowStep; // TODO: Enable when get_by_id is implemented
use crate::models::orchestration::step_readiness_status::StepReadinessStatus;
use crate::models::orchestration::task_execution_context::TaskExecutionContext;
use crate::orchestration::viable_step_discovery::ViableStepDiscovery;
use crate::state_machine::events::{StepEvent, TaskEvent};
use crate::state_machine::states::{TaskState, WorkflowStepState};
use crate::state_machine::step_state_machine::StepStateMachine;
use crate::state_machine::task_state_machine::TaskStateMachine;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Result type for step execution
#[derive(Debug, Clone)]
pub struct StepExecutionResult {
    pub step_id: i64,
    pub status: StepExecutionStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Status of step execution
#[derive(Debug, Clone, PartialEq)]
pub enum StepExecutionStatus {
    Success,
    Error,
    Skipped,
    Retry,
}

/// Viable step ready for execution
#[derive(Debug, Clone)]
pub struct ViableStep {
    pub workflow_step_id: i64,
    pub step_name: String,
    pub task_id: i64,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub named_step_id: i32,
}

impl From<StepReadinessStatus> for ViableStep {
    fn from(status: StepReadinessStatus) -> Self {
        Self {
            workflow_step_id: status.workflow_step_id,
            step_name: status.name, // Note: field is 'name', not 'step_name'
            task_id: status.task_id,
            dependencies_satisfied: status.dependencies_satisfied,
            retry_eligible: status.retry_eligible,
            named_step_id: status.named_step_id, // Using this instead of priority for now
        }
    }
}

/// Delegation interface for framework-specific step execution
#[async_trait]
pub trait StepExecutionDelegate: Send + Sync {
    /// Execute a batch of viable steps
    async fn execute_steps(
        &self,
        task_id: i64,
        steps: &[ViableStep],
    ) -> OrchestrationResult<Vec<StepExecutionResult>>;

    /// Framework name for logging/metrics
    fn framework_name(&self) -> &'static str;
}

/// Result of task orchestration
#[derive(Debug)]
pub enum TaskOrchestrationResult {
    /// Task completed successfully
    Complete {
        task_id: i64,
        steps_executed: usize,
        total_execution_time_ms: u64,
    },
    /// Task failed due to step failures
    Failed {
        task_id: i64,
        error: String,
        failed_steps: Vec<i64>,
    },
    /// Task is still in progress, should be re-queued
    InProgress {
        task_id: i64,
        steps_executed: usize,
        next_poll_delay_ms: u64,
    },
    /// Task is blocked waiting for dependencies
    Blocked {
        task_id: i64,
        blocking_reason: String,
    },
}

/// Main orchestration coordinator with state machine integration
pub struct OrchestrationCoordinator {
    task_state_machine: TaskStateMachine,
    step_state_machines: Arc<Mutex<HashMap<i64, StepStateMachine>>>,
    step_discovery: ViableStepDiscovery,
    pool: PgPool,
    #[allow(dead_code)] // TODO: Will be used for orchestration events in future iterations
    event_publisher: EventPublisher,
}

impl OrchestrationCoordinator {
    /// Create new coordinator for a specific task
    pub fn new(task: Task, pool: PgPool, event_publisher: EventPublisher) -> Self {
        let task_state_machine = TaskStateMachine::new(task, pool.clone(), event_publisher.clone());
        let step_discovery = ViableStepDiscovery::new(pool.clone());

        Self {
            task_state_machine,
            step_state_machines: Arc::new(Mutex::new(HashMap::new())),
            step_discovery,
            pool,
            event_publisher,
        }
    }

    /// Main orchestration method - coordinate task execution using delegation
    pub async fn orchestrate_task(
        &mut self,
        delegate: &dyn StepExecutionDelegate,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        let task_id = self.task_state_machine.task_id();

        tracing::info!(
            task_id = task_id,
            framework = delegate.framework_name(),
            "Starting task orchestration"
        );

        // 1. Ensure task is in correct state for orchestration
        self.ensure_task_ready().await?;

        // 2. Discover viable steps using SQL functions + state verification
        let viable_steps = self.step_discovery.find_viable_steps(task_id).await?;

        if viable_steps.is_empty() {
            return self.handle_no_viable_steps(task_id).await;
        }

        // 3. Transition step states to in_progress
        self.transition_steps_to_in_progress(&viable_steps).await?;

        // 4. Delegate execution to framework
        let execution_start = std::time::Instant::now();
        let results = delegate.execute_steps(task_id, &viable_steps).await?;
        let total_execution_time = execution_start.elapsed().as_millis() as u64;

        // 5. Process results and update step states
        self.process_step_results(&results).await?;

        // 6. Check for task completion and return appropriate result
        self.determine_orchestration_result(task_id, results.len(), total_execution_time)
            .await
    }

    /// Ensure task is in correct state for orchestration
    async fn ensure_task_ready(&mut self) -> OrchestrationResult<()> {
        // Transition from pending to in_progress if needed
        let current_state = self.task_state_machine.current_state().await?;

        match current_state {
            TaskState::Pending => {
                self.task_state_machine
                    .transition(TaskEvent::Start)
                    .await
                    .map_err(|e| OrchestrationError::StateTransitionFailed {
                        entity_type: "Task".to_string(),
                        entity_id: self.task_state_machine.task_id(),
                        reason: e.to_string(),
                    })?;
            }
            TaskState::InProgress => {
                // Already in progress, continue
            }
            other_state => {
                return Err(OrchestrationError::InvalidTaskState {
                    task_id: self.task_state_machine.task_id(),
                    current_state: other_state.to_string(),
                    expected_states: vec!["pending".to_string(), "in_progress".to_string()],
                });
            }
        }
        Ok(())
    }

    /// Transition viable steps to in_progress state
    async fn transition_steps_to_in_progress(
        &self,
        viable_steps: &[ViableStep],
    ) -> OrchestrationResult<()> {
        let mut step_machines = self.step_state_machines.lock().await;

        for step in viable_steps {
            let step_machine = self
                .get_or_create_step_machine(&mut step_machines, step.workflow_step_id)
                .await?;

            // Transition to in_progress if currently pending
            let current_state = step_machine.current_state().await?;
            if current_state == WorkflowStepState::Pending {
                step_machine
                    .transition(StepEvent::Start)
                    .await
                    .map_err(|e| OrchestrationError::StateTransitionFailed {
                        entity_type: "WorkflowStep".to_string(),
                        entity_id: step.workflow_step_id,
                        reason: e.to_string(),
                    })?;
            }
        }
        Ok(())
    }

    /// Process step execution results and update states
    async fn process_step_results(
        &self,
        results: &[StepExecutionResult],
    ) -> OrchestrationResult<()> {
        let mut step_machines = self.step_state_machines.lock().await;

        for result in results {
            let step_machine = step_machines.get_mut(&result.step_id).ok_or({
                OrchestrationError::StepStateMachineNotFound {
                    step_id: result.step_id,
                }
            })?;

            match result.status {
                StepExecutionStatus::Success => {
                    let output = result.output.clone(); // Already Option<Value>
                    step_machine
                        .transition(StepEvent::Complete(output))
                        .await
                        .map_err(|e| OrchestrationError::StateTransitionFailed {
                            entity_type: "WorkflowStep".to_string(),
                            entity_id: result.step_id,
                            reason: e.to_string(),
                        })?;
                }
                StepExecutionStatus::Error => {
                    let error = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "Unknown error".to_string());
                    step_machine
                        .transition(StepEvent::Fail(error))
                        .await
                        .map_err(|e| OrchestrationError::StateTransitionFailed {
                            entity_type: "WorkflowStep".to_string(),
                            entity_id: result.step_id,
                            reason: e.to_string(),
                        })?;
                }
                StepExecutionStatus::Retry => {
                    // Use Retry event (no parameter)
                    step_machine
                        .transition(StepEvent::Retry)
                        .await
                        .map_err(|e| OrchestrationError::StateTransitionFailed {
                            entity_type: "WorkflowStep".to_string(),
                            entity_id: result.step_id,
                            reason: e.to_string(),
                        })?;
                }
                StepExecutionStatus::Skipped => {
                    // Use Cancel event as equivalent to skip
                    step_machine
                        .transition(StepEvent::Cancel)
                        .await
                        .map_err(|e| OrchestrationError::StateTransitionFailed {
                            entity_type: "WorkflowStep".to_string(),
                            entity_id: result.step_id,
                            reason: e.to_string(),
                        })?;
                }
            }
        }
        Ok(())
    }

    /// Handle case when no viable steps are found
    async fn handle_no_viable_steps(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        // Check task execution context to understand why no steps are viable
        let context = TaskExecutionContext::get_for_task(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_task_execution_context".to_string(),
                reason: e.to_string(),
            })?;

        let context = context.ok_or_else(|| OrchestrationError::DatabaseError {
            operation: "get_task_execution_context".to_string(),
            reason: format!("No execution context found for task {task_id}"),
        })?;

        match context.execution_status.as_str() {
            "all_complete" => {
                // All steps complete - finalize task
                Ok(TaskOrchestrationResult::Complete {
                    task_id,
                    steps_executed: 0,
                    total_execution_time_ms: 0,
                })
            }
            "blocked_by_failures" => {
                Ok(TaskOrchestrationResult::Failed {
                    task_id,
                    error: "Task blocked by failed dependencies".to_string(),
                    failed_steps: Vec::new(), // TODO: Extract from context
                })
            }
            "blocked_by_dependencies" => Ok(TaskOrchestrationResult::Blocked {
                task_id,
                blocking_reason: "Waiting for external dependencies".to_string(),
            }),
            _ => {
                Ok(TaskOrchestrationResult::InProgress {
                    task_id,
                    steps_executed: 0,
                    next_poll_delay_ms: 5000, // 5 second default
                })
            }
        }
    }

    /// Determine final orchestration result based on current state
    async fn determine_orchestration_result(
        &mut self,
        task_id: i64,
        steps_executed: usize,
        total_execution_time_ms: u64,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        let context = TaskExecutionContext::get_for_task(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_task_execution_context".to_string(),
                reason: e.to_string(),
            })?;

        let context = context.ok_or_else(|| OrchestrationError::DatabaseError {
            operation: "get_task_execution_context".to_string(),
            reason: format!("No execution context found for task {task_id}"),
        })?;

        match context.execution_status.as_str() {
            "all_complete" => {
                // Transition task to complete
                self.task_state_machine
                    .transition(TaskEvent::Complete)
                    .await
                    .map_err(|e| OrchestrationError::StateTransitionFailed {
                        entity_type: "Task".to_string(),
                        entity_id: task_id,
                        reason: e.to_string(),
                    })?;

                Ok(TaskOrchestrationResult::Complete {
                    task_id,
                    steps_executed,
                    total_execution_time_ms,
                })
            }
            "blocked_by_failures" => {
                // Transition task to error
                self.task_state_machine
                    .transition(TaskEvent::Fail("Steps failed".to_string()))
                    .await
                    .map_err(|e| OrchestrationError::StateTransitionFailed {
                        entity_type: "Task".to_string(),
                        entity_id: task_id,
                        reason: e.to_string(),
                    })?;

                Ok(TaskOrchestrationResult::Failed {
                    task_id,
                    error: "Task failed due to step failures".to_string(),
                    failed_steps: Vec::new(), // TODO: Extract from context
                })
            }
            _ => {
                // Task still in progress
                Ok(TaskOrchestrationResult::InProgress {
                    task_id,
                    steps_executed,
                    next_poll_delay_ms: 2000, // 2 second delay
                })
            }
        }
    }

    /// Get or create step state machine for a workflow step
    async fn get_or_create_step_machine<'a>(
        &self,
        step_machines: &'a mut HashMap<i64, StepStateMachine>,
        workflow_step_id: i64,
    ) -> OrchestrationResult<&'a mut StepStateMachine> {
        if !step_machines.contains_key(&workflow_step_id) {
            // TODO: Implement WorkflowStep::get_by_id method
            // For now, return an error indicating this functionality is not yet implemented
            return Err(OrchestrationError::DatabaseError {
                operation: "get_workflow_step".to_string(),
                reason: "WorkflowStep::get_by_id not yet implemented".to_string(),
            });
        }

        Ok(step_machines.get_mut(&workflow_step_id).unwrap())
    }

    /// Get task ID for this coordinator
    pub fn task_id(&self) -> i64 {
        self.task_state_machine.task_id()
    }

    /// Get current task state
    pub async fn current_task_state(&self) -> OrchestrationResult<TaskState> {
        self.task_state_machine.current_state().await.map_err(|e| {
            OrchestrationError::StateTransitionFailed {
                entity_type: "Task".to_string(),
                entity_id: self.task_id(),
                reason: e.to_string(),
            }
        })
    }
}
