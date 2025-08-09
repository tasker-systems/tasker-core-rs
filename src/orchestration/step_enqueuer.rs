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
//! use tasker_core::orchestration::{step_enqueuer::StepEnqueuer, task_claimer::ClaimedTask};
//! use tasker_core::messaging::PgmqClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! # let database_url = "postgresql://localhost/test";
//! # let pgmq_client = PgmqClient::new(database_url).await
//! #     .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error>)?;
//! let enqueuer = StepEnqueuer::new(pool, pgmq_client).await?;
//!
//! // Enqueue ready steps for a claimed task
//! let claimed_task = ClaimedTask {
//!     task_id: 123,
//!     namespace_name: "fulfillment".to_string(),
//!     // ... other fields
//! #   priority: 2, computed_priority: 4.0, age_hours: 0.1,
//! #   ready_steps_count: 2, claim_timeout_seconds: 300,
//! #   claimed_at: chrono::Utc::now(),
//! };
//!
//! let result = enqueuer.enqueue_ready_steps(&claimed_task).await?;
//! println!("Enqueued {} steps for task {}", result.steps_enqueued, claimed_task.task_id);
//! # Ok(())
//! # }
//! ```

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::error::{Result, TaskerError};
use crate::events::EventPublisher;
use crate::messaging::message::SimpleStepMessage;
use crate::messaging::message::{StepDependencyResult, StepExecutionContext};
use crate::messaging::{PgmqClient, StepMessage, StepMessageMetadata};
use crate::orchestration::{
    state_manager::StateManager, task_claimer::ClaimedTask, types::ViableStep,
    viable_step_discovery::ViableStepDiscovery,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Result of step enqueueing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueueResult {
    /// Task ID that was processed
    pub task_id: i64,
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

/// Configuration for step enqueueing behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueuerConfig {
    /// Maximum number of steps to process per task
    pub max_steps_per_task: usize,
    /// Delay in seconds before making steps visible in queues
    pub enqueue_delay_seconds: i32,
    /// Enable detailed logging for debugging
    pub enable_detailed_logging: bool,
    /// Timeout for individual step enqueueing operations
    pub enqueue_timeout_seconds: u64,
}

impl Default for StepEnqueuerConfig {
    fn default() -> Self {
        Self {
            max_steps_per_task: 100,
            enqueue_delay_seconds: 0,
            enable_detailed_logging: false,
            enqueue_timeout_seconds: 30,
        }
    }
}

impl StepEnqueuerConfig {
    /// Create StepEnqueuerConfig from ConfigManager
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        let config = config_manager.config();

        Self {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enqueue_delay_seconds: 0, // No direct mapping, keep default
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }
}

/// Step enqueueing component for individual step processing
pub struct StepEnqueuer {
    viable_step_discovery: ViableStepDiscovery,
    pgmq_client: PgmqClient,
    pool: PgPool,
    config: StepEnqueuerConfig,
    state_manager: StateManager,
}

impl StepEnqueuer {
    /// Create a new step enqueuer instance
    pub async fn new(pool: PgPool, pgmq_client: PgmqClient) -> Result<Self> {
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

    /// Create a new step enqueuer with custom configuration
    pub async fn with_config(
        pool: PgPool,
        pgmq_client: PgmqClient,
        config: StepEnqueuerConfig,
    ) -> Result<Self> {
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
    #[instrument(skip(self), fields(task_id = claimed_task.task_id, namespace = %claimed_task.namespace_name))]
    pub async fn enqueue_ready_steps(
        &self,
        claimed_task: &ClaimedTask,
    ) -> Result<StepEnqueueResult> {
        let start_time = Instant::now();

        info!(
            task_id = claimed_task.task_id,
            namespace = %claimed_task.namespace_name,
            ready_steps_expected = claimed_task.ready_steps_count,
            "Starting step enqueueing for claimed task"
        );

        // 1. Discover viable steps using existing SQL-based logic
        let viable_steps = self
            .viable_step_discovery
            .find_viable_steps(claimed_task.task_id)
            .await
            .map_err(|e| {
                error!(
                    task_id = claimed_task.task_id,
                    error = %e,
                    "Failed to discover viable steps"
                );
                TaskerError::OrchestrationError(format!("Step discovery failed: {e}"))
            })?;

        let steps_discovered = viable_steps.len();
        debug!(
            task_id = claimed_task.task_id,
            steps_discovered = steps_discovered,
            "Discovered viable steps for enqueueing"
        );

        if steps_discovered == 0 {
            warn!(
                task_id = claimed_task.task_id,
                "No viable steps found for claimed task - task may have been processed already"
            );

            return Ok(StepEnqueueResult {
                task_id: claimed_task.task_id,
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
                            task_id = claimed_task.task_id,
                            step_id = viable_step.step_id,
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
                        viable_step.step_id, viable_step.name, e
                    );
                    warnings.push(warning.clone());

                    warn!(
                        task_id = claimed_task.task_id,
                        step_id = viable_step.step_id,
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
                .mark_task_in_progress(claimed_task.task_id)
                .await
            {
                error!(
                    task_id = claimed_task.task_id,
                    error = %e,
                    "Failed to transition task to in_progress state after enqueueing steps"
                );
                // Don't fail the entire operation - steps were successfully enqueued
                warnings.push(format!(
                    "Warning: Failed to transition task {} to in_progress state: {}",
                    claimed_task.task_id, e
                ));
            } else {
                info!(
                    task_id = claimed_task.task_id,
                    steps_enqueued = steps_enqueued,
                    "Successfully transitioned task to in_progress state after enqueueing steps"
                );
            }
        }

        let result = StepEnqueueResult {
            task_id: claimed_task.task_id,
            steps_discovered,
            steps_enqueued,
            steps_failed,
            processing_duration_ms,
            namespace_breakdown,
            warnings,
        };

        info!(
            task_id = claimed_task.task_id,
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
    ) -> Result<String> {
        info!(
            "🚀 STEP_ENQUEUER: Preparing to enqueue step {} (name: '{}') from task {} (namespace: '{}')",
            viable_step.step_id, viable_step.name, claimed_task.task_id, claimed_task.namespace_name
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
                    viable_step.step_id, queue_name, e
                );
                TaskerError::OrchestrationError(format!(
                    "Failed to enqueue step {} to {}: {}",
                    viable_step.step_id, queue_name, e
                ))
            })?;

        info!(
            "✅ STEP_ENQUEUER: Successfully sent step {} to pgmq queue '{}' with message ID {}",
            viable_step.step_id, queue_name, msg_id
        );

        // Transition the step to "in_progress" state since it's now enqueued for processing
        if let Err(e) = self
            .state_manager
            .mark_step_in_progress(viable_step.step_id)
            .await
        {
            error!(
                "❌ STEP_ENQUEUER: Failed to transition step {} to in_progress state after enqueueing: {}",
                viable_step.step_id, e
            );
            // Continue execution - enqueueing succeeded, state transition failure shouldn't block workflow
        } else {
            info!(
                "✅ STEP_ENQUEUER: Successfully marked step {} as in_progress",
                viable_step.step_id
            );
        }

        info!(
            "🎯 STEP_ENQUEUER: Step enqueueing complete - step_id: {}, task_id: {}, namespace: '{}', queue: '{}', msg_id: {}",
            viable_step.step_id,
            claimed_task.task_id,
            claimed_task.namespace_name,
            queue_name,
            msg_id
        );

        Ok(queue_name)
    }

    /// Create a step message with full execution context
    #[allow(dead_code)]
    async fn create_step_message(
        &self,
        claimed_task: &ClaimedTask,
        viable_step: &ViableStep,
    ) -> Result<StepMessage> {
        // Get task execution context and dependency results
        let task_context = self
            .get_task_execution_context(claimed_task.task_id)
            .await?;
        let dependency_results = self.get_dependency_results(viable_step).await?;

        // Create step execution context with (task, sequence, step) pattern
        let _step_execution_context = serde_json::json!({
            "task": task_context,
            "sequence": dependency_results,
            "step": {
                "step_id": viable_step.step_id,
                "workflow_step_id": viable_step.step_id,
                "step_name": viable_step.name,
                "current_state": viable_step.current_state,
                "named_step_id": viable_step.named_step_id
            }
        });

        // Extract task_name and version from task_context
        let task_name = task_context
            .get("task_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let task_version = task_context
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string();

        let step_message = StepMessage {
            step_id: viable_step.step_id,
            task_id: claimed_task.task_id,
            namespace: claimed_task.namespace_name.clone(),
            task_name,
            task_version,
            step_name: viable_step.name.clone(),
            step_payload: serde_json::json!({}), // ViableStep doesn't have step_payload field
            execution_context: StepExecutionContext {
                task: task_context,
                sequence: dependency_results,
                step: serde_json::json!({
                    "step_id": viable_step.step_id,
                    "workflow_step_id": viable_step.step_id, // Ruby registry expects workflow_step_id
                    "name": viable_step.name,
                    "current_state": viable_step.current_state,
                    "named_step_id": viable_step.named_step_id
                }),
                additional_context: std::collections::HashMap::new(),
            },
            metadata: StepMessageMetadata {
                created_at: Utc::now(),
                retry_count: 0,
                max_retries: 3,
                timeout_ms: 300000, // 5 minutes
                correlation_id: Some(format!(
                    "task-{}-step-{}",
                    claimed_task.task_id, viable_step.step_id
                )),
                priority: claimed_task.priority as u8,
                context: {
                    let mut context = std::collections::HashMap::new();
                    context.insert(
                        "task_priority".to_string(),
                        serde_json::json!(claimed_task.priority),
                    );
                    context.insert(
                        "computed_priority".to_string(),
                        serde_json::json!(claimed_task.computed_priority),
                    );
                    context.insert(
                        "task_age_hours".to_string(),
                        serde_json::json!(claimed_task.age_hours),
                    );
                    context.insert(
                        "orchestrator_claimed_at".to_string(),
                        serde_json::json!(claimed_task.claimed_at),
                    );
                    context.insert(
                        "enqueueing_version".to_string(),
                        serde_json::json!("phase_5.2"),
                    );
                    context
                },
            },
        };

        Ok(step_message)
    }

    /// Create a simple UUID-based step message (simplified architecture)
    ///
    /// This creates the new 3-field message format that leverages the shared database
    /// as the API layer, dramatically reducing message size and complexity.
    async fn create_simple_step_message(
        &self,
        claimed_task: &ClaimedTask,
        viable_step: &ViableStep,
    ) -> Result<SimpleStepMessage> {
        // Get task and step UUIDs from the database
        let task_uuid = self.get_task_uuid(claimed_task.task_id).await?;
        let step_uuid = self.get_step_uuid(viable_step.step_id).await?;

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

    /// Get task UUID from database by task_id
    async fn get_task_uuid(&self, task_id: i64) -> Result<Uuid> {
        let row = sqlx::query!(
            "SELECT task_uuid FROM tasker_tasks WHERE task_id = $1",
            task_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch task UUID: {e}")))?;

        Ok(row.task_uuid)
    }

    /// Get step UUID from database by workflow_step_id
    async fn get_step_uuid(&self, step_id: i64) -> Result<Uuid> {
        let row = sqlx::query!(
            "SELECT step_uuid FROM tasker_workflow_steps WHERE workflow_step_id = $1",
            step_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch step UUID: {e}")))?;

        Ok(row.step_uuid)
    }

    /// Get UUIDs of ready dependency steps (all transitive dependencies)
    async fn get_ready_dependency_uuids(&self, viable_step: &ViableStep) -> Result<Vec<Uuid>> {
        // Use our new SQL function to get ALL transitive dependencies
        use crate::database::sql_functions::SqlFunctionExecutor;
        let executor = SqlFunctionExecutor::new(self.pool.clone());

        let transitive_dependencies = executor
            .get_step_transitive_dependencies(viable_step.step_id)
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
            let step_ids: Vec<i64> = completed_dependencies
                .iter()
                .map(|dep| dep.workflow_step_id)
                .collect();

            let rows = sqlx::query!(
                "SELECT step_uuid FROM tasker_workflow_steps WHERE workflow_step_id = ANY($1)",
                &step_ids
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to fetch dependency UUIDs: {e}"))
            })?;

            rows.into_iter().map(|row| row.step_uuid).collect()
        };

        debug!(
            "📋 STEP_ENQUEUER: Found {} transitive dependency UUIDs for step {} (from {} total transitive deps)",
            uuids.len(),
            viable_step.step_id,
            completed_dependencies.len()
        );

        Ok(uuids)
    }

    /// Get task execution context for step processing
    #[allow(dead_code)]
    async fn get_task_execution_context(&self, task_id: i64) -> Result<Value> {
        // Join with tasker_named_tasks to get task name and version for step handler registry
        let query = "
            SELECT t.context, t.tags, nt.name as task_name, nt.version, ns.name as namespace_name
            FROM tasker_tasks t
            JOIN tasker_named_tasks nt ON t.named_task_id = nt.named_task_id
            JOIN tasker_task_namespaces ns ON nt.task_namespace_id = ns.task_namespace_id
            WHERE t.task_id = $1::BIGINT
        ";

        let row: Option<(Value, Value, String, String, String)> = sqlx::query_as(query)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to get task context: {e}")))?;

        match row {
            Some((context, tags, task_name, version, namespace_name)) => Ok(serde_json::json!({
                "task_id": task_id,
                "context": context,
                "tags": tags,
                "task_name": task_name,
                "version": version,
                "namespace_name": namespace_name
            })),
            None => Err(TaskerError::DatabaseError(format!(
                "Task {task_id} not found"
            ))),
        }
    }

    /// Get dependency results for step execution (sequence data)
    #[allow(dead_code)]
    async fn get_dependency_results(
        &self,
        viable_step: &ViableStep,
    ) -> Result<Vec<StepDependencyResult>> {
        // Query the database to get actual dependency results
        // For steps that have dependencies, we need to get the results from completed dependency steps

        // For now, create a simple dependency result placeholder
        // In a full implementation, this would query the database for dependency step results
        let dependency_results = if viable_step.dependencies_satisfied {
            // Create placeholder dependency results - in real implementation would query database
            vec![StepDependencyResult {
                step_name: format!("dependency_of_{}", viable_step.name),
                step_id: viable_step.step_id - 1, // Placeholder - would be actual dependency step ID
                named_step_id: viable_step.named_step_id - 1, // Placeholder
                results: Some(serde_json::json!({
                    "status": "completed",
                    "dependencies_satisfied": true
                })),
                processed_at: Some(Utc::now()),
                metadata: std::collections::HashMap::new(),
            }]
        } else {
            // No dependencies
            vec![]
        };

        Ok(dependency_results)
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
            task_id: 123,
            steps_discovered: 3,
            steps_enqueued: 3,
            steps_failed: 0,
            processing_duration_ms: 150,
            namespace_breakdown,
            warnings: vec![],
        };

        assert_eq!(result.task_id, 123);
        assert_eq!(result.steps_discovered, 3);
        assert_eq!(result.steps_enqueued, 3);
        assert_eq!(result.steps_failed, 0);
        assert!(result.warnings.is_empty());
    }
}
