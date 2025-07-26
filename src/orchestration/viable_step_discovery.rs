//! # Viable Step Discovery
//!
//! ## Architecture: SQL Functions + State Machine Verification
//!
//! This component uses the high-performance SQL functions from the existing Rails system
//! combined with state machine verification to determine which steps are ready for execution.
//! This approach balances performance (SQL functions) with consistency (state machine validation).
//!
//! ## Key Features
//!
//! - **SQL-driven discovery**: Uses existing `get_step_readiness_status()` function
//! - **State machine verification**: Ensures consistency between SQL results and state machine state
//! - **Dependency analysis**: Leverages `calculate_dependency_levels()` for complex workflows
//! - **Task execution context**: Uses `get_task_execution_context()` for comprehensive analysis
//! - **Circuit breaker integration**: Respects circuit breaker logic from SQL functions
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::orchestration::viable_step_discovery::ViableStepDiscovery;
//! use tasker_core::events::publisher::EventPublisher;
//! use tasker_core::database::sql_functions::SqlFunctionExecutor;
//!
//! // Create ViableStepDiscovery for step readiness analysis
//! # use sqlx::PgPool;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = PgPool::connect("postgresql://localhost/nonexistent").await?;
//! let sql_executor = SqlFunctionExecutor::new(pool.clone());
//! let event_publisher = EventPublisher::new();
//! let discovery = ViableStepDiscovery::new(sql_executor, event_publisher, pool);
//!
//! // ViableStepDiscovery uses SQL functions to determine step readiness
//! // Verify creation succeeded (we can't test SQL functions without a real database)
//! let _discovery = discovery;
//! # Ok(())
//! # }
//!
//! // For complete integration examples, see tests/orchestration/viable_step_discovery_integration.rs
//! ```

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::events::{EventPublisher, ViableStep as EventsViableStep};
use crate::orchestration::errors::{DiscoveryError, OrchestrationResult};
use crate::orchestration::types::ViableStep;
use crate::state_machine::persistence::{resolve_state_with_retry, StepTransitionPersistence};
use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

/// High-performance step readiness discovery engine
pub struct ViableStepDiscovery {
    sql_executor: SqlFunctionExecutor,
    event_publisher: EventPublisher,
    step_persistence: StepTransitionPersistence,
    pool: sqlx::PgPool,
}

impl ViableStepDiscovery {
    /// Create new step discovery instance
    pub fn new(
        sql_executor: SqlFunctionExecutor,
        event_publisher: EventPublisher,
        pool: sqlx::PgPool,
    ) -> Self {
        Self {
            sql_executor,
            event_publisher,
            step_persistence: StepTransitionPersistence,
            pool,
        }
    }

