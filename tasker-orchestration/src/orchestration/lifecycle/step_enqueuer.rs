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
//! use tasker_orchestration::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
//! use tasker_shared::system_context::SystemContext;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create system context and step enqueuer
//! # use tasker_shared::config::ConfigManager;
//! # let config_manager = ConfigManager::load()?;
//! let context = Arc::new(SystemContext::from_config(config_manager).await?);
//! let enqueuer = StepEnqueuer::new(context).await?;
//!
//! // StepEnqueuer is ready to enqueue steps for tasks
//! // See integration tests for complete usage examples
//! let _enqueuer = enqueuer;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::{
    state_manager::StateManager, viable_step_discovery::ViableStepDiscovery,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tasker_shared::config::orchestration::StepEnqueuerConfig;
use tasker_shared::database::sql_functions::ReadyTaskInfo;
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::PgmqClientTrait;
use tasker_shared::models::WorkflowStep;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::types::ViableStep;
use tasker_shared::{SystemContext, TaskerError, TaskerResult};
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

    /// Step UUIDs that were enqueued
    pub step_uuids: Vec<Uuid>,
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
    context: Arc<SystemContext>,
    config: StepEnqueuerConfig,
    state_manager: StateManager,
}

impl StepEnqueuer {
    /// Create a new step enqueuer instance (backward compatibility with standard client)
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        let viable_step_discovery = ViableStepDiscovery::new(context.clone());
        let state_manager = StateManager::new(context.clone());
        let config: StepEnqueuerConfig = context.tasker_config.clone().into();
        Ok(Self {
            viable_step_discovery,
            context,
            config,
            state_manager,
        })
    }

    /// Enqueue all ready steps for a claimed task
    ///
    /// This discovers viable steps using the existing SQL-based logic, then enqueues each
    /// step individually to its namespace-specific queue for autonomous worker processing.
    #[instrument(skip(self), fields(task_uuid = task_info.task_uuid.to_string(), namespace = %task_info.namespace_name))]
    pub async fn enqueue_ready_steps(
        &self,
        task_info: &ReadyTaskInfo,
    ) -> TaskerResult<StepEnqueueResult> {
        let start_time = Instant::now();

        info!(
            task_uuid = task_info.task_uuid.to_string(),
            namespace = %task_info.namespace_name,
            "Starting step enqueueing for claimed task"
        );

        // 1. Discover viable steps using existing SQL-based logic
        let viable_steps = self
            .viable_step_discovery
            .find_viable_steps(task_info.task_uuid)
            .await
            .map_err(|e| {
                error!(
                    task_uuid = task_info.task_uuid.to_string(),
                    error = %e,
                    "Failed to discover viable steps"
                );
                TaskerError::OrchestrationError(format!("Step discovery failed: {e}"))
            })?;

        let steps_discovered = viable_steps.len();
        debug!(
            task_uuid = task_info.task_uuid.to_string(),
            steps_discovered = steps_discovered,
            "Discovered viable steps for enqueueing"
        );

        if steps_discovered == 0 {
            warn!(
                task_uuid = task_info.task_uuid.to_string(),
                "No viable steps found for claimed task - task may have been processed already"
            );

            return Ok(StepEnqueueResult {
                task_uuid: task_info.task_uuid,
                steps_discovered: 0,
                steps_enqueued: 0,
                steps_failed: 0,
                processing_duration_ms: start_time.elapsed().as_millis() as u64,
                namespace_breakdown: HashMap::new(),
                warnings: vec!["No viable steps found for enqueueing".to_string()],
                step_uuids: Vec::new(),
            });
        }

        // 2. Filter and prepare steps for enqueueing based on current state
        // Only enqueue steps that are in valid states for enqueueing
        let enqueuable_steps = self
            .filter_and_prepare_enqueuable_steps(viable_steps)
            .await?;

        let enqueuable_count = enqueuable_steps.len();
        debug!(
            task_uuid = task_info.task_uuid.to_string(),
            steps_discovered = steps_discovered,
            enqueuable_steps = enqueuable_count,
            "Filtered steps ready for enqueueing"
        );

        if enqueuable_count == 0 {
            warn!(
                task_uuid = task_info.task_uuid.to_string(),
                "No enqueuable steps after state filtering - steps may be in error/waiting states"
            );

            return Ok(StepEnqueueResult {
                task_uuid: task_info.task_uuid,
                steps_discovered,
                steps_enqueued: 0,
                steps_failed: 0,
                processing_duration_ms: start_time.elapsed().as_millis() as u64,
                namespace_breakdown: HashMap::new(),
                warnings: vec!["No steps in enqueuable state".to_string()],
                step_uuids: Vec::new(),
            });
        }

        // 3. Enqueue each step individually to namespace queues
        let mut steps_enqueued = 0;
        let mut steps_failed = 0;
        let mut namespace_breakdown: HashMap<String, NamespaceEnqueueStats> = HashMap::new();
        let mut warnings = Vec::new();
        let mut step_uuids = Vec::new();

        for viable_step in enqueuable_steps {
            match self.enqueue_individual_step(task_info, &viable_step).await {
                Ok(queue_name) => {
                    steps_enqueued += 1;
                    step_uuids.push(viable_step.step_uuid);

                    // Update namespace stats
                    let stats = namespace_breakdown
                        .entry(task_info.namespace_name.clone())
                        .or_insert_with(|| NamespaceEnqueueStats {
                            steps_enqueued: 0,
                            steps_failed: 0,
                            queue_name: queue_name.clone(),
                        });
                    stats.steps_enqueued += 1;

                    if self.config.enable_detailed_logging {
                        debug!(
                            task_uuid = task_info.task_uuid.to_string(),
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
                    let queue_name = format!("worker_{}_queue", task_info.namespace_name);
                    let stats = namespace_breakdown
                        .entry(task_info.namespace_name.clone())
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
                        task_uuid = task_info.task_uuid.to_string(),
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
                .mark_task_in_progress(task_info.task_uuid)
                .await
            {
                error!(
                    task_uuid = task_info.task_uuid.to_string(),
                    error = %e,
                    "Failed to transition task to in_progress state after enqueueing steps"
                );
                // Don't fail the entire operation - steps were successfully enqueued
                warnings.push(format!(
                    "Warning: Failed to transition task {} to in_progress state: {}",
                    task_info.task_uuid, e
                ));
            } else {
                info!(
                    task_uuid = task_info.task_uuid.to_string(),
                    steps_enqueued = steps_enqueued,
                    "Successfully transitioned task to in_progress state after enqueueing steps"
                );
            }
        }

        let result = StepEnqueueResult {
            task_uuid: task_info.task_uuid,
            steps_discovered,
            steps_enqueued,
            steps_failed,
            processing_duration_ms,
            namespace_breakdown,
            warnings,
            step_uuids,
        };

        info!(
            task_uuid = task_info.task_uuid.to_string(),
            steps_discovered = steps_discovered,
            steps_enqueued = steps_enqueued,
            steps_failed = steps_failed,
            processing_duration_ms = processing_duration_ms,
            "Completed step enqueueing for claimed task"
        );

        Ok(result)
    }

    /// Filter viable steps and prepare them for enqueueing based on current state
    ///
    /// Handles state transitions for steps that need to move from WaitingForRetry to Pending
    /// before they can be enqueued. Only returns steps that are in valid states for enqueueing.
    async fn filter_and_prepare_enqueuable_steps(
        &self,
        viable_steps: Vec<ViableStep>,
    ) -> TaskerResult<Vec<ViableStep>> {
        let mut enqueuable_steps = Vec::new();

        for viable_step in viable_steps {
            // Load the full WorkflowStep from database to check current state
            let workflow_step =
                WorkflowStep::find_by_id(self.context.database_pool(), viable_step.step_uuid)
                    .await
                    .map_err(|e| {
                        TaskerError::DatabaseError(format!(
                            "Failed to load step {} for state check: {}",
                            viable_step.step_uuid, e
                        ))
                    })?;

            let Some(workflow_step) = workflow_step else {
                warn!(
                    step_uuid = %viable_step.step_uuid,
                    "Step not found in database, skipping"
                );
                continue;
            };

            // Create state machine to check current state
            let mut state_machine = StepStateMachine::new(workflow_step, self.context.clone());
            let current_state = state_machine.current_state().await.map_err(|e| {
                TaskerError::StateTransitionError(format!(
                    "Failed to get current state for step {}: {}",
                    viable_step.step_uuid, e
                ))
            })?;

            match current_state {
                WorkflowStepState::Pending => {
                    // Ready to enqueue directly
                    debug!(
                        step_uuid = %viable_step.step_uuid,
                        step_name = %viable_step.name,
                        "Step in Pending state, ready to enqueue"
                    );
                    enqueuable_steps.push(viable_step);
                }

                WorkflowStepState::WaitingForRetry => {
                    // The viable_step came from the SQL function which already checked backoff
                    // If it's in the viable steps list and in WaitingForRetry state, backoff has expired
                    // Transition WaitingForRetry → Pending via Retry event
                    info!(
                        step_uuid = %viable_step.step_uuid,
                        step_name = %viable_step.name,
                        "Step in WaitingForRetry with expired backoff, transitioning to Pending"
                    );

                    match state_machine.transition(StepEvent::Retry).await {
                        Ok(_) => {
                            debug!(
                                step_uuid = %viable_step.step_uuid,
                                "Successfully transitioned to Pending, ready to enqueue"
                            );
                            enqueuable_steps.push(viable_step);
                        }
                        Err(e) => {
                            warn!(
                                step_uuid = %viable_step.step_uuid,
                                error = %e,
                                "Failed to transition from WaitingForRetry to Pending, skipping"
                            );
                        }
                    }
                }

                WorkflowStepState::Error => {
                    // Defensive: Should not happen if SQL function is correct
                    warn!(
                        step_uuid = %viable_step.step_uuid,
                        step_name = %viable_step.name,
                        "Step in Error state returned as viable - should be in WaitingForRetry. Skipping."
                    );
                }

                _ => {
                    // Skip steps in other states (Enqueued, InProgress, Complete, etc.)
                    debug!(
                        step_uuid = %viable_step.step_uuid,
                        step_name = %viable_step.name,
                        current_state = ?current_state,
                        "Skipping step - not in enqueueable state"
                    );
                }
            }
        }

        Ok(enqueuable_steps)
    }

    /// Enqueue a single viable step to its namespace queue
    async fn enqueue_individual_step(
        &self,
        task_info: &ReadyTaskInfo,
        viable_step: &ViableStep,
    ) -> TaskerResult<String> {
        info!(
            "Preparing to enqueue step {} (name: '{}') from task {} (namespace: '{}')",
            viable_step.step_uuid, viable_step.name, task_info.task_uuid, task_info.namespace_name
        );

        // Create simple UUID-based message (simplified architecture)
        let simple_message = self
            .create_simple_step_message(task_info, viable_step)
            .await?;

        // Enqueue to namespace-specific queue
        let queue_name = format!("worker_{}_queue", task_info.namespace_name);

        info!(
            "Created simple step message - targeting queue '{}' for step UUID '{}'",
            queue_name, simple_message.step_uuid
        );

        let msg_id = self
            .context
            .message_client()
            .send_json_message(&queue_name, &simple_message)
            .await
            .map_err(|e| {
                error!(
                    "Failed to enqueue step {} to queue '{}': {}",
                    viable_step.step_uuid, queue_name, e
                );
                TaskerError::OrchestrationError(format!(
                    "Failed to enqueue step {} to {}: {}",
                    viable_step.step_uuid, queue_name, e
                ))
            })?;

        info!(
            "Successfully sent step {} to pgmq queue '{}' with message ID {}",
            viable_step.step_uuid, queue_name, msg_id
        );

        // This replaces the old behavior of marking steps as in_progress when enqueued
        if let Err(e) = self
            .state_manager
            .mark_step_enqueued(viable_step.step_uuid)
            .await
        {
            error!(
                "Failed to transition step {} to enqueued state after enqueueing: {}",
                viable_step.step_uuid, e
            );
            // Continue execution - enqueueing succeeded, state transition failure shouldn't block workflow
        } else {
            info!(
                "Successfully marked step {} as enqueued",
                viable_step.step_uuid
            );
        }

        info!(
            "Step enqueueing complete - step_uuid: {}, task_uuid: {}, namespace: '{}', queue: '{}', msg_id: {}",
            viable_step.step_uuid,
            task_info.task_uuid,
            task_info.namespace_name,
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
        task_info: &ReadyTaskInfo,
        viable_step: &ViableStep,
    ) -> TaskerResult<SimpleStepMessage> {
        // Get task UUID and correlation_id from database
        let (task_uuid, correlation_id) = self.get_task_info(task_info.task_uuid).await?;
        let step_uuid = self.get_step_uuid(viable_step.step_uuid).await?;

        // Create minimal SimpleStepMessage - workers will query dependencies as needed
        let simple_message = SimpleStepMessage {
            task_uuid,
            step_uuid,
            correlation_id,
        };

        debug!(
            "Created minimal message - task_uuid: {}, step_uuid: {}, correlation_id: {} (dependencies queried by workers)",
            simple_message.task_uuid,
            simple_message.step_uuid,
            simple_message.correlation_id
        );

        Ok(simple_message)
    }

    /// Get task UUID and correlation_id from database by task_uuid
    async fn get_task_info(&self, task_uuid: Uuid) -> TaskerResult<(Uuid, Uuid)> {
        let row = sqlx::query!(
            "SELECT task_uuid, correlation_id FROM tasker_tasks WHERE task_uuid = $1",
            task_uuid
        )
        .fetch_one(self.context.database_pool())
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch task info: {e}")))?;

        Ok((row.task_uuid, row.correlation_id))
    }

    /// Get step UUID from database by workflow_step_uuid
    async fn get_step_uuid(&self, step_uuid: Uuid) -> TaskerResult<Uuid> {
        let row = sqlx::query!(
            "SELECT workflow_step_uuid FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .fetch_one(self.context.database_pool())
        .await
        .map_err(|e| TaskerError::DatabaseError(format!("Failed to fetch step UUID: {e}")))?;

        Ok(row.workflow_step_uuid)
    }

    // REMOVED: get_ready_dependency_uuids method - no longer needed
    // Workers will query dependencies directly as needed
    // This eliminates redundant database queries in orchestration layer

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
            .fetch_optional(self.context.database_pool())
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
            step_uuids: vec![],
        };

        assert_eq!(result.task_uuid, task_uuid);
        assert_eq!(result.steps_discovered, 3);
        assert_eq!(result.steps_enqueued, 3);
        assert_eq!(result.steps_failed, 0);
        assert!(result.warnings.is_empty());
    }
}
