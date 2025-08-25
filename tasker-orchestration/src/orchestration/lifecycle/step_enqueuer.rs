//! # Step Enqueuer
//!
//! ## Architecture: Individual Step Enqueueing with Dependency Resolution
//!
//! The StepEnqueuer component combines ViableStepDiscovery with pgmq to enqueue individual ready
//! steps to namespace-specific queues. This replaces the batch-based approach with individual
//! step processing, providing better fault isolation and more granular control.
//!
//! ## Key Features
//!
//! - **Individual Step Processing**: Enqueues steps one by one, not in batches
//! - **Dependency Resolution**: Uses existing ViableStepDiscovery for proven readiness logic
//! - **Namespace Routing**: Routes steps to appropriate `{namespace}_queue` queues
//! - **Metadata Flow**: Includes task, sequence, and step context for handlers
//! - **Error Resilience**: Continues processing other steps if one fails to enqueue
//!
//! ## Integration
//!
//! Works with:
//! - `ViableStepDiscovery`: For step readiness analysis
//! - `PgmqClient`: For queue operations
//! - `TaskClaimer`: Processes claimed tasks
//! - Queue workers: Autonomous Ruby workers process enqueued step messages
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::{step_enqueuer::StepEnqueuer, task_claimer::ClaimedTask};
//! use tasker_shared::messaging::{PgmqClient, UnifiedPgmqClient};
//! use uuid::Uuid;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! # let database_url = "postgresql://localhost/test";
//! # let pgmq_client = PgmqClient::new(database_url).await
//! #     .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error>)?;
//! # let unified_client = UnifiedPgmqClient::Standard(pgmq_client);
//! let enqueuer = StepEnqueuer::with_unified_client(pool, unified_client).await?;
//!
//! // Enqueue ready steps for a claimed task
//! let claimed_task = ClaimedTask {
//!     task_uuid: Uuid::now_v7(),
//!     namespace_name: "fulfillment".to_string(),
//!     // ... other fields
//! #   priority: 2, computed_priority: 4.0, age_hours: 0.1,
//! #   ready_steps_count: 2, claim_timeout_seconds: 300,
//! #   claimed_at: chrono::Utc::now(),
//! };
//!
//! let result = enqueuer.enqueue_ready_steps(&claimed_task).await?;
//! println!("Enqueued {} steps for task {}", result.steps_enqueued, claimed_task.task_uuid);
//! # Ok(())
//! # }
//! ```

use crate::orchestration::{
    state_manager::StateManager, task_claim::task_claimer::ClaimedTask,
    viable_step_discovery::ViableStepDiscovery,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Instant;
use tasker_shared::config::orchestration::StepEnqueuerConfig;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::events::EventPublisher;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::{PgmqClient, PgmqClientTrait, UnifiedPgmqClient};
use tasker_shared::types::ViableStep;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Result of step enqueueing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueueResult {
    /// Task ID that was processed
    pub task_uuid: Uuid,
    /// Total number of steps that were ready for enqueueing
    pub steps_discovered: usize,
    /// Number of steps successfully enqueued to pgmq
    pub steps_enqueued: usize,
    /// Number of steps that failed to enqueue
    pub steps_failed: usize,
    /// Time taken for the entire operation
    pub processing_duration_ms: u64,
    /// Breakdown by namespace for monitoring
    pub namespace_breakdown: HashMap<String, NamespaceEnqueueStats>,
    /// Any non-fatal errors encountered
    pub warnings: Vec<String>,
}

/// Per-namespace enqueueing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceEnqueueStats {
    /// Steps enqueued to this namespace queue
    pub steps_enqueued: usize,
    /// Steps that failed to enqueue
    pub steps_failed: usize,
    /// Queue name used
    pub queue_name: String,
}

