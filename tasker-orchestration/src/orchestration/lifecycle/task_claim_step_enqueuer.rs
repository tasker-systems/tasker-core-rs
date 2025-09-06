//! # Orchestration Loop
//!
//! ## Architecture: Main Orchestration Coordinator
//!
//! The OrchestrationLoop is the main coordinator that manages the complete orchestration cycle:
//! **Claim** → **Discover** → **Enqueue** → **Release**. This replaces the batch-based TCP
//! approach with a queue-driven, priority-fair, individual step processing system.
//!
//! ## Key Features
//!
//! - **Priority-Fair Task Claiming**: Uses `tasker_ready_tasks` view with computed priority
//! - **Individual Step Processing**: Enqueues steps individually, not in batches
//! - **Autonomous Workers**: Steps processed by independent Ruby queue workers
//! - **Distributed Safety**: Multiple orchestrators can run safely with task claiming
//! - **Comprehensive Monitoring**: Rich metrics and observability throughout the cycle
//!
//! ## Orchestration Cycle
//!
//! 1. **Claim**: Atomically claim ready tasks using priority fairness
//! 2. **Discover**: Find viable steps using existing SQL-based logic
//! 3. **Enqueue**: Send individual step messages to namespace queues
//! 4. **Release**: Release task claims to make tasks available again
//! 5. **Monitor**: Track performance, priority distribution, and system health
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::orchestration_loop::OrchestrationLoop;
//! use tasker_shared::config::orchestration::OrchestrationLoopConfig;
//! use tasker_shared::config::TaskerConfig;
//! use tasker_shared::messaging::{PgmqClient, UnifiedPgmqClient};
//! use sqlx::PgPool;
//! use std::sync::Arc;
//!
//! # async fn example(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let database_url = "postgresql://localhost/tasker";
//! let pgmq_client = PgmqClient::new(database_url).await
//!     .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error>)?;
//! let unified_client = Arc::new(UnifiedPgmqClient::Standard(pgmq_client));
//! let orchestrator_id = "orchestrator-host123-uuid".to_string();
//! let config = OrchestrationLoopConfig::default();
//! let tasker_config = TaskerConfig::default();
//!
//! let orchestration_loop = OrchestrationLoop::with_unified_client(
//!     pool,
//!     unified_client,
//!     orchestrator_id,
//!     config,
//!     tasker_config,
//! ).await?;
//!
//! // Run a single orchestration cycle
//! let result = orchestration_loop.run_cycle().await?;
//! println!("Processed {} tasks, enqueued {} steps",
//!          result.tasks_processed, result.total_steps_enqueued);
//!
//! // Run continuous orchestration
//! orchestration_loop.run_continuous().await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::{
    lifecycle::step_enqueuer::{StepEnqueueResult, StepEnqueuer},
    task_claim::task_claimer::{ClaimedTask, TaskClaimer},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::config::orchestration::TaskClaimStepEnqueuerConfig;
