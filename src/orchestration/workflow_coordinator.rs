//! # Workflow Coordinator
//!
//! ## Architecture: Main Orchestration Engine
//!
//! The WorkflowCoordinator is the central orchestration engine that coordinates all workflow
//! execution activities. It brings together all orchestration components to provide complete
//! workflow lifecycle management from task creation to completion.
//!
//! ## Key Responsibilities
//!
//! - **Workflow Lifecycle Management**: Start, execute, monitor, and complete workflows
//! - **Step Discovery**: Use ViableStepDiscovery to find ready-to-execute steps
//! - **Concurrent Execution**: Leverage StepExecutor for parallel step execution
//! - **State Management**: Coordinate state transitions through StateManager
//! - **Event Coordination**: Publish workflow lifecycle events
//! - **Error Recovery**: Handle failures and coordinate retry logic
//! - **Task Finalization**: Determine when tasks are complete and finalize them
//!
//! ## Integration with Orchestration Components
//!
//! The WorkflowCoordinator serves as the conductor that orchestrates:
//! - **ViableStepDiscovery**: Finds steps ready for execution
//! - **StepExecutor**: Executes individual steps with concurrency control
//! - **StateManager**: Manages workflow and step state transitions
//! - **EventPublisher**: Publishes orchestration lifecycle events
//! - **TaskHandlerRegistry**: Resolves handlers for step execution
//! - **FrameworkIntegration**: Delegates to framework-specific execution
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
//! use tasker_core::orchestration::types::FrameworkIntegration;
//! use std::sync::Arc;
//!
//! # async fn example(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! // Create the workflow coordinator
//! let coordinator = WorkflowCoordinator::new(pool);
//!
//! // Execute a task workflow
//! let task_id = 123;
//! let result = coordinator.execute_task_workflow(task_id, framework).await?;
//!
//! match result {
//!     tasker_core::orchestration::TaskOrchestrationResult::Complete { .. } => {
//!         println!("Task completed successfully!");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::InProgress { .. } => {
//!         println!("Task still in progress, will be re-queued");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::Failed { .. } => {
//!         println!("Task failed");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::Blocked { .. } => {
//!         println!("Task blocked waiting for dependencies");
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::events::{Event, EventPublisher, OrchestrationEvent as EventsOrchestrationEvent};
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::orchestration::state_manager::{StateHealthSummary, StateManager};
use crate::orchestration::system_events::SystemEventsManager;
use crate::orchestration::types::TaskOrchestrationResult;
use crate::orchestration::types::{StepResult, ViableStep};
use crate::orchestration::viable_step_discovery::ViableStepDiscovery;
use crate::registry::TaskHandlerRegistry;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

/// Configuration for workflow coordination
#[derive(Debug, Clone)]
pub struct WorkflowCoordinatorConfig {
    /// Maximum number of discovery attempts before failing
    pub max_discovery_attempts: u32,
    /// Delay between discovery attempts when no steps are found
    pub discovery_retry_delay: Duration,
    /// Maximum execution time for a single workflow run
    pub max_workflow_duration: Duration,
    /// Enable detailed performance metrics
    pub enable_metrics: bool,
    /// Maximum steps to execute in a single workflow run
    pub max_steps_per_run: usize,
    /// Batch size for concurrent step execution
    pub step_batch_size: usize,
}

impl Default for WorkflowCoordinatorConfig {
    fn default() -> Self {
        Self {
            max_discovery_attempts: 3,
            discovery_retry_delay: Duration::from_secs(1),
            max_workflow_duration: Duration::from_secs(3600), // 1 hour
            enable_metrics: true,
            max_steps_per_run: 1000,
            step_batch_size: 10,
        }
    }
}

impl WorkflowCoordinatorConfig {
    /// Create configuration optimized for testing with short timeouts
    pub fn for_testing() -> Self {
        Self {
            max_discovery_attempts: 2,
            discovery_retry_delay: Duration::from_millis(100),
            max_workflow_duration: Duration::from_secs(2), // Short timeout for tests
            enable_metrics: true,
            max_steps_per_run: 10,
            step_batch_size: 5,
        }
    }

    /// Create configuration optimized for testing with custom timeout
    pub fn for_testing_with_timeout(timeout_secs: u64) -> Self {
        Self {
            max_discovery_attempts: 2,
            discovery_retry_delay: Duration::from_millis(100),
            max_workflow_duration: Duration::from_secs(timeout_secs),
            enable_metrics: true,
            max_steps_per_run: 10,
            step_batch_size: 5,
        }
    }

    /// Create configuration from ConfigurationManager
    pub fn from_config_manager(config_manager: &ConfigurationManager) -> Self {
        let system_config = config_manager.system_config();

        Self {
            max_discovery_attempts: system_config.execution.max_discovery_attempts,
            discovery_retry_delay: Duration::from_secs(
                system_config.backoff.default_reenqueue_delay as u64,
            ),
            max_workflow_duration: Duration::from_secs(
                system_config.execution.default_timeout_seconds,
            ),
            enable_metrics: system_config.telemetry.enabled,
            max_steps_per_run: system_config.execution.max_concurrent_steps,
            step_batch_size: system_config.execution.step_batch_size,
        }
    }
}