/// Step enqueueing component for individual step processing
pub struct StepEnqueuer {
    viable_step_discovery: ViableStepDiscovery,
    pgmq_client: UnifiedPgmqClient,
    pool: PgPool,
    config: StepEnqueuerConfig,
    state_manager: StateManager,
}

impl StepEnqueuer {
    /// Create a new step enqueuer instance (backward compatibility with standard client)
    pub async fn new(pool: PgPool, pgmq_client: PgmqClient) -> TaskerResult<Self> {
        let unified_client = UnifiedPgmqClient::Standard(pgmq_client);
        Self::with_unified_client(pool, unified_client).await
    }

    /// Create a new step enqueuer with unified client (supports circuit breakers)
    pub async fn with_unified_client(
        pool: PgPool,
        pgmq_client: UnifiedPgmqClient,
    ) -> TaskerResult<Self> {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let event_publisher = EventPublisher::new();
        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher, pool.clone());

        Ok(Self {
            viable_step_discovery,
            pgmq_client,
            pool,
            config: StepEnqueuerConfig::default(),
            state_manager,
        })
    }

    /// Create a new step enqueuer with custom configuration (backward compatibility)
    pub async fn with_config(
        pool: PgPool,
        pgmq_client: PgmqClient,
        config: StepEnqueuerConfig,
    ) -> TaskerResult<Self> {
        let unified_client = UnifiedPgmqClient::Standard(pgmq_client);
        Self::with_unified_client_and_config(pool, unified_client, config).await
    }

    /// Create a new step enqueuer with unified client and custom configuration
    pub async fn with_unified_client_and_config(
        pool: PgPool,
        pgmq_client: UnifiedPgmqClient,
        config: StepEnqueuerConfig,
    ) -> TaskerResult<Self> {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let event_publisher = EventPublisher::new();
        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher, pool.clone());

        Ok(Self {
            viable_step_discovery,
            pgmq_client,
            pool,
            config,
            state_manager,
        })
    }

    /// Enqueue all ready steps for a claimed task
    ///
    /// This discovers viable steps using the existing SQL-based logic, then enqueues each
    /// step individually to its namespace-specific queue for autonomous worker processing.
    #[instrument(skip(self), fields(task_uuid = claimed_task.task_uuid.to_string(), namespace = %claimed_task.namespace_name))]
    pub async fn enqueue_ready_steps(
        &self,
        claimed_task: &ClaimedTask,
    ) -> TaskerResult<StepEnqueueResult> {
        let start_time = Instant::now();

        info!(
            task_uuid = claimed_task.task_uuid.to_string(),
            namespace = %claimed_task.namespace_name,
            ready_steps_expected = claimed_task.ready_steps_count,
            "Starting step enqueueing for claimed task"
        );

        // 1. Discover viable steps using existing SQL-based logic
        let viable_steps = self
            .viable_step_discovery
            .find_viable_steps(claimed_task.task_uuid)
            .await
            .map_err(|e| {
                error!(
                    task_uuid = claimed_task.task_uuid.to_string(),
                    error = %e,
                    "Failed to discover viable steps"
                );
                TaskerError::OrchestrationError(format!("Step discovery failed: {e}"))
            })?;

        let steps_discovered = viable_steps.len();
        debug!(
            task_uuid = claimed_task.task_uuid.to_string(),
            steps_discovered = steps_discovered,
            "Discovered viable steps for enqueueing"
        );

        if steps_discovered == 0 {
            warn!(
                task_uuid = claimed_task.task_uuid.to_string(),
                "No viable steps found for claimed task - task may have been processed already"
            );

            return Ok(StepEnqueueResult {
                task_uuid: claimed_task.task_uuid,
                steps_discovered: 0,
                steps_enqueued: 0,
                steps_failed: 0,
                processing_duration_ms: start_time.elapsed().as_millis() as u64,
                namespace_breakdown: HashMap::new(),
                warnings: vec!["No viable steps found for enqueueing".to_string()],
            });
        }

        // 2. Enqueue each step individually to namespace queues
        let mut steps_enqueued = 0;
        let mut steps_failed = 0;
        let mut namespace_breakdown: HashMap<String, NamespaceEnqueueStats> = HashMap::new();
        let mut warnings = Vec::new();

        for viable_step in viable_steps {
            match self
                .enqueue_individual_step(claimed_task, &viable_step)
                .await
            {
                Ok(queue_name) => {
                    steps_enqueued += 1;

                    // Update namespace stats
                    let stats = namespace_breakdown
                        .entry(claimed_task.namespace_name.clone())
                        .or_insert_with(|| NamespaceEnqueueStats {
                            steps_enqueued: 0,
                            steps_failed: 0,
                            queue_name: queue_name.clone(),
                        });
                    stats.steps_enqueued += 1;

                    if self.config.enable_detailed_logging {
                        debug!(
                            task_uuid = claimed_task.task_uuid.to_string(),
                            step_uuid = viable_step.step_uuid.to_string(),
                            step_name = %viable_step.name,
                            queue_name = %queue_name,
                            "Successfully enqueued step"
                        );
                    }
                }
                Err(e) => {
                    steps_failed += 1;

                    // Update namespace stats
                    let queue_name = format!("{}_queue", claimed_task.namespace_name);
                    let stats = namespace_breakdown
                        .entry(claimed_task.namespace_name.clone())
                        .or_insert_with(|| NamespaceEnqueueStats {
                            steps_enqueued: 0,
                            steps_failed: 0,
                            queue_name: queue_name.clone(),
                        });
                    stats.steps_failed += 1;

                    let warning = format!(
                        "Failed to enqueue step {} ({}): {}",
                        viable_step.step_uuid, viable_step.name, e
                    );
                    warnings.push(warning.clone());

                    warn!(
                        task_uuid = claimed_task.task_uuid.to_string(),
                        step_uuid = viable_step.step_uuid.to_string(),
                        step_name = %viable_step.name,
                        error = %e,
                        "Failed to enqueue individual step"
                    );
                }
            }
        }

        let processing_duration_ms = start_time.elapsed().as_millis() as u64;

        // Transition task to "in_progress" state if we successfully enqueued any steps
        // Note: claiming only sets claimed_at/claimed_by but doesn't change execution state
        if steps_enqueued > 0 {
            if let Err(e) = self
                .state_manager
                .mark_task_in_progress(claimed_task.task_uuid)
                .await
            {
                error!(
                    task_uuid = claimed_task.task_uuid.to_string(),
                    error = %e,
                    "Failed to transition task to in_progress state after enqueueing steps"
                );
                // Don't fail the entire operation - steps were successfully enqueued
                warnings.push(format!(
                    "Warning: Failed to transition task {} to in_progress state: {}",
                    claimed_task.task_uuid, e
                ));
            } else {
                info!(
                    task_uuid = claimed_task.task_uuid.to_string(),
                    steps_enqueued = steps_enqueued,
                    "Successfully transitioned task to in_progress state after enqueueing steps"
                );
            }
        }

        let result = StepEnqueueResult {
            task_uuid: claimed_task.task_uuid,
            steps_discovered,
            steps_enqueued,
            steps_failed,
            processing_duration_ms,
            namespace_breakdown,
            warnings,
        };

        info!(
            task_uuid = claimed_task.task_uuid.to_string(),
            steps_discovered = steps_discovered,
            steps_enqueued = steps_enqueued,
            steps_failed = steps_failed,
            processing_duration_ms = processing_duration_ms,
            "Completed step enqueueing for claimed task"
        );

        Ok(result)
    }

    /// Enqueue a single viable step to its namespace queue
    async fn enqueue_individual_step(
        &self,
        claimed_task: &ClaimedTask,
        viable_step: &ViableStep,
    ) -> TaskerResult<String> {
        info!(
            "🚀 STEP_ENQUEUER: Preparing to enqueue step {} (name: '{}') from task {} (namespace: '{}')",
            viable_step.step_uuid, viable_step.name, claimed_task.task_uuid, claimed_task.namespace_name
        );

        // Create simple UUID-based message (simplified architecture)
        let simple_message = self
            .create_simple_step_message(claimed_task, viable_step)
            .await?;

        // Enqueue to namespace-specific queue
        let queue_name = format!("{}_queue", claimed_task.namespace_name);

        info!(
            "📝 STEP_ENQUEUER: Created simple step message - targeting queue '{}' for step UUID '{}'",
            queue_name, simple_message.step_uuid
        );

        let msg_id = self
            .pgmq_client
            .send_json_message(&queue_name, &simple_message)
            .await
            .map_err(|e| {
                error!(
                    "❌ STEP_ENQUEUER: Failed to enqueue step {} to queue '{}': {}",
                    viable_step.step_uuid, queue_name, e
                );
                TaskerError::OrchestrationError(format!(
                    "Failed to enqueue step {} to {}: {}",
                    viable_step.step_uuid, queue_name, e
                ))
            })?;

        info!(
            "✅ STEP_ENQUEUER: Successfully sent step {} to pgmq queue '{}' with message ID {}",
            viable_step.step_uuid, queue_name, msg_id
        );

        // TAS-32: Transition the step to "enqueued" state since it's now enqueued for processing
        // This replaces the old behavior of marking steps as in_progress when enqueued
        if let Err(e) = self
            .state_manager
            .mark_step_enqueued(viable_step.step_uuid)
            .await
        {
            error!(
                "❌ STEP_ENQUEUER: Failed to transition step {} to enqueued state after enqueueing: {}",
                viable_step.step_uuid, e
            );
            // Continue execution - enqueueing succeeded, state transition failure shouldn't block workflow
        } else {
            info!(
                "✅ STEP_ENQUEUER: Successfully marked step {} as enqueued (TAS-32)",
                viable_step.step_uuid
            );
        }

        info!(
            "🎯 STEP_ENQUEUER: Step enqueueing complete - step_uuid: {}, task_uuid: {}, namespace: '{}', queue: '{}', msg_id: {}",
            viable_step.step_uuid,
            claimed_task.task_uuid,
            claimed_task.namespace_name,
            queue_name,
            msg_id
        );

        Ok(queue_name)
    }

    /// Create a simple UUID-based step message (simplified architecture)
    ///
    /// This creates the new 3-field message format that leverages the shared database
    /// as the API layer, dramatically reducing message size and complexity.
    async fn create_simple_step_message(
        &self,
        claimed_task: &ClaimedTask,
        viable_step: &ViableStep,
    ) -> TaskerResult<SimpleStepMessage> {
        // Get task and step UUIDs from the database
        let task_uuid = self.get_task_uuid(claimed_task.task_uuid).await?;
        let step_uuid = self.get_step_uuid(viable_step.step_uuid).await?;

        // Get ready dependency step UUIDs
        let ready_dependency_step_uuids = self.get_ready_dependency_uuids(viable_step).await?;

        let simple_message = SimpleStepMessage {
            task_uuid,
            step_uuid,
            ready_dependency_step_uuids,
        };

        debug!(
            "✅ STEP_ENQUEUER: Created simple message - task_uuid: {}, step_uuid: {}, {} dependencies",
            simple_message.task_uuid,
            simple_message.step_uuid,
            simple_message.ready_dependency_step_uuids.len()
        );

        Ok(simple_message)
    }

    /// Get task UUID from database by task_uuid
    async fn get_task_uuid(&self, task_uuid: Uuid) -> TaskerResult<Uuid> {
        let row = sqlx::query!(
            "SELECT task_uuid FROM tasker_tasks WHERE task_uuid = $1",
            task_uuid
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch task UUID: {e}")))?;

        Ok(row.task_uuid)
    }

    /// Get step UUID from database by workflow_step_uuid
    async fn get_step_uuid(&self, step_uuid: Uuid) -> TaskerResult<Uuid> {
        let row = sqlx::query!(
            "SELECT workflow_step_uuid FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch step UUID: {e}")))?;

        Ok(row.workflow_step_uuid)
    }

    /// Get UUIDs of ready dependency steps (all transitive dependencies)
    async fn get_ready_dependency_uuids(
        &self,
        viable_step: &ViableStep,
    ) -> TaskerResult<Vec<Uuid>> {
        // Use our new SQL function to get ALL transitive dependencies
        use tasker_shared::database::sql_functions::SqlFunctionExecutor;
        let executor = SqlFunctionExecutor::new(self.pool.clone());

        let transitive_dependencies = executor
            .get_step_transitive_dependencies(viable_step.step_uuid)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to fetch transitive dependencies: {e}"))
            })?;

        // Only include dependencies that are processed (completed)
        let completed_dependencies: Vec<_> = transitive_dependencies
            .into_iter()
            .filter(|dep| dep.processed)
            .collect();

        // Get UUIDs for completed dependencies
        let uuids = if completed_dependencies.is_empty() {
            Vec::new()
        } else {
            completed_dependencies
                .iter()
                .map(|dep| dep.workflow_step_uuid)
                .collect()
        };

        debug!(
            "📋 STEP_ENQUEUER: Found {} transitive dependency UUIDs for step {} (from {} total transitive deps)",
            uuids.len(),
            viable_step.step_uuid,
            completed_dependencies.len()
        );

        Ok(uuids)
    }

    /// Get task execution context for step processing
    #[allow(dead_code)]
    async fn get_task_execution_context(&self, task_uuid: Uuid) -> TaskerResult<Value> {
        // Join with tasker_named_tasks to get task name and version for step handler registry
        let query = "
            SELECT t.context, t.tags, nt.name as task_name, nt.version, ns.name as namespace_name
            FROM tasker_tasks t
            JOIN tasker_named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
            JOIN tasker_task_namespaces ns ON nt.task_namespace_uuid = ns.task_namespace_uuid
            WHERE t.task_uuid = $1::UUID
        ";

        let row: Option<(Value, Value, String, String, String)> = sqlx::query_as(query)
            .bind(task_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to get task context: {e}")))?;

        match row {
            Some((context, tags, task_name, version, namespace_name)) => Ok(serde_json::json!({
                "task_uuid": task_uuid,
                "context": context,
                "tags": tags,
                "task_name": task_name,
                "version": version,
                "namespace_name": namespace_name
            })),
            None => Err(TaskerError::DatabaseError(format!(
                "Task {task_uuid} not found"
            ))),
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &StepEnqueuerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_enqueuer_config_defaults() {
        let config = StepEnqueuerConfig::default();
        assert_eq!(config.max_steps_per_task, 100);
        assert_eq!(config.enqueue_delay_seconds, 0);
        assert!(!config.enable_detailed_logging);
        assert_eq!(config.enqueue_timeout_seconds, 30);
    }

    #[test]
    fn test_step_enqueue_result_creation() {
        let task_uuid = Uuid::now_v7();

        let mut namespace_breakdown = HashMap::new();
        namespace_breakdown.insert(
            "fulfillment".to_string(),
            NamespaceEnqueueStats {
                steps_enqueued: 3,
                steps_failed: 0,
                queue_name: "fulfillment_queue".to_string(),
            },
        );

        let result = StepEnqueueResult {
            task_uuid,
            steps_discovered: 3,
            steps_enqueued: 3,
            steps_failed: 0,
            processing_duration_ms: 150,
            namespace_breakdown,
            warnings: vec![],
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert_eq!(result.steps_discovered, 3);
        assert_eq!(result.steps_enqueued, 3);
        assert_eq!(result.steps_failed, 0);
        assert!(result.warnings.is_empty());
    }
}
