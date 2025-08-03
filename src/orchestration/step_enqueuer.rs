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
//! ```rust
//! use tasker_core::orchestration::{step_enqueuer::StepEnqueuer, task_claimer::ClaimedTask};
//! use tasker_core::messaging::PgmqClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! # let pgmq_client = PgmqClient::new(pool.clone()).await?;
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
use crate::messaging::{PgmqClient, StepMessage, StepMessageMetadata};
use crate::messaging::message::StepExecutionContext;
use crate::orchestration::{
    task_claimer::ClaimedTask, types::ViableStep, viable_step_discovery::ViableStepDiscovery,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};

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

/// Step enqueueing component for individual step processing
pub struct StepEnqueuer {
    viable_step_discovery: ViableStepDiscovery,
    pgmq_client: PgmqClient,
    pool: PgPool,
    config: StepEnqueuerConfig,
}

impl StepEnqueuer {
    /// Create a new step enqueuer instance
    pub async fn new(pool: PgPool, pgmq_client: PgmqClient) -> Result<Self> {
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let event_publisher = EventPublisher::new();
        let viable_step_discovery = ViableStepDiscovery::new(sql_executor, event_publisher, pool.clone());

        Ok(Self {
            viable_step_discovery,
            pgmq_client,
            pool,
            config: StepEnqueuerConfig::default(),
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
        let viable_step_discovery = ViableStepDiscovery::new(sql_executor, event_publisher, pool.clone());

        Ok(Self {
            viable_step_discovery,
            pgmq_client,
            pool,
            config,
        })
    }

    /// Enqueue all ready steps for a claimed task
    ///
    /// This discovers viable steps using the existing SQL-based logic, then enqueues each
    /// step individually to its namespace-specific queue for autonomous worker processing.
    #[instrument(skip(self), fields(task_id = claimed_task.task_id, namespace = %claimed_task.namespace_name))]
    pub async fn enqueue_ready_steps(&self, claimed_task: &ClaimedTask) -> Result<StepEnqueueResult> {
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
                TaskerError::OrchestrationError(format!("Step discovery failed: {}", e))
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
            match self.enqueue_individual_step(claimed_task, &viable_step).await {
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
        // Create step message with execution context (task, sequence, step pattern)
        let step_message = self.create_step_message(claimed_task, viable_step).await?;
        
        // Enqueue to namespace-specific queue
        let queue_name = format!("{}_queue", claimed_task.namespace_name);
        
        let msg_id = self
            .pgmq_client
            .send_json_message(&queue_name, &step_message)
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to enqueue step {} to {}: {}",
                    viable_step.step_id, queue_name, e
                ))
            })?;

        debug!(
            task_id = claimed_task.task_id,
            step_id = viable_step.step_id,
            step_name = %viable_step.name,
            queue_name = %queue_name,
            msg_id = msg_id,
            "Step message enqueued successfully"
        );

        Ok(queue_name)
    }

    /// Create a step message with full execution context
    async fn create_step_message(
        &self,
        claimed_task: &ClaimedTask,
        viable_step: &ViableStep,
    ) -> Result<StepMessage> {
        // Get task execution context and dependency results
        let task_context = self.get_task_execution_context(claimed_task.task_id).await?;
        let dependency_results = self.get_dependency_results(viable_step).await?;

        // Create step execution context with (task, sequence, step) pattern
        let step_execution_context = serde_json::json!({
            "task": task_context,
            "sequence": dependency_results,
            "step": {
                "step_id": viable_step.step_id,
                "step_name": viable_step.name,
                "current_state": viable_step.current_state,
                "named_step_id": viable_step.named_step_id
            }
        });

        let step_message = StepMessage {
            step_id: viable_step.step_id,
            task_id: claimed_task.task_id,
            namespace: claimed_task.namespace_name.clone(),
            task_name: "".to_string(), // Will be populated from task context
            task_version: "1.0.0".to_string(), // Default version
            step_name: viable_step.name.clone(),
            step_payload: serde_json::json!({}), // ViableStep doesn't have step_payload field
            execution_context: StepExecutionContext {
                task: task_context,
                sequence: vec![], // Will be populated from dependency results
                step: serde_json::json!({
                    "step_id": viable_step.step_id,
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
                correlation_id: Some(format!("task-{}-step-{}", claimed_task.task_id, viable_step.step_id)),
                priority: claimed_task.priority as u8,
                context: {
                    let mut context = std::collections::HashMap::new();
                    context.insert("task_priority".to_string(), serde_json::json!(claimed_task.priority));
                    context.insert("computed_priority".to_string(), serde_json::json!(claimed_task.computed_priority));
                    context.insert("task_age_hours".to_string(), serde_json::json!(claimed_task.age_hours));
                    context.insert("orchestrator_claimed_at".to_string(), serde_json::json!(claimed_task.claimed_at));
                    context.insert("enqueueing_version".to_string(), serde_json::json!("phase_5.2"));
                    context
                },
            },
        };

        Ok(step_message)
    }

    /// Get task execution context for step processing
    async fn get_task_execution_context(&self, task_id: i64) -> Result<Value> {
        // Use existing SQL function to get task context
        // This is a simplified version - the actual implementation would use SqlFunctionExecutor
        let query = "SELECT context, tags FROM tasker_tasks WHERE task_id = $1::BIGINT";
        
        let row: Option<(Value, Value)> = sqlx::query_as(query)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to get task context: {}", e))
            })?;

        match row {
            Some((context, tags)) => Ok(serde_json::json!({
                "task_id": task_id,
                "context": context,
                "tags": tags
            })),
            None => Err(TaskerError::DatabaseError(format!("Task {} not found", task_id))),
        }
    }

    /// Get dependency results for step execution (sequence data)
    async fn get_dependency_results(&self, viable_step: &ViableStep) -> Result<Value> {
        // This would typically query step execution results for dependencies
        // For now, return the dependencies metadata from viable step
        Ok(serde_json::json!({
            "step_id": viable_step.step_id,
            "dependencies_satisfied": viable_step.dependencies_satisfied,
            "current_state": viable_step.current_state
        }))
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