/// Workflow execution metrics
#[derive(Debug, Clone)]
pub struct WorkflowExecutionMetrics {
    pub task_id: i64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_duration: Option<Duration>,
    pub steps_discovered: usize,
    pub steps_executed: usize,
    pub steps_succeeded: usize,
    pub steps_failed: usize,
    pub steps_retried: usize,
    pub discovery_attempts: u32,
    pub discovery_duration: Duration,
    pub execution_duration: Duration,
}

/// Result of step discovery and processing
#[derive(Debug)]
struct DiscoveryResult {
    should_break: bool,
    batch_results: Option<Vec<StepResult>>,
}

/// Main workflow coordinator that orchestrates task execution
pub struct WorkflowCoordinator {
    /// Discovers viable steps for execution
    viable_step_discovery: ViableStepDiscovery,
    /// Executes individual steps
    /// Manages state transitions
    state_manager: StateManager,
    /// Publishes orchestration events
    event_publisher: EventPublisher,
    /// Finalizes tasks when no more viable steps
    task_finalizer: crate::orchestration::task_finalizer::TaskFinalizer,
    /// Configuration
    config: WorkflowCoordinatorConfig,
    /// Configuration manager for system settings
    config_manager: Arc<ConfigurationManager>,
    /// System events manager for structured event publishing
    events_manager: Arc<SystemEventsManager>,
    /// pgmq client for queue-based step enqueueing
    pgmq_client: Arc<crate::messaging::PgmqClient>,
    /// Task handler registry for resolving step handlers
    task_handler_registry: Arc<TaskHandlerRegistry>,
    /// Database connection pool for direct database operations
    database_pool: PgPool,
}

impl WorkflowCoordinator {
    /// Create a new workflow coordinator with pgmq client for queue-based step enqueueing
    ///
    /// This is the main constructor for the pgmq architecture where steps are enqueued
    /// to PostgreSQL message queues instead of using TCP command routing.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - Workflow coordinator configuration
    /// * `config_manager` - Shared configuration manager
    /// * `event_publisher` - Shared EventPublisher instance
    /// * `shared_registry` - Shared TaskHandlerRegistry instance
    /// * `pgmq_client` - Shared PgmqClient for queue-based step enqueueing
    pub fn new(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
        event_publisher: crate::events::publisher::EventPublisher,
        shared_registry: TaskHandlerRegistry,
        pgmq_client: Arc<crate::messaging::PgmqClient>,
    ) -> Self {
        let sql_executor = crate::database::sql_functions::SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer =
            crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
                pool.clone(),
                event_publisher.clone(),
            );

        // Create system events manager - in production this would be loaded from file
        let events_manager = Arc::new(SystemEventsManager::new(
            crate::orchestration::system_events::SystemEventsConfig {
                event_metadata: std::collections::HashMap::new(),
                state_machine_mappings: crate::orchestration::system_events::StateMachineMappings {
                    task_transitions: vec![],
                    step_transitions: vec![],
                },
            },
        ));

