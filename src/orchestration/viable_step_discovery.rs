//! # Viable Step Discovery
//!
//! ## Architecture: SQL Functions + State Machine Verification
//!
//! This component uses the high-performance SQL functions from the orchestration models
//! combined with state machine verification to determine which steps are ready for execution.
//! This approach balances performance (SQL functions) with consistency (state machine validation).

use crate::error::{OrchestrationError, OrchestrationResult};
use crate::models::orchestration::step_readiness_status::StepReadinessStatus;
use crate::orchestration::coordinator::ViableStep;
use crate::state_machine::persistence::{resolve_state_with_retry, StepTransitionPersistence};
use sqlx::PgPool;

/// High-performance step readiness discovery engine
pub struct ViableStepDiscovery {
    pool: PgPool,
    step_persistence: StepTransitionPersistence,
}

impl ViableStepDiscovery {
    /// Create new step discovery instance
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            step_persistence: StepTransitionPersistence,
        }
    }

    /// Find all viable steps for a task using SQL functions + state verification
    pub async fn find_viable_steps(&self, task_id: i64) -> OrchestrationResult<Vec<ViableStep>> {
        tracing::debug!(task_id = task_id, "Finding viable steps");

        // 1. Get step readiness status using high-performance SQL functions
        let readiness_statuses = StepReadinessStatus::get_for_task(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_step_readiness_status".to_string(),
                reason: e.to_string(),
            })?;

        tracing::debug!(
            task_id = task_id,
            candidate_steps = readiness_statuses.len(),
            "Retrieved step readiness statuses"
        );

        // 2. Filter to ready steps using SQL function results
        let candidate_steps: Vec<_> = readiness_statuses
            .into_iter()
            .filter(|status| {
                status.ready_for_execution
                    && status.dependencies_satisfied
                    && status.current_state != "complete" // Not completed
            })
            .collect();

        tracing::debug!(
            task_id = task_id,
            ready_steps = candidate_steps.len(),
            "Filtered to ready steps"
        );

        // 3. Verify state machine consistency for each candidate
        let mut viable_steps = Vec::new();
        for status in candidate_steps {
            match self.verify_step_state_consistency(&status).await {
                Ok(true) => {
                    viable_steps.push(ViableStep::from(status));
                }
                Ok(false) => {
                    tracing::warn!(
                        task_id = task_id,
                        step_id = status.workflow_step_id,
                        "Step failed state machine consistency check"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        task_id = task_id,
                        step_id = status.workflow_step_id,
                        error = %e,
                        "Error verifying step state consistency"
                    );
                    // Continue with other steps rather than failing entire discovery
                }
            }
        }

        // 4. Sort by named_step_id (lower ID = higher priority)
        viable_steps.sort_by(|a, b| a.named_step_id.cmp(&b.named_step_id));

        tracing::info!(
            task_id = task_id,
            viable_steps = viable_steps.len(),
            "Completed viable step discovery"
        );

        Ok(viable_steps)
    }

    /// Verify that a step's SQL-reported readiness matches state machine state
    async fn verify_step_state_consistency(
        &self,
        status: &StepReadinessStatus,
    ) -> OrchestrationResult<bool> {
        // Get current state from state machine persistence layer with retry logic
        let current_state = resolve_state_with_retry(
            &self.step_persistence,
            status.workflow_step_id,
            &self.pool,
            3, // max retries
        )
        .await
        .map_err(|e| OrchestrationError::StateVerificationFailed {
            step_id: status.workflow_step_id,
            reason: e.to_string(),
        })?;

        // Verify state consistency
        match current_state {
            Some(state) => {
                // Step should be in 'pending' state to be viable for execution
                let is_consistent = state == "pending" && status.ready_for_execution;

                if !is_consistent {
                    tracing::debug!(
                        step_id = status.workflow_step_id,
                        current_state = state,
                        sql_ready = status.ready_for_execution,
                        "State machine and SQL readiness mismatch"
                    );
                }

                Ok(is_consistent)
            }
            None => {
                // No state transitions yet - step should be ready if SQL says so
                Ok(status.ready_for_execution)
            }
        }
    }

    /// Get viable steps filtered by specific criteria
    pub async fn find_viable_steps_with_criteria(
        &self,
        task_id: i64,
        max_steps: Option<usize>,
        min_priority: Option<i32>,
        step_names: Option<&[String]>,
    ) -> OrchestrationResult<Vec<ViableStep>> {
        let mut viable_steps = self.find_viable_steps(task_id).await?;

        // Apply priority filter (using named_step_id as priority)
        if let Some(min_priority) = min_priority {
            viable_steps.retain(|step| step.named_step_id >= min_priority);
        }

        // Apply step name filter
        if let Some(step_names) = step_names {
            viable_steps.retain(|step| step_names.contains(&step.step_name));
        }

        // Apply max steps limit
        if let Some(max_steps) = max_steps {
            viable_steps.truncate(max_steps);
        }

        Ok(viable_steps)
    }

    /// Get readiness summary for a task (for monitoring/debugging)
    pub async fn get_task_readiness_summary(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<TaskReadinessSummary> {
        let statuses = StepReadinessStatus::get_for_task(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_step_readiness_status".to_string(),
                reason: e.to_string(),
            })?;

        let total_steps = statuses.len();
        let ready_steps = statuses.iter().filter(|s| s.ready_for_execution).count();
        let complete_steps = statuses
            .iter()
            .filter(|s| s.current_state == "complete")
            .count();
        let blocked_steps = statuses
            .iter()
            .filter(|s| !s.dependencies_satisfied)
            .count();
        let failed_steps = statuses
            .iter()
            .filter(|s| s.current_state == "error")
            .count();

        Ok(TaskReadinessSummary {
            task_id,
            total_steps,
            ready_steps,
            complete_steps,
            blocked_steps,
            failed_steps,
            progress_percentage: if total_steps > 0 {
                (complete_steps as f64 / total_steps as f64 * 100.0) as u8
            } else {
                0
            },
        })
    }
}

/// Summary of task readiness status for monitoring
#[derive(Debug, Clone)]
pub struct TaskReadinessSummary {
    pub task_id: i64,
    pub total_steps: usize,
    pub ready_steps: usize,
    pub complete_steps: usize,
    pub blocked_steps: usize,
    pub failed_steps: usize,
    pub progress_percentage: u8,
}

impl TaskReadinessSummary {
    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        self.complete_steps == self.total_steps && self.total_steps > 0
    }

    /// Check if task is blocked
    pub fn is_blocked(&self) -> bool {
        self.ready_steps == 0 && self.complete_steps < self.total_steps
    }

    /// Check if task has failures
    pub fn has_failures(&self) -> bool {
        self.failed_steps > 0
    }
}