use tasker_shared::{SystemContext, TaskerResult};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Result of a single orchestration cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimStepEnqueueCycleResult {
    /// Cycle execution timestamp
    pub cycle_started_at: DateTime<Utc>,
    /// Total time for the complete cycle
    pub cycle_duration_ms: u64,
    /// Number of tasks that were claimed
    pub tasks_claimed: usize,
    /// Number of tasks successfully processed
    pub tasks_processed: usize,
    /// Number of tasks that failed processing
    pub tasks_failed: usize,
    /// Total steps discovered across all tasks
    pub total_steps_discovered: usize,
    /// Total steps successfully enqueued
    pub total_steps_enqueued: usize,
    /// Total steps that failed to enqueue
    pub total_steps_failed: usize,
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
    /// Orchestrator ID
    pub orchestrator_id: String,
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
pub struct TaskClaimStepEnqueuer {
    task_claimer: TaskClaimer,
    step_enqueuer: StepEnqueuer,
    orchestrator_id: String,
    config: TaskClaimStepEnqueuerConfig,
}

impl TaskClaimStepEnqueuer {
    /// Create a new orchestration loop
    pub async fn new(context: Arc<SystemContext>, orchestrator_id: String) -> TaskerResult<Self> {
        let config =
            TaskClaimStepEnqueuerConfig::from_tasker_config(context.config_manager.config());
        Self::with_config(context, orchestrator_id, config).await
    }

    /// Create a new orchestration loop with custom configuration
    pub async fn with_config(
        context: Arc<SystemContext>,
        orchestrator_id: String,
        config: TaskClaimStepEnqueuerConfig,
    ) -> TaskerResult<Self> {
        let task_claimer = TaskClaimer::with_config(
            context.database_pool().clone(),
            orchestrator_id.clone(),
            config.task_claimer_config.clone(),
        );

        let step_enqueuer = StepEnqueuer::new(context.clone()).await?;

        Ok(Self {
            task_claimer,
            step_enqueuer,
            orchestrator_id,
            config,
        })
    }

    /// Run a single orchestration cycle: Claim → Discover → Enqueue → Release
    #[instrument(skip(self), fields(orchestrator_id = %self.orchestrator_id))]
    pub async fn process_batch(&self) -> TaskerResult<TaskClaimStepEnqueueCycleResult> {
        let cycle_start = Instant::now();
        let cycle_started_at = Utc::now();

        debug!(
            "🔄 ORCHESTRATION_LOOP: Starting cycle - orchestrator_id: {}, tasks_per_cycle: {}, namespace_filter: {:?}",
            self.orchestrator_id, self.config.max_batch_size, self.config.namespace_filter
        );

        // PHASE 1: Claim ready tasks with priority fairness
        let claim_start = Instant::now();
        debug!(
            "🎯 ORCHESTRATION_LOOP: Attempting to claim {} ready tasks",
            self.config.max_batch_size
        );

        let claimed_tasks = self
            .task_claimer
            .claim_ready_tasks(
                self.config.max_batch_size,
                self.config.namespace_filter.as_deref(),
            )
            .await?;
        let claim_duration_ms = claim_start.elapsed().as_millis() as u64;

        debug!(
            "📊 ORCHESTRATION_LOOP: Task claiming completed - claimed {} tasks in {}ms",
            claimed_tasks.len(),
            claim_duration_ms
        );

        if claimed_tasks.is_empty() {
            debug!("⏸️ ORCHESTRATION_LOOP: No tasks available for claiming in this cycle - orchestration cycle ending early");
            return Ok(self.create_empty_cycle_result(
                cycle_started_at,
                cycle_start.elapsed().as_millis() as u64,
            ));
        }

        // Log details about claimed tasks
        for task in &claimed_tasks {
            debug!(
                "📋 ORCHESTRATION_LOOP: Claimed task {} (namespace: '{}') with {} ready steps, priority: {}",
                task.task_uuid, task.namespace_name, task.ready_steps_count, task.priority
            );
        }

        let tasks_claimed = claimed_tasks.len();
        debug!(tasks_claimed = tasks_claimed, "Successfully claimed tasks");

        // PHASE 2 & 3: Discover and enqueue steps for each claimed task, then release immediately
        let discovery_enqueueing_start = Instant::now();
        let mut tasks_processed = 0;
        let mut tasks_failed = 0;
        let mut total_steps_discovered = 0;
        let mut total_steps_enqueued = 0;
        let mut total_steps_failed = 0;
        let mut namespace_stats: HashMap<String, NamespaceStats> = HashMap::new();
        let mut warnings = Vec::new();
        let mut total_release_duration_ms = 0u64;

        for claimed_task in &claimed_tasks {
            match self.process_claimed_task(claimed_task).await {
                Ok(enqueue_result) => {
                    tasks_processed += 1;
                    total_steps_discovered += enqueue_result.steps_discovered;
                    total_steps_enqueued += enqueue_result.steps_enqueued;
                    total_steps_failed += enqueue_result.steps_failed;

                    // Aggregate namespace stats
                    for (namespace, stats) in enqueue_result.namespace_breakdown {
                        let namespace_stat = namespace_stats
                            .entry(namespace.clone())
                            .or_insert_with(|| NamespaceStats {
                                tasks_processed: 0,
                                steps_enqueued: 0,
                                steps_failed: 0,
                                queue_name: stats.queue_name.clone(),
                                avg_processing_time_ms: 0,
                            });

                        namespace_stat.tasks_processed += 1;
                        namespace_stat.steps_enqueued += stats.steps_enqueued;
                        namespace_stat.steps_failed += stats.steps_failed;
                        namespace_stat.avg_processing_time_ms +=
                            enqueue_result.processing_duration_ms;
                    }

                    warnings.extend(enqueue_result.warnings);
                }
                Err(e) => {
                    tasks_failed += 1;
                    let warning =
                        format!("Failed to process task {}: {}", claimed_task.task_uuid, e);
                    warnings.push(warning.clone());

                    warn!(
                        task_uuid = claimed_task.task_uuid.to_string(),
                        error = %e,
                        "Failed to process claimed task"
                    );
                }
            }

            // IMMEDIATELY RELEASE THE CLAIM after processing each task
            // This makes the task available sooner for other orchestrators
            let release_start = Instant::now();
            match self
                .task_claimer
                .release_task_claim(claimed_task.task_uuid)
                .await
            {
                Ok(released) => {
                    if !released {
                        let warning = format!(
                            "Task claim {} was not released (may have been released already)",
                            claimed_task.task_uuid
                        );
                        warnings.push(warning);
                    } else {
                        debug!(
                            task_uuid = claimed_task.task_uuid.to_string(),
                            "Released task claim immediately after processing"
                        );
                    }
                }
                Err(e) => {
                    let warning = format!(
                        "Failed to release task claim {}: {}",
                        claimed_task.task_uuid, e
                    );
                    warnings.push(warning);
                }
            }
            total_release_duration_ms += release_start.elapsed().as_millis() as u64;
        }

        let discovery_enqueueing_duration = discovery_enqueueing_start.elapsed().as_millis() as u64;

        // Calculate final averages for namespace stats
        for stats in namespace_stats.values_mut() {
            if stats.tasks_processed > 0 {
                stats.avg_processing_time_ms /= stats.tasks_processed as u64;
            }
        }

        let cycle_duration_ms = cycle_start.elapsed().as_millis() as u64;

        let result = TaskClaimStepEnqueueCycleResult {
            cycle_started_at,
            cycle_duration_ms,
            tasks_claimed,
            tasks_processed,
            tasks_failed,
            total_steps_discovered,
            total_steps_enqueued,
            total_steps_failed,
            priority_distribution: self.calculate_priority_distribution(&claimed_tasks),
            namespace_stats,
            performance_metrics: PerformanceMetrics {
                claim_duration_ms,
                discovery_duration_ms: (discovery_enqueueing_duration - total_release_duration_ms)
                    / 2, // Rough split
                enqueueing_duration_ms: (discovery_enqueueing_duration - total_release_duration_ms)
                    / 2,
                release_duration_ms: total_release_duration_ms,
                avg_task_processing_ms: if tasks_processed > 0 {
                    discovery_enqueueing_duration / tasks_processed as u64
                } else {
                    0
                },
                steps_per_second: if cycle_duration_ms > 0 {
                    (total_steps_enqueued as f64) / (cycle_duration_ms as f64 / 1000.0)
                } else {
                    0.0
                },
                tasks_per_second: if cycle_duration_ms > 0 {
                    (tasks_processed as f64) / (cycle_duration_ms as f64 / 1000.0)
                } else {
                    0.0
                },
            },
            warnings,
        };

        info!(
            cycle_duration_ms = cycle_duration_ms,
            tasks_processed = tasks_processed,
            tasks_failed = tasks_failed,
            total_steps_enqueued = total_steps_enqueued,
            "Completed orchestration cycle"
        );

        if self.config.enable_performance_logging {
            self.log_performance_details(&result);
        }

        Ok(result)
    }

    /// Process a single claimed task: discover steps and enqueue them
    async fn process_claimed_task(
        &self,
        claimed_task: &ClaimedTask,
    ) -> TaskerResult<StepEnqueueResult> {
        debug!(
            task_uuid = claimed_task.task_uuid.to_string(),
            "Processing claimed task for step discovery and enqueueing"
        );

        self.step_enqueuer.enqueue_ready_steps(claimed_task).await
    }

    /// Process a single task by UUID: claim it, enqueue steps, and release
    ///
    /// This is used for immediate step enqueuing after task creation (TAS-41).
    /// Returns None if the task couldn't be claimed (no ready steps, already claimed, etc.)
    pub async fn process_single_task(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        debug!(
            task_uuid = task_uuid.to_string(),
            "Processing single task for immediate step enqueuing"
        );

        // Claim the individual task
        let claimed_task = match self.task_claimer.claim_individual_task(task_uuid).await? {
            Some(task) => task,
            None => {
                debug!(
                    task_uuid = task_uuid.to_string(),
                    "Task not ready for processing (no ready steps or already claimed)"
                );
                return Ok(None);
            }
        };

        // Process the claimed task to enqueue steps
        let enqueue_result = match self.process_claimed_task(&claimed_task).await {
            Ok(result) => result,
            Err(e) => {
                error!(
                    task_uuid = task_uuid.to_string(),
                    error = %e,
                    "Failed to process claimed task"
                );

                // Always try to release the claim on error
                let _ = self.task_claimer.release_task_claim(task_uuid).await;
                return Err(e);
            }
        };

        // Release the claim immediately after processing
        match self.task_claimer.release_task_claim(task_uuid).await {
            Ok(released) => {
                if !released {
                    warn!(
                        task_uuid = task_uuid.to_string(),
                        "Task claim was not released (may have been released already)"
                    );
                }
            }
            Err(e) => {
                warn!(
                    task_uuid = task_uuid.to_string(),
                    error = %e,
                    "Failed to release task claim after processing"
                );
            }
        }

        info!(
            task_uuid = task_uuid.to_string(),
            steps_enqueued = enqueue_result.steps_enqueued,
            steps_failed = enqueue_result.steps_failed,
            "Successfully processed single task with immediate step enqueuing"
        );

        Ok(Some(enqueue_result))
    }

    /// Calculate priority distribution for monitoring
    fn calculate_priority_distribution(
        &self,
        claimed_tasks: &[ClaimedTask],
    ) -> PriorityDistribution {
        let mut distribution = PriorityDistribution::default();
        let mut total_computed_priority = 0.0;
        let mut total_age_hours = 0.0;

        for task in claimed_tasks {
            match task.priority {
                4 => distribution.urgent_tasks += 1,
                3 => distribution.high_tasks += 1,
                2 => distribution.normal_tasks += 1,
                1 => distribution.low_tasks += 1,
                _ => distribution.invalid_tasks += 1,
            }

            if task.computed_priority > task.priority as f64 {
                distribution.escalated_tasks += 1;
            }

            total_computed_priority += task.computed_priority;
            total_age_hours += task.age_hours;
        }

        let task_count = claimed_tasks.len();
        if task_count > 0 {
            distribution.avg_computed_priority = total_computed_priority / task_count as f64;
            distribution.avg_task_age_hours = total_age_hours / task_count as f64;
        }

        distribution
    }

    /// Create result for empty cycle (no tasks claimed)
    fn create_empty_cycle_result(
        &self,
        cycle_started_at: DateTime<Utc>,
        cycle_duration_ms: u64,
    ) -> TaskClaimStepEnqueueCycleResult {
        TaskClaimStepEnqueueCycleResult {
            cycle_started_at,
            cycle_duration_ms,
            tasks_claimed: 0,
            tasks_processed: 0,
            tasks_failed: 0,
            total_steps_discovered: 0,
            total_steps_enqueued: 0,
            total_steps_failed: 0,
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

    /// Log detailed performance information
    fn log_performance_details(&self, result: &TaskClaimStepEnqueueCycleResult) {
        debug!(
            orchestrator_id = %self.orchestrator_id,
            cycle_duration_ms = result.cycle_duration_ms,
            claim_duration_ms = result.performance_metrics.claim_duration_ms,
            discovery_duration_ms = result.performance_metrics.discovery_duration_ms,
            enqueueing_duration_ms = result.performance_metrics.enqueueing_duration_ms,
            release_duration_ms = result.performance_metrics.release_duration_ms,
            steps_per_second = result.performance_metrics.steps_per_second,
            tasks_per_second = result.performance_metrics.tasks_per_second,
            "Detailed orchestration performance metrics"
        );

        debug!(
            urgent_tasks = result.priority_distribution.urgent_tasks,
            high_tasks = result.priority_distribution.high_tasks,
            normal_tasks = result.priority_distribution.normal_tasks,
            low_tasks = result.priority_distribution.low_tasks,
            escalated_tasks = result.priority_distribution.escalated_tasks,
            avg_computed_priority = result.priority_distribution.avg_computed_priority,
            avg_task_age_hours = result.priority_distribution.avg_task_age_hours,
            "Priority distribution analysis"
        );
    }

    /// Get current configuration
    pub fn config(&self) -> &TaskClaimStepEnqueuerConfig {
        &self.config
    }

    /// Get orchestrator ID
    pub fn orchestrator_id(&self) -> &str {
        &self.orchestrator_id
    }
}

impl ContinuousOrchestrationSummary {
    /// Create a new continuous orchestration summary
    pub fn new(orchestrator_id: String) -> Self {
        Self {
            orchestrator_id,
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
    pub fn accumulate_cycle_result(&mut self, result: &TaskClaimStepEnqueueCycleResult) {
        self.total_cycles += 1;
        self.total_tasks_processed += result.tasks_processed as u64;
        self.total_tasks_failed += result.tasks_failed as u64;
        self.total_steps_enqueued += result.total_steps_enqueued as u64;
        self.total_steps_failed += result.total_steps_failed as u64;

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
    use std::time::Duration;

    #[test]
    fn test_orchestration_loop_config_defaults() {
        let config = TaskClaimStepEnqueuerConfig::default();
        assert_eq!(config.max_batch_size, 5);
        assert_eq!(config.cycle_interval, Duration::from_secs(1));
        assert!(config.namespace_filter.is_none());
        assert!(config.max_cycles.is_none());
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