        Self {
            viable_step_discovery,
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
            pgmq_client,
            task_handler_registry: Arc::new(shared_registry),
            database_pool: pool,
        }
    }

    /// Create a new workflow coordinator with configuration manager (compatibility method)
    ///
    /// This is a compatibility method for code that still uses the old constructor.
    /// For new code, use the main `new()` constructor.
    pub fn with_config_manager(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
    ) -> Result<Self, String> {
        // This would require a pgmq client, which we don't have in the old signature
        // Return an error explaining the migration path
        Err("WorkflowCoordinator::with_config_manager() is deprecated. Use WorkflowCoordinator::new() with a PgmqClient instead.".to_string())
    }

    /// Execute a complete task workflow
    ///
    /// In batch execution architecture, this method:
    /// 1. Publishes viable steps to ZeroMQ as fire-and-forget batches
    /// 2. Returns when no more steps are immediately ready for execution
    /// 3. Does NOT finalize the workflow - that happens via the result listener
    #[instrument(skip(self), fields(task_id = task_id))]
    pub async fn execute_task_workflow(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        info!(task_id = task_id, "Starting task workflow execution");

        let mut metrics = self.initialize_workflow_metrics(task_id).await?;

        // Publish workflow started event
        self.publish_workflow_started_event(task_id, &metrics)
            .await?;

        // Transition task to in_progress if needed
        self.ensure_task_in_progress(task_id).await?;

        // Execute main orchestration loop - publishes batches and returns
        self.execute_step_batch_publication(task_id, &mut metrics)
            .await?;

        // Return Published result - finalization happens via result listener
        info!(
            task_id = task_id,
            steps_discovered = metrics.steps_discovered,
            steps_published = metrics.steps_executed,
            "Workflow orchestration complete - batches published to ZeroMQ"
        );

        Ok(TaskOrchestrationResult::Published {
            task_id,
            viable_steps_discovered: metrics.steps_discovered,
            steps_published: metrics.steps_executed,
            batch_id: None, // TODO: Get actual batch ID from ZmqPubSubExecutor
            publication_time_ms: metrics.execution_duration.as_millis() as u64,
            next_poll_delay_ms: 1000, // 1 second delay before next check
        })
    }

    /// Initialize workflow execution metrics
    async fn initialize_workflow_metrics(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<WorkflowExecutionMetrics> {
        Ok(WorkflowExecutionMetrics {
            task_id,
            started_at: Utc::now(),
            completed_at: None,
            total_duration: None,
            steps_discovered: 0,
            steps_executed: 0,
            steps_succeeded: 0,
            steps_failed: 0,
            steps_retried: 0,
            discovery_attempts: 0,
            discovery_duration: Duration::default(),
            execution_duration: Duration::default(),
        })
    }

    /// Publish workflow started event
    async fn publish_workflow_started_event(
        &self,
        task_id: i64,
        metrics: &WorkflowExecutionMetrics,
    ) -> OrchestrationResult<()> {
        // Publish structured system event
        let task_start_payload = serde_json::json!({
            "task_id": task_id.to_string(),
            "task_name": format!("workflow_task_{}", task_id), // Enhancement: Load task name from database or pass as parameter
            "timestamp": metrics.started_at.to_rfc3339()
        });

        info!(task_id = task_id, "Publishing task.start_requested event");

        // Validate event payload against schema
        if let Err(e) = self.events_manager.config().validate_event_payload(
            "task",
            "start_requested",
            &task_start_payload,
        ) {
            warn!(task_id = task_id, error = %e, "Task start event validation failed");
        }

        // Also publish the original orchestration event for backward compatibility
        self.event_publisher
            .publish_event(Event::orchestration(
                EventsOrchestrationEvent::TaskOrchestrationStarted {
                    task_id,
                    framework: "fire_and_forget_tcp".to_string(),
                    started_at: metrics.started_at,
                },
            ))
            .await
            .map_err(OrchestrationError::from)
    }

    /// Execute the main orchestration loop
    async fn execute_step_batch_publication(
        &self,
        task_id: i64,
        metrics: &mut WorkflowExecutionMetrics,
    ) -> OrchestrationResult<DiscoveryResult> {
        let mut consecutive_empty_discoveries = 0;
        // Discover and process viable steps
        let batch_results = self
            .discover_and_process_steps(task_id, metrics, &mut consecutive_empty_discoveries)
            .await?;
        // Continue loop for next discovery iteration
        debug!(
            task_id = task_id,
            "Continuing workflow loop - no break condition met"
        );

        Ok(batch_results)
    }

    /// Discover viable steps and process them
    async fn discover_and_process_steps(
        &self,
        task_id: i64,
        metrics: &mut WorkflowExecutionMetrics,
        consecutive_empty_discoveries: &mut u32,
    ) -> OrchestrationResult<DiscoveryResult> {
        // Discover viable steps
        let discovery_start = Instant::now();
        metrics.discovery_attempts += 1;

        let viable_steps = match self.discover_viable_steps(task_id).await {
            Ok(steps) => steps,
            Err(e) => {
                error!(
                    task_id = task_id,
                    error = %e,
                    "Failed to discover viable steps"
                );
                return Err(e);
            }
        };

        metrics.discovery_duration += discovery_start.elapsed();
        metrics.steps_discovered += viable_steps.len();

        // Handle empty discovery
        if viable_steps.is_empty() {
            let discovery_result = self
                .handle_empty_discovery(task_id, consecutive_empty_discoveries)
                .await?;

            return Ok(discovery_result);
        }

        // Reset counter when we find steps
        *consecutive_empty_discoveries = 0;

        // Publish viable steps discovery event
        let step_ids: Vec<i64> = viable_steps.iter().map(|step| step.step_id).collect();
        let viable_steps_payload = self.events_manager.create_viable_steps_discovered_payload(
            task_id,
            &step_ids,
            &self
                .config_manager
                .system_config()
                .execution
                .processing_mode,
        );

        debug!(
            task_id = task_id,
            steps_count = viable_steps.len(),
            "Publishing workflow.viable_steps_discovered event"
        );

        if let Err(e) = self.events_manager.config().validate_event_payload(
            "workflow",
            "viable_steps_discovered",
            &viable_steps_payload,
        ) {
            warn!(task_id = task_id, error = %e, "Viable steps discovery event validation failed");
        }

        // Execute the discovered steps
        let batch_results = self
            .execute_discovered_steps(task_id, viable_steps, metrics)
            .await?;

        Ok(DiscoveryResult {
            should_break: false,
            batch_results: Some(batch_results),
        })
    }

    /// Handle the case when no viable steps are discovered
    async fn handle_empty_discovery(
        &self,
        task_id: i64,
        consecutive_empty_discoveries: &mut u32,
    ) -> OrchestrationResult<DiscoveryResult> {
        *consecutive_empty_discoveries += 1;

        if *consecutive_empty_discoveries >= self.config.max_discovery_attempts {
            info!(
                task_id = task_id,
                attempts = consecutive_empty_discoveries,
                "No viable steps found after multiple attempts - calling task finalizer"
            );

            // Call TaskFinalizer to determine next action
            match self.task_finalizer.handle_no_viable_steps(task_id).await {
                Ok(finalization_result) => {
                    info!(
                        task_id = task_id,
                        action = ?finalization_result.action,
                        reason = ?finalization_result.reason,
                        "Task finalization completed"
                    );

                    match finalization_result.action {
                        crate::orchestration::task_finalizer::FinalizationAction::Reenqueued => {
                            info!(
                                task_id = task_id,
                                "Task re-enqueued - breaking workflow loop"
                            );
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Completed => {
                            info!(task_id = task_id, "Task completed - breaking workflow loop");
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Failed => {
                            info!(task_id = task_id, "Task failed - breaking workflow loop");
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Pending => {
                            info!(
                                task_id = task_id,
                                "Task marked as pending - continuing loop"
                            );
                            *consecutive_empty_discoveries = 0; // Reset counter to continue trying
                            return Ok(DiscoveryResult {
                                should_break: false,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::NoAction => {
                            warn!(
                                task_id = task_id,
                                "Task finalizer took no action - breaking loop"
                            );
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    error!(
                        task_id = task_id,
                        error = %e,
                        "Failed to finalize task - breaking workflow loop"
                    );
                    return Ok(DiscoveryResult {
                        should_break: true,
                        batch_results: None,
                    });
                }
            }
        }

        // Wait before retrying discovery
        tokio::time::sleep(self.config.discovery_retry_delay).await;
        Ok(DiscoveryResult {
            should_break: false,
            batch_results: None,
        })
    }

    /// Execute discovered viable steps in batches
    async fn execute_discovered_steps(
        &self,
        task_id: i64,
        viable_steps: Vec<ViableStep>,
        metrics: &mut WorkflowExecutionMetrics,
    ) -> OrchestrationResult<Vec<StepResult>> {
        let execution_start = Instant::now();
        let steps_count = viable_steps.len();

        let batch_results = self.execute_step_batch(task_id, viable_steps).await?;

        // Update metrics with the actual number of steps published
        // Only count non-empty results (steps actually sent to workers)
        let published_steps = batch_results.len();
        metrics.steps_executed += published_steps;

        tracing::debug!(
            task_id = task_id,
            viable_steps = steps_count,
            published_steps = published_steps,
            "Updated metrics.steps_executed after batch publication"
        );

        metrics.execution_duration += execution_start.elapsed();
        Ok(batch_results)
    }

    /// Get current workflow health summary
    pub async fn get_workflow_health(&self) -> OrchestrationResult<StateHealthSummary> {
        self.state_manager.get_state_health_summary().await
    }

    /// Ensure task is in progress state
    async fn ensure_task_in_progress(&self, task_id: i64) -> OrchestrationResult<()> {
        let evaluation = self.state_manager.evaluate_task_state(task_id).await?;

        if evaluation.transition_required {
            debug!(
                task_id = task_id,
                from_state = %evaluation.current_state,
                to_state = ?evaluation.recommended_state,
                "Transitioning task state"
            );
        }

        Ok(())
    }

    /// Discover viable steps for execution
    async fn discover_viable_steps(&self, task_id: i64) -> OrchestrationResult<Vec<ViableStep>> {
        let steps = self
            .viable_step_discovery
            .find_viable_steps(task_id)
            .await?;

        if !steps.is_empty() {
            // Publish discovery event
            // Convert viable steps to events format
            let events_viable_steps: Vec<crate::events::ViableStep> = steps
                .iter()
                .map(|step| crate::events::ViableStep {
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
                .publish_event(Event::orchestration(
                    EventsOrchestrationEvent::ViableStepsDiscovered {
                        task_id,
                        step_count: steps.len(),
                        steps: events_viable_steps,
                    },
                ))
                .await?;
        }

        Ok(steps)
    }

    /// Execute a batch of steps using individual step enqueueing (Phase 5.2)
    async fn execute_step_batch(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
    ) -> OrchestrationResult<Vec<StepResult>> {
        self.enqueue_individual_steps(task_id, steps).await
    }

    /// Phase 5.2: Enqueue individual steps to namespace-specific queues with execution context
    /// This replaces the batch enqueueing system with individual StepMessage enqueueing.
    async fn enqueue_individual_steps(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
    ) -> OrchestrationResult<Vec<StepResult>> {
        if steps.is_empty() {
            return Ok(vec![]);
        }

        tracing::info!(
            task_id = task_id,
            step_count = steps.len(),
            "WorkflowCoordinator: Starting individual step enqueueing to pgmq"
        );

        // Get task information for execution context
        let task = crate::models::core::task::Task::find_by_id(&self.database_pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "load_task_for_step_enqueueing".to_string(),
                reason: format!("Failed to load task: {}", e),
            })?
            .ok_or_else(|| OrchestrationError::TaskExecutionFailed {
                task_id,
                reason: "Task not found".to_string(),
                error_code: Some("TASK_NOT_FOUND".to_string()),
            })?;

        // Get task orchestration info for namespace and metadata
        let task_info = task
            .for_orchestration(&self.database_pool)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_task_orchestration_info".to_string(),
                reason: format!("Failed to get task orchestration info: {}", e),
            })?;

        let mut all_results = Vec::new();

        // Process each step individually
        for (sequence, step) in steps.into_iter().enumerate() {
            match self
                .enqueue_individual_step(task_id, &task, &task_info, &step, sequence)
                .await
            {
                Ok(step_result) => {
                    all_results.push(step_result);
                }
                Err(e) => {
                    // Create failure result for step that couldn't be enqueued
                    let failure_result = StepResult {
                        step_id: step.step_id,
                        status: crate::orchestration::types::StepStatus::Failed,
                        output: serde_json::json!({
                            "status": "individual_enqueue_failed",
                            "message": "Failed to enqueue individual step to pgmq",
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        }),
                        execution_duration: std::time::Duration::from_millis(1),
                        error_message: Some(format!("Individual step enqueue failed: {}", e)),
                        retry_after: Some(std::time::Duration::from_secs(30)),
                        error_code: Some("INDIVIDUAL_STEP_ENQUEUE_FAILED".to_string()),
                        error_context: None,
                    };
                    all_results.push(failure_result);
                }
            }
        }

        tracing::info!(
            task_id = task_id,
            total_steps = all_results.len(),
            "WorkflowCoordinator: Completed individual step enqueueing"
        );

        Ok(all_results)
    }

    /// Enqueue a single step with full execution context
    async fn enqueue_individual_step(
        &self,
        task_id: i64,
        task: &crate::models::core::task::Task,
        task_info: &crate::models::core::task::TaskForOrchestration,
        step: &ViableStep,
        sequence: usize,
    ) -> OrchestrationResult<StepResult> {
        // Determine namespace from step name
        let namespace = self.determine_step_namespace(step).await?;

        // Fetch dependency results for this step
        let dependency_results = self.fetch_step_dependency_results(step.step_id).await?;

        // Create execution context with (task, sequence, step) pattern
        let execution_context = crate::messaging::message::StepExecutionContext::new(
            // Task data as JSON for handler access
            serde_json::json!({
                "task_id": task.task_id,
                "named_task_id": task.named_task_id,
                "complete": task.complete,
                "requested_at": task.requested_at,
                "initiator": task.initiator,
                "source_system": task.source_system,
                "reason": task.reason,
                "bypass_steps": task.bypass_steps,
                "tags": task.tags,
                "context": task.context,
                "identity_hash": task.identity_hash,
                "created_at": task.created_at,
                "updated_at": task.updated_at,
                "claimed_at": task.claimed_at,
                "claimed_by": task.claimed_by,
                "priority": task.priority,
                "claim_timeout_seconds": task.claim_timeout_seconds
            }),
            // Dependency chain results: all completed steps this step depends on
            dependency_results,
            // Step data as JSON for handler access
            serde_json::json!({
                "step_id": step.step_id,
                "task_id": step.task_id,
                "named_step_id": step.named_step_id,
                "name": step.name,
                "current_state": step.current_state,
                "dependencies_satisfied": step.dependencies_satisfied,
                "retry_eligible": step.retry_eligible,
                "attempts": step.attempts,
                "retry_limit": step.retry_limit,
                "last_failure_at": step.last_failure_at,
                "next_retry_at": step.next_retry_at
            }),
        );

        // Create step message with execution context
        let step_message = crate::messaging::message::StepMessage::new(
            step.step_id,
            task_id,
            namespace.clone(),
            task_info.task_name.clone(),
            "1.0.0".to_string(), // TODO: Get actual task version from database
            step.name.clone(),
            serde_json::json!({
                "step_id": step.step_id,
                "task_id": step.task_id,
                "named_step_id": step.named_step_id,
                "current_state": step.current_state,
                "dependencies_satisfied": step.dependencies_satisfied,
                "retry_eligible": step.retry_eligible,
                "attempts": step.attempts,
                "retry_limit": step.retry_limit,
                "last_failure_at": step.last_failure_at,
                "next_retry_at": step.next_retry_at,
                "context": task.context,
            }),
            execution_context,
        );

        // Determine the target queue name (namespace-specific)
        let queue_name = format!("{}_queue", namespace);

        // Enqueue the individual step message
        let enqueue_time = chrono::Utc::now();
        match self
            .pgmq_client
            .send_json_message(&queue_name, &step_message)
            .await
        {
            Ok(message_id) => {
                tracing::debug!(
                    task_id = task_id,
                    step_id = step.step_id,
                    namespace = %namespace,
                    queue_name = %queue_name,
                    message_id = message_id,
                    sequence = sequence,
                    "Successfully enqueued individual step to pgmq"
                );

                // Create successful enqueueing result
                Ok(StepResult {
                    step_id: step.step_id,
                    status: crate::orchestration::types::StepStatus::InProgress,
                    output: serde_json::json!({
                        "status": "enqueued_individually",
                        "message": "Step enqueued individually to pgmq for autonomous worker processing",
                        "queue_name": queue_name,
                        "message_id": message_id,
                        "namespace": namespace,
                        "sequence": sequence + 1,
                        "timestamp": enqueue_time.to_rfc3339(),
                        "note": "Ruby workers will poll queue and execute step autonomously with immediate delete pattern"
                    }),
                    execution_duration: std::time::Duration::from_millis(1), // Minimal time for enqueueing
                    error_message: None,
                    retry_after: None,
                    error_code: None,
                    error_context: None,
                })
            }
            Err(e) => {
                tracing::error!(
                    task_id = task_id,
                    step_id = step.step_id,
                    namespace = %namespace,
                    queue_name = %queue_name,
                    error = %e,
                    "Failed to enqueue individual step to pgmq"
                );

                Err(OrchestrationError::StepExecutionFailed {
                    step_id: step.step_id,
                    task_id,
                    reason: format!("Failed to enqueue step to {}: {}", queue_name, e),
                    error_code: Some("STEP_ENQUEUE_FAILED".to_string()),
                    retry_after: Some(std::time::Duration::from_secs(30)),
                })
            }
        }
    }

    /// Fetch dependency results for a step
    async fn fetch_step_dependency_results(
        &self,
        step_id: i64,
    ) -> OrchestrationResult<Vec<crate::messaging::message::StepDependencyResult>> {
        // Get the WorkflowStep from step_id
        let workflow_step = crate::models::core::workflow_step::WorkflowStep::find_by_id(
            &self.database_pool,
            step_id,
        )
        .await
        .map_err(|e| OrchestrationError::DatabaseError {
            operation: "load_workflow_step_for_dependencies".to_string(),
            reason: format!("Failed to load workflow step: {}", e),
        })?
        .ok_or_else(|| OrchestrationError::StepExecutionFailed {
            step_id,
            task_id: 0, // Will be filled in by caller
            reason: "Workflow step not found".to_string(),
            error_code: Some("WORKFLOW_STEP_NOT_FOUND".to_string()),
            retry_after: None,
        })?;

        // Get dependencies with their names
        let dependencies_with_names = workflow_step
            .get_dependencies_with_names(&self.database_pool)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "load_step_dependencies".to_string(),
                reason: format!("Failed to load step dependencies: {}", e),
            })?;

        // Convert to StepDependencyResult
        let dependency_results: Vec<crate::messaging::message::StepDependencyResult> =
            dependencies_with_names
                .into_iter()
                .map(|(dep_step, step_name)| {
                    crate::messaging::message::StepDependencyResult::new(
                        step_name,
                        dep_step.workflow_step_id,
                        dep_step.named_step_id,
                        dep_step.results,
                        dep_step.processed_at.map(|dt| dt.and_utc()),
                    )
                    .with_metadata("attempts".to_string(), serde_json::json!(dep_step.attempts))
                    .with_metadata(
                        "retryable".to_string(),
                        serde_json::json!(dep_step.retryable),
                    )
                    .with_metadata(
                        "processed".to_string(),
                        serde_json::json!(dep_step.processed),
                    )
                })
                .collect();

        tracing::debug!(
            step_id = step_id,
            dependency_count = dependency_results.len(),
            "Fetched dependency results for step"
        );

        Ok(dependency_results)
    }

    /// Execute a batch of steps using pgmq queue-based enqueueing (new architecture)
    async fn execute_step_batch_pgmq(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
        pgmq_client: &crate::messaging::PgmqClient,
    ) -> OrchestrationResult<Vec<StepResult>> {
        if steps.is_empty() {
            return Ok(vec![]);
        }

        tracing::info!(
            task_id = task_id,
            step_count = steps.len(),
            "WorkflowCoordinator: Starting pgmq-based batch creation and enqueueing"
        );

        // Group steps by namespace for efficient batch processing
        let mut namespace_groups: std::collections::HashMap<String, Vec<ViableStep>> =
            std::collections::HashMap::new();

        for step in steps {
            let namespace = self.determine_step_namespace(&step).await?;
            namespace_groups
                .entry(namespace)
                .or_insert_with(Vec::new)
                .push(step);
        }

        let mut all_results = Vec::new();

        // Process each namespace group as a separate batch
        for (namespace, namespace_steps) in namespace_groups {
            let batch_results = self
                .create_and_enqueue_batch(task_id, namespace, namespace_steps, pgmq_client)
                .await?;
            all_results.extend(batch_results);
        }

        tracing::info!(
            task_id = task_id,
            total_steps = all_results.len(),
            "WorkflowCoordinator: Completed pgmq batch enqueueing"
        );

        Ok(all_results)
    }

    /// Legacy method - replaced by individual step enqueueing in Phase 5.2
    async fn create_and_enqueue_batch(
        &self,
        _task_id: i64,
        _namespace: String,
        steps: Vec<ViableStep>,
        _pgmq_client: &crate::messaging::PgmqClient,
    ) -> OrchestrationResult<Vec<StepResult>> {
        // Legacy batch system removed in Phase 5.2 - this method is deprecated
        // All functionality moved to enqueue_individual_steps()

        tracing::warn!("create_and_enqueue_batch called but is deprecated - use enqueue_individual_steps instead");

        // Return empty results to maintain interface compatibility
        Ok(steps
            .into_iter()
            .map(|step| StepResult {
                step_id: step.step_id,
                status: crate::orchestration::types::StepStatus::Failed,
                output: serde_json::json!({
                    "error": "Legacy batch system deprecated - use individual step enqueueing"
                }),
                execution_duration: std::time::Duration::from_millis(1),
                error_message: Some("Legacy batch system deprecated in Phase 5.2".to_string()),
                retry_after: None,
                error_code: Some("LEGACY_BATCH_DEPRECATED".to_string()),
                error_context: None,
            })
            .collect())

        /* Legacy batch implementation - commented out in Phase 5.2
        // Create step execution batch record in database
        let new_batch = NewStepExecutionBatch {
            task_id,
            handler_class: format!("{}BatchHandler", namespace),
            batch_uuid: StepExecutionBatch::generate_batch_uuid(),
            initiated_by: Some("orchestrator".to_string()),
            batch_size: steps.len() as i32,
            timeout_seconds: 300, // 5 minutes default timeout
            metadata: Some(serde_json::json!({
                "namespace": namespace,
                "created_by": "workflow_coordinator",
                "enqueue_time": chrono::Utc::now().to_rfc3339(),
                "step_count": steps.len()
            })),
        };

        let batch_record = StepExecutionBatch::create(&self.database_pool, new_batch)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "create_batch_record".to_string(),
                reason: format!("Failed to create batch record: {}", e),
            })?;

        tracing::debug!(
            task_id = task_id,
            batch_id = batch_record.batch_id,
            namespace = %namespace,
            step_count = steps.len(),
            "Created step execution batch record"
        );

        // Get task information for batch context
        let task = crate::models::core::task::Task::find_by_id(&self.database_pool, task_id)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "load_task".to_string(),
                reason: format!("Failed to load task: {}", e),
            })?
            .ok_or_else(|| OrchestrationError::TaskExecutionFailed {
                task_id,
                reason: "Task not found".to_string(),
                error_code: Some("TASK_NOT_FOUND".to_string()),
            })?;

        // Get task orchestration info for namespace
        let task_info = task
            .for_orchestration(&self.database_pool)
            .await
            .map_err(|e| OrchestrationError::DatabaseError {
                operation: "get_task_orchestration_info".to_string(),
                reason: format!("Failed to get task orchestration info: {}", e),
            })?;

        // Create batch steps from viable steps
        let batch_steps: Vec<crate::messaging::orchestration_messages::BatchStep> = steps
            .iter()
            .enumerate()
            .map(
                |(index, step)| crate::messaging::orchestration_messages::BatchStep {
                    step_id: step.step_id,
                    sequence: index as i32 + 1,
                    step_name: step.name.clone(),
                    step_payload: serde_json::json!({
                        "step_id": step.step_id,
                        "task_id": step.task_id,
                        "named_step_id": step.named_step_id,
                        "current_state": step.current_state,
                        "dependencies_satisfied": step.dependencies_satisfied,
                        "retry_eligible": step.retry_eligible,
                        "attempts": step.attempts,
                        "retry_limit": step.retry_limit,
                        "last_failure_at": step.last_failure_at,
                        "next_retry_at": step.next_retry_at,
                        "context": task.context,
                    }),
                    step_metadata: {
                        let mut metadata = std::collections::HashMap::new();
                        metadata.insert(
                            "task_context".to_string(),
                            task.context.clone().unwrap_or(serde_json::Value::Null),
                        );
                        metadata.insert(
                            "namespace".to_string(),
                            serde_json::Value::String(namespace.clone()),
                        );
                        metadata.insert(
                            "batch_id".to_string(),
                            serde_json::Value::Number(batch_record.batch_id.into()),
                        );
                        metadata.insert(
                            "attempts".to_string(),
                            serde_json::Value::Number((step.attempts as i32).into()),
                        );
                        metadata.insert(
                            "retry_limit".to_string(),
                            serde_json::Value::Number((step.retry_limit as i32).into()),
                        );
                        metadata
                    },
                },
            )
            .collect();

        // Create the batch message
        let batch_message = crate::messaging::orchestration_messages::BatchMessage::new(
            batch_record.batch_id,
            task_id,
            namespace.clone(),
            task_info.task_name,
            "1.0.0".to_string(), // TODO: Get actual task version from database
            batch_steps,
        )
        .with_timeout(300); // 5 minutes default timeout

        // Determine the target queue name
        let queue_name = format!("{}_batch_queue", namespace);

        // Enqueue the batch message to the namespace-specific queue
        let enqueue_time = chrono::Utc::now();
        match pgmq_client
            .send_json_message(&queue_name, &batch_message)
            .await
        {
            Ok(message_id) => {
                tracing::info!(
                    task_id = task_id,
                    batch_id = batch_record.batch_id,
                    namespace = %namespace,
                    queue_name = %queue_name,
                    message_id = message_id,
                    step_count = steps.len(),
                    "Successfully enqueued batch to pgmq"
                );

                // Create step results for successful enqueueing
                let results = steps.iter().map(|step| {
                    StepResult {
                        step_id: step.step_id,
                        status: crate::orchestration::types::StepStatus::InProgress,
                        output: serde_json::json!({
                            "status": "enqueued_to_batch_queue",
                            "message": "Step enqueued in batch to pgmq for autonomous worker processing",
                            "queue_name": queue_name.clone(),
                            "batch_id": batch_record.batch_id,
                            "message_id": message_id,
                            "timestamp": enqueue_time.to_rfc3339(),
                            "note": "Ruby batch workers will poll queue and execute batch autonomously"
                        }),
                        execution_duration: std::time::Duration::from_millis(1), // Minimal time for enqueueing
                        error_message: None,
                        retry_after: None,
                        error_code: None,
                        error_context: None,
                    }
                }).collect();

                Ok(results)
            }
            Err(e) => {
                tracing::error!(
                    task_id = task_id,
                    batch_id = batch_record.batch_id,
                    namespace = %namespace,
                    queue_name = %queue_name,
                    error = %e,
                    "Failed to enqueue batch to pgmq"
                );

                // Create step results for failed enqueueing
                let results = steps
                    .iter()
                    .map(|step| StepResult {
                        step_id: step.step_id,
                        status: crate::orchestration::types::StepStatus::Failed,
                        output: serde_json::json!({
                            "status": "batch_enqueue_failed",
                            "message": "Failed to enqueue batch to pgmq",
                            "queue_name": queue_name.clone(),
                            "batch_id": batch_record.batch_id,
                            "timestamp": enqueue_time.to_rfc3339()
                        }),
                        execution_duration: std::time::Duration::from_millis(1),
                        error_message: Some(format!("Batch queue enqueue failed: {}", e)),
                        retry_after: Some(std::time::Duration::from_secs(30)),
                        error_code: Some("BATCH_QUEUE_ENQUEUE_FAILED".to_string()),
                        error_context: Some({
                            let mut context = std::collections::HashMap::new();
                            context.insert(
                                "namespace".to_string(),
                                serde_json::Value::String(namespace.clone()),
                            );
                            context.insert(
                                "queue_name".to_string(),
                                serde_json::Value::String(queue_name.clone()),
                            );
                            context.insert(
                                "batch_id".to_string(),
                                serde_json::Value::Number(batch_record.batch_id.into()),
                            );
                            context.insert(
                                "error".to_string(),
                                serde_json::Value::String(e.to_string()),
                            );
                            context
                        }),
                    })
                    .collect();

                Ok(results)
            }
        }
        */
    }

    /// Determine the namespace for a step based on its name
    async fn determine_step_namespace(&self, step: &ViableStep) -> OrchestrationResult<String> {
        // Extract namespace from step name (e.g., "fulfillment.validate_order" -> "fulfillment")
        let step_namespace = step.name.split('.').next().unwrap_or("default").to_string();

        tracing::debug!(
            step_id = step.step_id,
            step_name = %step.name,
            extracted_namespace = %step_namespace,
            "Determined step namespace from step name"
        );

        Ok(step_namespace)
    }

    // NOTE: finalize_workflow method removed in fire-and-forget architecture
    // Task finalization is now handled by:
    // 1. TaskFinalizer in partial result processing (ResultAggregationHandler::handle_partial_result)
    // 2. TaskFinalizer in batch completion processing (ResultAggregationHandler::handle_batch_completion)
    // This eliminates redundant finalization logic and centralizes it in the appropriate
    // execution paths where actual step results determine task completion status via TCP commands.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_coordinator_config_defaults() {
        let config = WorkflowCoordinatorConfig::default();
        assert_eq!(config.max_discovery_attempts, 3);
        assert_eq!(config.discovery_retry_delay, Duration::from_secs(1));
        assert_eq!(config.max_workflow_duration, Duration::from_secs(3600));
        assert!(config.enable_metrics);
        assert_eq!(config.max_steps_per_run, 1000);
        assert_eq!(config.step_batch_size, 10);
    }
}
