use crate::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
use crate::orchestration::StepEnqueueResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::config::orchestration::TaskClaimStepEnqueuerConfig;
use tasker_shared::database::sql_functions::{ReadyTaskInfo, SqlFunctionExecutor};
use tasker_shared::models::orchestration::ExecutionStatus;
use tasker_shared::state_machine::events::TaskEvent;
use tasker_shared::state_machine::states::TaskState;
use tasker_shared::state_machine::task_state_machine::TaskStateMachine;
use tasker_shared::{SystemContext, TaskerError, TaskerResult};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Result of a single orchestration cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueuerServiceResult {
    /// Cycle execution timestamp
    pub cycle_started_at: DateTime<Utc>,
    /// Total time for the complete cycle
    pub cycle_duration_ms: u64,
    /// Number of tasks successfully processed
    pub tasks_processed: usize,
    /// Number of tasks that failed processing
    pub tasks_failed: usize,
    /// Priority distribution of claimed tasks
    pub priority_distribution: PriorityDistribution,
    /// Per-namespace enqueueing statistics
    pub namespace_stats: HashMap<String, NamespaceStats>,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Any warnings encountered during the cycle
    pub warnings: Vec<String>,
}

/// Priority distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PriorityDistribution {
    pub urgent_tasks: usize,
    pub high_tasks: usize,
    pub normal_tasks: usize,
    pub low_tasks: usize,
    pub invalid_tasks: usize,
    pub escalated_tasks: usize,
    pub avg_computed_priority: f64,
    pub avg_task_age_hours: f64,
}

/// Per-namespace statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceStats {
    pub tasks_processed: usize,
    pub steps_enqueued: usize,
    pub steps_failed: usize,
    pub queue_name: String,
    pub avg_processing_time_ms: u64,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub claim_duration_ms: u64,
    pub discovery_duration_ms: u64,
    pub enqueueing_duration_ms: u64,
    pub release_duration_ms: u64,
    pub avg_task_processing_ms: u64,
    pub steps_per_second: f64,
    pub tasks_per_second: f64,
}

/// Aggregate summary for continuous orchestration to prevent memory bloat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousOrchestrationSummary {
    /// When continuous orchestration started
    pub started_at: DateTime<Utc>,
    /// When continuous orchestration ended (None if still running)
    pub ended_at: Option<DateTime<Utc>>,
    /// Total number of cycles completed
    pub total_cycles: u64,
    /// Total number of cycles that failed
    pub failed_cycles: u64,
    /// Total tasks processed across all cycles
    pub total_tasks_processed: u64,
    /// Total tasks that failed processing
    pub total_tasks_failed: u64,
    /// Total steps enqueued across all cycles
    pub total_steps_enqueued: u64,
    /// Total steps that failed to enqueue
    pub total_steps_failed: u64,
    /// Aggregate priority distribution across all cycles
    pub aggregate_priority_distribution: PriorityDistribution,
    /// Performance metrics aggregated across cycles
    pub aggregate_performance_metrics: AggregatePerformanceMetrics,
    /// Most active namespaces (top 10)
    pub top_namespaces: Vec<(String, u64)>, // (namespace, steps_enqueued)
    /// Total warnings collected
    pub total_warnings: u64,
    /// Sample of recent warnings (last 50)
    pub recent_warnings: Vec<String>,
}

/// Aggregate performance metrics to track long-running orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatePerformanceMetrics {
    pub total_cycle_duration_ms: u64,
    pub total_claim_duration_ms: u64,
    pub total_discovery_duration_ms: u64,
    pub total_enqueueing_duration_ms: u64,
    pub total_release_duration_ms: u64,
    pub peak_steps_per_second: f64,
    pub peak_tasks_per_second: f64,
    pub avg_steps_per_second: f64,
    pub avg_tasks_per_second: f64,
}

/// Main orchestration loop coordinator
pub struct StepEnqueuerService {
    step_enqueuer: Arc<StepEnqueuer>,
    context: Arc<SystemContext>,
    config: TaskClaimStepEnqueuerConfig,
}

