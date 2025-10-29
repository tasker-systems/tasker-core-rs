//! Batch Processor
//!
//! Handles batch processing of ready tasks.

use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

use super::task_processor::TaskProcessor;
use super::types::*;
use crate::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
use tasker_shared::config::orchestration::TaskClaimStepEnqueuerConfig;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::{SystemContext, TaskerResult};

/// Processes batches of ready tasks
#[derive(Clone, Debug)]
pub struct BatchProcessor {
    task_processor: TaskProcessor,
    context: Arc<SystemContext>,
    config: TaskClaimStepEnqueuerConfig,
}

impl BatchProcessor {
    pub fn new(
        context: Arc<SystemContext>,
        step_enqueuer: Arc<StepEnqueuer>,
        config: TaskClaimStepEnqueuerConfig,
    ) -> Self {
        let task_processor = TaskProcessor::new(context.clone(), step_enqueuer);
        Self {
            task_processor,
            context,
            config,
        }
    }

    /// Run a single orchestration cycle using state machine approach
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
                task.task_uuid,
                task.namespace_name,
                task.ready_steps_count,
                task.priority,
                task.current_state
            );
        }

        // Process tasks concurrently
        let mut task_futures = Vec::new();

        for task_info in ready_tasks {
            let processor = self.task_processor.clone();

            let future = tokio::spawn(async move {
                processor
                    .process_from_ready_info(&task_info)
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
            priority_distribution: PriorityDistribution::default(),
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: 0,
                release_duration_ms: 0,
                discovery_duration_ms: cycle_duration_ms / 3,
                enqueueing_duration_ms: cycle_duration_ms / 3,
                avg_task_processing_ms: if processed > 0 {
                    cycle_duration_ms / processed as u64
                } else {
                    0
                },
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

    /// Create result for empty cycle (no tasks claimed)
    fn create_empty_cycle_result(
        &self,
        cycle_started_at: chrono::DateTime<Utc>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_batch_processor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let processor = BatchProcessor::new(context, step_enqueuer, config);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&processor.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_batch_processor_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let processor = BatchProcessor::new(context, step_enqueuer, config);

        let cloned = processor.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&processor.context),
            Arc::as_ptr(&cloned.context)
        );
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_empty_cycle_result(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let processor = BatchProcessor::new(context, step_enqueuer, config);

        let cycle_started_at = Utc::now();
        let cycle_duration_ms = 100;

        let result = processor.create_empty_cycle_result(cycle_started_at, cycle_duration_ms);

        // Verify empty cycle result structure
        assert_eq!(result.tasks_processed, 0);
        assert_eq!(result.tasks_failed, 0);
        assert_eq!(result.cycle_duration_ms, cycle_duration_ms);
        assert_eq!(result.performance_metrics.steps_per_second, 0.0);
        assert_eq!(result.warnings.len(), 0);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_batch_processor_config_storage(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let expected_batch_size = config.batch_size;
        let processor = BatchProcessor::new(context, step_enqueuer, config);

        // Verify config is stored correctly
        assert_eq!(processor.config.batch_size, expected_batch_size);
        Ok(())
    }
}