    /// Find all viable steps for a task using SQL functions + state verification
    #[instrument(skip(self), fields(task_id = task_id))]
    pub async fn find_viable_steps(&self, task_id: i64) -> OrchestrationResult<Vec<ViableStep>> {
        debug!(task_id = task_id, "Finding viable steps");

        // 1. Get step readiness status using SqlFunctionExecutor
        let readiness_statuses = self
            .sql_executor
            .get_step_readiness_status(task_id, None)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_step_readiness_status".to_string(),
                reason: e.to_string(),
            })?;

        debug!(
            task_id = task_id,
            total_statuses = readiness_statuses.len(),
            "Retrieved step readiness statuses from SQL function"
        );

        // Log each status for debugging
        for status in &readiness_statuses {
            debug!(
                task_id = task_id,
                step_name = %status.name,
                current_state = %status.current_state,
                ready_for_execution = status.ready_for_execution,
                dependencies_satisfied = status.dependencies_satisfied,
                retry_eligible = status.retry_eligible,
                attempts = status.attempts,
                retry_limit = status.retry_limit,
                total_parents = status.total_parents,
                completed_parents = status.completed_parents,
                "Step readiness status details"
            );
        }

        // 2. Filter to only steps that are ready for execution
        // SQL function returns all steps but marks readiness - we only want the ready ones
        let candidate_steps: Vec<_> = readiness_statuses
            .into_iter()
            .filter(|status| status.ready_for_execution)
            .collect();

        debug!(
            task_id = task_id,
            ready_steps = candidate_steps.len(),
            "Filtered to ready steps"
        );

        // 3. Convert to ViableStep objects and verify state machine consistency
        let mut viable_steps = Vec::new();
        for status in candidate_steps {
            match self.verify_step_state_consistency(&status).await {
                Ok(true) => {
                    let viable_step = ViableStep {
                        step_id: status.workflow_step_id,
                        task_id: status.task_id,
                        name: status.name,
                        named_step_id: status.named_step_id,
                        current_state: status.current_state,
                        dependencies_satisfied: status.dependencies_satisfied,
                        retry_eligible: status.retry_eligible,
                        attempts: status.attempts,
                        retry_limit: status.retry_limit,
                        last_failure_at: status.last_failure_at,
                        next_retry_at: status.next_retry_at,
                    };
                    viable_steps.push(viable_step);
                }
                Ok(false) => {
                    warn!(
                        task_id = task_id,
                        step_id = status.workflow_step_id,
                        "Step failed state machine consistency check"
                    );
                }
                Err(e) => {
                    error!(
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

        info!(
            task_id = task_id,
            viable_steps = viable_steps.len(),
            "Completed viable step discovery"
        );

        // 5. Publish discovery event
        let events_viable_steps: Vec<EventsViableStep> = viable_steps
            .iter()
            .map(|step| EventsViableStep {
                step_id: step.step_id,
                task_id: step.task_id,
                name: step.name.clone(),
                named_step_id: step.named_step_id as i64,
                current_state: step.current_state.clone(),
                dependencies_satisfied: step.dependencies_satisfied,
                retry_eligible: step.retry_eligible,
                attempts: step.attempts as u32,
                retry_limit: step.retry_limit as u32,
                last_failure_at: step.last_failure_at.map(|dt| dt.and_utc()),
                next_retry_at: step.next_retry_at.map(|dt| dt.and_utc()),
            })
            .collect();

        self.event_publisher
            .publish_viable_steps_discovered(task_id, &events_viable_steps)
            .await?;

        Ok(viable_steps)
    }

    /// Verify that a step's SQL-reported readiness matches state machine state
    async fn verify_step_state_consistency(
        &self,
        status: &crate::database::sql_functions::StepReadinessStatus,
    ) -> OrchestrationResult<bool> {
        // Get current state from state machine persistence layer with retry logic
        let current_state = resolve_state_with_retry(
            &self.step_persistence,
            status.workflow_step_id,
            &self.pool,
            3, // max retries
        )
        .await
        .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

        // Verify state consistency
        match current_state {
            Some(state) => {
                // Step should be in 'pending' state to be viable for execution
                let is_consistent = state == "pending" && status.ready_for_execution;

                if !is_consistent {
                    debug!(
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

    /// Get dependency levels using SQL function
    pub async fn get_dependency_levels(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<HashMap<i64, i32>> {
        self.sql_executor
            .dependency_levels_hash(task_id)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "calculate_dependency_levels".to_string(),
                reason: e.to_string(),
            })
            .map_err(Into::into)
    }

    /// Get task execution context using SQL function
    pub async fn get_execution_context(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<Option<crate::database::sql_functions::TaskExecutionContext>> {
        self.sql_executor
            .get_task_execution_context(task_id)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_task_execution_context".to_string(),
                reason: e.to_string(),
            })
            .map_err(Into::into)
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
            viable_steps.retain(|step| step_names.contains(&step.name));
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
        let statuses = self
            .sql_executor
            .get_step_readiness_status(task_id, None)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_step_readiness_status".to_string(),
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