impl StepEnqueuerService {
    /// Create a new orchestration loop
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);

        Ok(Self {
            step_enqueuer,
            context,
            config,
        })
    }

    /// Run a single orchestration cycle using state machine approach
    #[instrument(skip(self), fields(system_id = %self.context.processor_uuid()))]
    pub async fn process_batch(&self) -> TaskerResult<StepEnqueuerServiceResult> {
        let cycle_start = Instant::now();
        let cycle_started_at = Utc::now();

        debug!(
            "Starting cycle using state machine - system_id: {}, tasks_per_cycle: {}, namespace_filter: {:?}",
            self.context.processor_uuid, self.config.batch_size, self.config.namespace_filter
        );

        // Get ready tasks using SqlFunctionExecutor (query operation)
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let ready_tasks = sql_executor
            .get_next_ready_tasks(self.config.batch_size as i32)
            .await?;

        debug!(
            "Task discovery completed - found {} ready tasks",
            ready_tasks.len()
        );

        if ready_tasks.is_empty() {
            debug!("⏸No ready tasks available in this cycle - orchestration cycle ending early");
            return Ok(self.create_empty_cycle_result(
                cycle_started_at,
                cycle_start.elapsed().as_millis() as u64,
            ));
        }

        // Log details about ready tasks
        for task in &ready_tasks {
            debug!(
                "📋 Ready task {} (namespace: '{}') with {} ready steps, priority: {}, state: {}",
                task.task_uuid, task.namespace_name, task.ready_steps_count, task.priority, task.current_state
            );
        }

        // Process tasks concurrently using state machine
        let context = self.context.clone();
        let mut task_futures = Vec::new();

        for task_info in ready_tasks {
            let system_context = Arc::clone(&context);
            let owned_step_enqueuer = self.step_enqueuer.clone();

            let future = tokio::spawn(async move {
                Self::process_single_task_owned(&task_info, system_context, owned_step_enqueuer)
                    .await
                    .map(|success| (task_info.task_uuid, success))
            });

            task_futures.push(future);
        }

        // Wait for all tasks to complete
        let results = futures::future::join_all(task_futures).await;
        let mut processed = 0;
        let mut failed = 0;

        for result in results {
            match result {
                Ok(Ok((_, Some(_)))) => processed += 1,
                Ok(Ok((_, None))) => {} // Already processing elsewhere
                _ => failed += 1,
            }
        }

        let cycle_duration_ms = cycle_start.elapsed().as_millis() as u64;

        info!(
            processed,
            failed,
            duration_ms = cycle_duration_ms,
            "Batch processing complete"
        );

        Ok(StepEnqueuerServiceResult {
            cycle_started_at,
            cycle_duration_ms,
            tasks_processed: processed,
            tasks_failed: failed,
            priority_distribution: PriorityDistribution::default(), // Not critical for state machine approach
            namespace_stats: HashMap::new(), // Complex to track without significant refactoring
            performance_metrics: PerformanceMetrics {
                // Legacy claim fields (not applicable in state machine approach)
                claim_duration_ms: 0,
                release_duration_ms: 0,
                // Simplified duration estimates
                discovery_duration_ms: cycle_duration_ms / 3,
                enqueueing_duration_ms: cycle_duration_ms / 3,
                avg_task_processing_ms: if processed > 0 {
                    cycle_duration_ms / processed as u64
                } else {
                    0
                },
                // Simplified throughput metrics
                steps_per_second: if cycle_duration_ms > 0 && processed > 0 {
                    processed as f64 / (cycle_duration_ms as f64 / 1000.0)
                } else {
                    0.0
                },
                tasks_per_second: if cycle_duration_ms > 0 {
                    processed as f64 / (cycle_duration_ms as f64 / 1000.0)
                } else {
                    0.0
                },
            },
            warnings: vec![],
        })
    }

    pub async fn process_single_task_from_ready_info(
        &self,
        task_info: &ReadyTaskInfo,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        Self::process_single_task_owned(task_info, self.context.clone(), self.step_enqueuer.clone())
            .await
    }

    pub async fn process_single_task_from_uuid(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let task_info = sql_executor.get_task_ready_info(task_uuid).await?;
        if let Some(task_info) = task_info {
            Self::process_single_task_owned(
                &task_info,
                self.context.clone(),
                self.step_enqueuer.clone(),
            )
            .await
        } else {
            Err(TaskerError::OrchestrationError(format!(
                "Task not found: {}",
                task_uuid
            )))
        }
    }

    /// Process a single task with proper state machine transitions
    async fn process_single_task_owned(
        task_info: &ReadyTaskInfo,
        context: Arc<SystemContext>,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        // Create state machine for this task
        let mut state_machine = TaskStateMachine::for_task(
            task_info.task_uuid,
            context.database_pool().clone(),
            context.processor_uuid(),
        )
        .await
        .map_err(|err| {
            TaskerError::DatabaseError(format!("Failed to create state machine: {err}"))
        })?;

        let current_state = state_machine.current_state().await.map_err(|err| {
            TaskerError::DatabaseError(format!("Failed to get current state: {err}"))
        })?;

        // Handle state-specific transitions using proper state machine
        match current_state {
            TaskState::Pending => {
                // Start the task
                if state_machine.transition(TaskEvent::Start).await? {
                    Ok(Self::handle_initializing_task(
                        context.clone(),
                        &mut state_machine,
                        task_info,
                        step_enqueuer,
                    )
                    .await?)
                } else {
                    Ok(None) // Already claimed by another processor
                }
            }

            TaskState::Initializing => {
                // Task is already in Initializing state (likely from initialization process)
                // Process it directly
                Ok(Self::handle_initializing_task(
                    context.clone(),
                    &mut state_machine,
                    task_info,
                    step_enqueuer,
                )
                .await?)
            }

            TaskState::EvaluatingResults => Ok(Self::handle_evaluating_task(
                context.clone(),
                &mut state_machine,
                task_info,
                step_enqueuer,
            )
            .await?),

            TaskState::WaitingForDependencies => {
                // Dependencies became ready
                if state_machine
                    .transition(TaskEvent::DependenciesReady)
                    .await?
                {
                    Ok(Self::handle_evaluating_task(
                        context.clone(),
                        &mut state_machine,
                        task_info,
                        step_enqueuer,
                    )
                    .await?)
                } else {
                    Err(TaskerError::StateTransitionError(
                        "Task unable to transition to dependencies ready state".to_string(),
                    ))
                }
            }

            TaskState::WaitingForRetry => {
                // Retry timeout expired
                if state_machine.transition(TaskEvent::RetryReady).await? {
                    Ok(Some(
                        Self::handle_enqueueing_task(
                            context.clone(),
                            &mut state_machine,
                            task_info,
                            step_enqueuer,
                        )
                        .await?,
                    ))
                } else {
                    Err(TaskerError::StateTransitionError(
                        "Task unable to transition to retry state".to_string(),
                    ))
                }
            }

            _ => {
                warn!(
                    task_uuid = %task_info.task_uuid,
                    state = ?current_state,
                    "Task in unexpected state for batch processing"
                );
                Ok(None)
            }
        }
    }

    /// Handle task in Initializing state
    async fn handle_initializing_task(
        context: Arc<SystemContext>,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        if task_info.ready_steps_count > 0 {
            // Has ready steps to enqueue
            if state_machine
                .transition(TaskEvent::ReadyStepsFound(
                    task_info.ready_steps_count as u32,
                ))
                .await?
            {
                Ok(Some(
                    Self::handle_enqueueing_task(context, state_machine, task_info, step_enqueuer)
                        .await?,
                ))
            } else {
                Err(TaskerError::StateTransitionError(format!(
                    "Failed to transition to ready steps found for task {}",
                    task_info.task_uuid
                )))
            }
        } else {
            // No steps - task is complete
            match state_machine.transition(TaskEvent::NoStepsFound).await {
                Ok(_) => Ok(None),
                Err(err) => Err(err.into()),
            }
        }
    }

    /// Handle task in EnqueuingSteps state
    async fn handle_enqueueing_task(
        _context: Arc<SystemContext>,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<StepEnqueueResult> {
        // Enqueue the ready steps
        let enqueue_result = step_enqueuer.enqueue_ready_steps(task_info).await?;

        // Transition to StepsInProcess
        let transition_result = state_machine
            .transition(TaskEvent::StepsEnqueued(enqueue_result.step_uuids.clone()))
            .await?;

        if transition_result {
            Ok(enqueue_result)
        } else {
            Err(TaskerError::StateTransitionError(format!(
                "Could not transition the task to steps enqueued for task {}",
                task_info.task_uuid
            )))
        }
    }

    /// Handle task in EvaluatingResults state
    async fn handle_evaluating_task(
        system_context: Arc<SystemContext>,
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        // Get execution context to determine next action
        let sql_executor = SqlFunctionExecutor::new(system_context.database_pool().clone());
        let context_opt = sql_executor
            .get_task_execution_context(task_info.task_uuid)
            .await?;

        let Some(context) = context_opt else {
            error!(task_uuid = %task_info.task_uuid, "No execution context found");
            return Err(TaskerError::DatabaseError(format!(
                "No execution context found for task {}",
                task_info.task_uuid
            )));
        };

        match context.execution_status {
            ExecutionStatus::HasReadySteps => {
                if state_machine
                    .transition(TaskEvent::ReadyStepsFound(context.ready_steps as u32))
                    .await?
                {
                    Ok(Some(
                        Self::handle_enqueueing_task(
                            system_context,
                            state_machine,
                            task_info,
                            step_enqueuer,
                        )
                        .await?,
                    ))
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to ready steps found for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::AllComplete => {
                if state_machine
                    .transition(TaskEvent::AllStepsSuccessful)
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to all steps successful for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::BlockedByFailures => {
                if state_machine
                    .transition(TaskEvent::PermanentFailure("Too many step failures".into()))
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to permanent failure for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::WaitingForDependencies => {
                if state_machine
                    .transition(TaskEvent::NoDependenciesReady)
                    .await?
                {
                    Ok(None)
                } else {
                    Err(TaskerError::StateTransitionError(format!(
                        "Unable to transition to no dependencies ready for task {}",
                        task_info.task_uuid
                    )))
                }
            }
            ExecutionStatus::Processing => {
                // Task is still processing, nothing to do
                Ok(None)
            }
        }
    }

    /// Create result for empty cycle (no tasks claimed)
    fn create_empty_cycle_result(
        &self,
        cycle_started_at: DateTime<Utc>,
        cycle_duration_ms: u64,
    ) -> StepEnqueuerServiceResult {
        StepEnqueuerServiceResult {
            cycle_started_at,
            cycle_duration_ms,
            tasks_processed: 0,
            tasks_failed: 0,
            priority_distribution: PriorityDistribution::default(),
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: cycle_duration_ms,
                discovery_duration_ms: 0,
                enqueueing_duration_ms: 0,
                release_duration_ms: 0,
                avg_task_processing_ms: 0,
                steps_per_second: 0.0,
                tasks_per_second: 0.0,
            },
            warnings: vec![],
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &TaskClaimStepEnqueuerConfig {
        &self.config
    }

    /// Get processor ID (compatibility method)
    pub fn processor_uuid(&self) -> String {
        self.context.processor_uuid.to_string()
    }
}

impl Default for ContinuousOrchestrationSummary {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuousOrchestrationSummary {
    /// Create a new continuous orchestration summary
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            ended_at: None,
            total_cycles: 0,
            failed_cycles: 0,
            total_tasks_processed: 0,
            total_tasks_failed: 0,
            total_steps_enqueued: 0,
            total_steps_failed: 0,
            aggregate_priority_distribution: PriorityDistribution::default(),
            aggregate_performance_metrics: AggregatePerformanceMetrics::default(),
            top_namespaces: Vec::new(),
            total_warnings: 0,
            recent_warnings: Vec::new(),
        }
    }

    /// Accumulate results from a single cycle without storing the full result
    pub fn accumulate_cycle_result(&mut self, result: &StepEnqueuerServiceResult) {
        self.total_cycles += 1;
        self.total_tasks_processed += result.tasks_processed as u64;
        self.total_tasks_failed += result.tasks_failed as u64;

        // Aggregate priority distribution
        self.aggregate_priority_distribution.urgent_tasks +=
            result.priority_distribution.urgent_tasks;
        self.aggregate_priority_distribution.high_tasks += result.priority_distribution.high_tasks;
        self.aggregate_priority_distribution.normal_tasks +=
            result.priority_distribution.normal_tasks;
        self.aggregate_priority_distribution.low_tasks += result.priority_distribution.low_tasks;
        self.aggregate_priority_distribution.invalid_tasks +=
            result.priority_distribution.invalid_tasks;
        self.aggregate_priority_distribution.escalated_tasks +=
            result.priority_distribution.escalated_tasks;

        // Aggregate performance metrics
        self.aggregate_performance_metrics.total_cycle_duration_ms += result.cycle_duration_ms;
        self.aggregate_performance_metrics.total_claim_duration_ms +=
            result.performance_metrics.claim_duration_ms;
        self.aggregate_performance_metrics
            .total_discovery_duration_ms += result.performance_metrics.discovery_duration_ms;
        self.aggregate_performance_metrics
            .total_enqueueing_duration_ms += result.performance_metrics.enqueueing_duration_ms;
        self.aggregate_performance_metrics.total_release_duration_ms +=
            result.performance_metrics.release_duration_ms;

        // Track peak performance
        if result.performance_metrics.steps_per_second
            > self.aggregate_performance_metrics.peak_steps_per_second
        {
            self.aggregate_performance_metrics.peak_steps_per_second =
                result.performance_metrics.steps_per_second;
        }
        if result.performance_metrics.tasks_per_second
            > self.aggregate_performance_metrics.peak_tasks_per_second
        {
            self.aggregate_performance_metrics.peak_tasks_per_second =
                result.performance_metrics.tasks_per_second;
        }

        // Aggregate namespace statistics (keep top 10)
        for (namespace, stats) in &result.namespace_stats {
            if let Some(existing) = self
                .top_namespaces
                .iter_mut()
                .find(|(ns, _)| ns == namespace)
            {
                existing.1 += stats.steps_enqueued as u64;
            } else {
                self.top_namespaces
                    .push((namespace.clone(), stats.steps_enqueued as u64));
            }
        }
        // Keep only top 10 namespaces by activity
        self.top_namespaces.sort_by(|a, b| b.1.cmp(&a.1));
        self.top_namespaces.truncate(10);

        // Aggregate warnings (keep recent 50)
        self.total_warnings += result.warnings.len() as u64;
        for warning in &result.warnings {
            self.recent_warnings.push(warning.clone());
        }
        if self.recent_warnings.len() > 50 {
            self.recent_warnings
                .drain(0..self.recent_warnings.len() - 50);
        }
    }

    /// Increment error count for failed cycles
    pub fn increment_error_count(&mut self) {
        self.failed_cycles += 1;
    }

    /// Finalize the summary when continuous orchestration stops
    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());

        // Calculate final averages
        if self.total_cycles > 0 {
            let total_tasks = (self.aggregate_priority_distribution.urgent_tasks
                + self.aggregate_priority_distribution.high_tasks
                + self.aggregate_priority_distribution.normal_tasks
                + self.aggregate_priority_distribution.low_tasks
                + self.aggregate_priority_distribution.invalid_tasks)
                as f64;

            if total_tasks > 0.0 {
                self.aggregate_priority_distribution.avg_computed_priority =
                    (self.aggregate_priority_distribution.urgent_tasks as f64 * 4.0
                        + self.aggregate_priority_distribution.high_tasks as f64 * 3.0
                        + self.aggregate_priority_distribution.normal_tasks as f64 * 2.0
                        + self.aggregate_priority_distribution.low_tasks as f64 * 1.0)
                        / total_tasks;
            }

            // Calculate average performance metrics
            let total_duration_seconds =
                self.aggregate_performance_metrics.total_cycle_duration_ms as f64 / 1000.0;
            if total_duration_seconds > 0.0 {
                self.aggregate_performance_metrics.avg_steps_per_second =
                    self.total_steps_enqueued as f64 / total_duration_seconds;
                self.aggregate_performance_metrics.avg_tasks_per_second =
                    self.total_tasks_processed as f64 / total_duration_seconds;
            }
        }
    }

    /// Get average cycle duration in milliseconds
    pub fn avg_cycle_duration_ms(&self) -> f64 {
        if self.total_cycles > 0 {
            self.aggregate_performance_metrics.total_cycle_duration_ms as f64
                / self.total_cycles as f64
        } else {
            0.0
        }
    }

    /// Get success rate percentage
    pub fn success_rate_percentage(&self) -> f64 {
        if self.total_cycles > 0 {
            ((self.total_cycles - self.failed_cycles) as f64 / self.total_cycles as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get runtime duration
    pub fn runtime_duration(&self) -> chrono::Duration {
        let end_time = self.ended_at.unwrap_or_else(Utc::now);
        end_time - self.started_at
    }
}

impl Default for AggregatePerformanceMetrics {
    fn default() -> Self {
        Self {
            total_cycle_duration_ms: 0,
            total_claim_duration_ms: 0,
            total_discovery_duration_ms: 0,
            total_enqueueing_duration_ms: 0,
            total_release_duration_ms: 0,
            peak_steps_per_second: 0.0,
            peak_tasks_per_second: 0.0,
            avg_steps_per_second: 0.0,
            avg_tasks_per_second: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_loop_config_defaults() {
        let config = TaskClaimStepEnqueuerConfig::default();
        assert_eq!(config.batch_size, 5);
        assert!(config.namespace_filter.is_none());
        assert!(!config.enable_performance_logging);
        assert!(config.enable_heartbeat);
    }

    #[test]
    fn test_priority_distribution_calculation() {
        let distribution = PriorityDistribution {
            urgent_tasks: 2,
            high_tasks: 1,
            normal_tasks: 3,
            escalated_tasks: 1,
            ..Default::default()
        };

        assert_eq!(distribution.urgent_tasks, 2);
        assert_eq!(distribution.escalated_tasks, 1);
    }
}
